//! The attachment cache (design §5.6): files the app's image views and the
//! share sheet read by path, at
//! `<cache_dir>/attachments/<16 hex of sha256(server key)>/<id>.<o|t>`.
//!
//! A file is fetched into a unique `.part` name and renamed into place, so
//! a reader never sees half a file and two callers may race. The files are
//! plain: the phone's app sandbox and disk encryption guard them.

use std::future::Future;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};

use crate::MobileError;
use crate::types::AttachmentVariant;

/// Names `.part` files apart within this process.
static NEXT_PART: AtomicU64 = AtomicU64::new(0);

/// The folder of one server's files.
pub(crate) fn server_folder(cache_dir: &Path, server_key: &str) -> PathBuf {
    let digest = Sha256::digest(server_key.as_bytes());
    cache_dir
        .join("attachments")
        .join(&hex::encode(digest)[..16])
}

pub(crate) fn file_path(
    cache_dir: &Path,
    server_key: &str,
    attachment_id: i64,
    variant: AttachmentVariant,
) -> PathBuf {
    let suffix = match variant {
        AttachmentVariant::Original => "o",
        AttachmentVariant::Thumbnail => "t",
    };
    server_folder(cache_dir, server_key).join(format!("{attachment_id}.{suffix}"))
}

/// A `.part` path beside `target`, used by no one else.
fn part_path(target: &Path) -> PathBuf {
    let part = NEXT_PART.fetch_add(1, Ordering::Relaxed);
    let mut name = target.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}-{part}.part", std::process::id()));
    target.with_file_name(name)
}

/// `target` when it is cached; otherwise `fetch` writes it to the `.part`
/// path it is given, which is then renamed into place. A failed fetch
/// leaves nothing behind.
pub(crate) async fn cached<F, Fut>(target: PathBuf, fetch: F) -> Result<PathBuf, MobileError>
where
    F: FnOnce(PathBuf) -> Fut,
    Fut: Future<Output = Result<(), MobileError>>,
{
    match tokio::fs::try_exists(&target).await {
        Ok(true) => return Ok(target),
        Ok(false) => {}
        // Unreadable: fetch it again, and let the write say what is wrong.
        Err(error) => tracing::warn!(%error, "the attachment cache could not be read"),
    }
    if let Some(folder) = target.parent() {
        tokio::fs::create_dir_all(folder)
            .await
            .map_err(|error| storage(&error))?;
    }
    let part = part_path(&target);
    let fetched = match fetch(part.clone()).await {
        Ok(()) => tokio::fs::rename(&part, &target)
            .await
            .map_err(|error| storage(&error)),
        Err(error) => Err(error),
    };
    if let Err(error) = fetched {
        discard(&part).await;
        return Err(error);
    }
    Ok(target)
}

async fn discard(part: &Path) {
    if let Err(error) = tokio::fs::remove_file(part).await
        && error.kind() != ErrorKind::NotFound
    {
        tracing::warn!(%error, "a partial attachment could not be removed");
    }
}

pub(crate) fn storage(error: &std::io::Error) -> MobileError {
    MobileError::Storage {
        message: error.to_string(),
    }
}

/// A path as JavaScript takes it.
pub(crate) fn path_text(path: PathBuf) -> Result<String, MobileError> {
    path.into_os_string()
        .into_string()
        .map_err(|path| MobileError::Storage {
            message: format!("not a UTF-8 path: {path:?}"),
        })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use super::*;

    #[test]
    fn server_folders_are_hashed() {
        let folder = server_folder(Path::new("/cache"), "10.0.2.2:7710");
        let digest = hex::encode(Sha256::digest(b"10.0.2.2:7710"));
        assert_eq!(folder, Path::new("/cache/attachments").join(&digest[..16]));
        let name = folder.file_name().unwrap().to_str().unwrap();
        assert_eq!(name.len(), 16);
        assert!(
            name.bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
        );
    }

    #[tokio::test]
    async fn a_cached_file_comes_back_without_fetching() {
        let dir = tempfile::tempdir().unwrap();
        let target = file_path(dir.path(), "s", 5, AttachmentVariant::Thumbnail);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, b"cached").unwrap();
        let fetched = AtomicBool::new(false);
        let path = cached(target.clone(), |_| async {
            fetched.store(true, Ordering::SeqCst);
            Ok(())
        })
        .await
        .unwrap();
        assert_eq!(path, target);
        assert!(!fetched.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn a_fetched_file_lands_whole() {
        let dir = tempfile::tempdir().unwrap();
        let target = file_path(dir.path(), "s", 6, AttachmentVariant::Original);
        let path = cached(target.clone(), |part| async move {
            tokio::fs::write(&part, b"whole")
                .await
                .map_err(|error| storage(&error))
        })
        .await
        .unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"whole");
        let names: Vec<_> = std::fs::read_dir(target.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["6.o"]);
    }

    #[tokio::test]
    async fn a_failed_fetch_leaves_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = file_path(dir.path(), "s", 7, AttachmentVariant::Thumbnail);
        let error = cached(target.clone(), |part| async move {
            // Half written, then the connection broke.
            tokio::fs::write(&part, b"half")
                .await
                .map_err(|error| storage(&error))?;
            Err(MobileError::Timeout)
        })
        .await
        .unwrap_err();
        assert_eq!(error, MobileError::Timeout);
        assert!(!target.exists());
        assert!(
            std::fs::read_dir(target.parent().unwrap())
                .unwrap()
                .next()
                .is_none(),
            "a .part file was left"
        );
    }
}
