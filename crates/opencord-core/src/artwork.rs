//! Music artwork on this device (Phase 3 plan §15.4, §19.10): fetched only
//! from the server's artwork proxy, never from the sites themselves, and
//! kept on disk for every server at once, at most 100 MB, the least
//! recently shown going first. A file's modification time is when it was
//! last shown or fetched.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::SystemTime;

use crate::api::types::{CoreError, ErrorCode};
use crate::client::Client;
use crate::http::MediaHttp;

/// Inside the app's data directory.
const DIR: &str = "music-artwork";
const CACHE_BYTES: u64 = 100 * 1024 * 1024;
/// The proxy's pictures are small JPEGs.
const RESPONSE_BYTES: usize = 2 * 1024 * 1024;
/// The sizes the server makes: list thumbnails and the cover.
pub const SIZES: [u32; 2] = [128, 512];

pub(crate) struct ArtworkCache {
    dir: PathBuf,
    limit: u64,
    media: MediaHttp,
    /// One fetch per picture at a time; others wait for it.
    fetching: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl ArtworkCache {
    pub fn new(data_dir: &Path, media: MediaHttp) -> Self {
        Self {
            dir: data_dir.join(DIR),
            limit: CACHE_BYTES,
            media,
            fetching: Mutex::default(),
        }
    }

    /// The path of the picture's file, fetched from the server at `key`
    /// unless it is here already.
    pub async fn get(
        &self,
        client: &Client,
        key: &str,
        artwork_id: &str,
        size: u32,
    ) -> Result<String, CoreError> {
        let valid_id = artwork_id.len() == 32 && artwork_id.bytes().all(|b| b.is_ascii_hexdigit());
        if !valid_id || !SIZES.contains(&size) {
            return Err(CoreError::InvalidInput {
                message: format!("{artwork_id} at {size} px is not artwork"),
            });
        }
        let name = format!("{}-{size}.jpg", artwork_id.to_ascii_lowercase());
        let path = self.dir.join(&name);
        if touch(&path) {
            return Ok(path.to_string_lossy().into_owned());
        }
        let turn = Arc::clone(
            self.fetching
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(name.clone())
                .or_default(),
        );
        let _turn = turn.lock().await;
        let fetched = if touch(&path) {
            Ok(())
        } else {
            self.fetch(client, key, artwork_id, size, &path).await
        };
        self.fetching
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&name);
        fetched.map(|()| path.to_string_lossy().into_owned())
    }

    async fn fetch(
        &self,
        client: &Client,
        key: &str,
        artwork_id: &str,
        size: u32,
        path: &Path,
    ) -> Result<(), CoreError> {
        let response = self
            .media
            .request(
                client,
                key,
                "GET",
                &format!("/media/music/artwork/{artwork_id}?size={size}"),
                &[],
                RESPONSE_BYTES,
            )
            .await?;
        if response.status != 200 || !response.body.starts_with(&[0xff, 0xd8]) {
            return Err(CoreError::Server {
                code: ErrorCode::NotFound,
                message: "the artwork is not available".to_owned(),
                retry_after_ms: None,
            });
        }
        let (dir, path, limit) = (self.dir.clone(), path.to_owned(), self.limit);
        tokio::task::spawn_blocking(move || store(&dir, &path, &response.body, limit))
            .await
            .map_err(|error| CoreError::Storage {
                message: error.to_string(),
            })?
            .map_err(|error| CoreError::Storage {
                message: error.to_string(),
            })
    }
}

/// Whether the file is here; if so, it was just used. Shared with the
/// avatar cache.
pub(crate) fn touch(path: &Path) -> bool {
    let Ok(file) = fs::File::options().write(true).open(path) else {
        return false;
    };
    let _ = file.set_modified(SystemTime::now());
    true
}

/// Writes the picture, then removes the least recently used until the
/// rest fit.
pub(crate) fn store(dir: &Path, path: &Path, bytes: &[u8], limit: u64) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let partial = path.with_extension("partial");
    fs::write(&partial, bytes)?;
    fs::rename(&partial, path)?;
    let mut files: Vec<(SystemTime, u64, PathBuf)> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            metadata.is_file().then(|| {
                let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                (modified, metadata.len(), entry.path())
            })
        })
        .collect();
    let mut total: u64 = files.iter().map(|(_, size, _)| size).sum();
    files.sort();
    for (_, size, file) in files {
        if total <= limit {
            break;
        }
        if file == path {
            continue;
        }
        fs::remove_file(&file)?;
        total -= size;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn the_least_recently_shown_go_first() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("old-128.jpg");
        let shown = dir.path().join("shown-128.jpg");
        fs::write(&old, [0u8; 60]).unwrap();
        fs::write(&shown, [0u8; 60]).unwrap();
        let hour_ago = SystemTime::now() - Duration::from_secs(3600);
        for path in [&old, &shown] {
            fs::File::options()
                .write(true)
                .open(path)
                .unwrap()
                .set_modified(hour_ago - Duration::from_secs(u64::from(path == &old)))
                .unwrap();
        }
        assert!(touch(&shown));
        let new = dir.path().join("new-128.jpg");
        store(dir.path(), &new, &[0u8; 60], 150).unwrap();
        assert!(!old.exists());
        assert!(shown.exists() && new.exists());
        assert!(!touch(&dir.path().join("missing-128.jpg")));
    }
}
