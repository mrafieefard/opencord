//! Attachments kept on this device (media uploads plan, Task 19): one cache
//! for every server, at most 1 GiB, the least recently shown going first.
//! Each file is sealed with a key the app keeps in the system keychain, so
//! the folder holds no readable file. A file's modification time is when
//! it was last shown or fetched.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use opencord_common::blob::{self, BlobKey, BlobKind};
use sha2::{Digest, Sha256};

use crate::api::types::AttachmentVariant;

/// Inside the app's data directory.
pub(crate) const DIR: &str = "media-cache";
pub(crate) const LIMIT: u64 = 1024 * 1024 * 1024;

/// One attachment's place in the cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    /// The hash's 64 hex digits.
    name: String,
    /// What it is sealed under: the hash's first 8 bytes. A file copied
    /// over another entry's name doesn't open.
    blob_id: i64,
}

impl Entry {
    pub fn new(server_key: &str, attachment_id: i64, variant: AttachmentVariant) -> Self {
        let variant = match variant {
            AttachmentVariant::Original => "original",
            AttachmentVariant::Thumbnail => "thumbnail",
        };
        let hash = Sha256::digest(format!("{server_key}\n{attachment_id}\n{variant}"));
        let mut id = [0u8; 8];
        id.copy_from_slice(&hash[..8]);
        Self {
            name: hex::encode(hash),
            blob_id: i64::from_be_bytes(id),
        }
    }
}

pub(crate) struct MediaCache {
    dir: PathBuf,
    key: BlobKey,
    limit: u64,
}

impl MediaCache {
    pub fn open(dir: PathBuf, key: BlobKey, limit: u64) -> io::Result<Self> {
        fs::create_dir_all(&dir)?;
        Ok(Self { dir, key, limit })
    }

    /// The entry's bytes, if it is here and opens. One that doesn't open is
    /// removed. Its time is updated, so it is kept longer.
    pub fn get(&self, entry: &Entry) -> Option<Vec<u8>> {
        let path = self.dir.join(&entry.name);
        let sealed = fs::read(&path).ok()?;
        match blob::open_all(&self.key, BlobKind::Cache, entry.blob_id, &sealed) {
            Ok(plain) => {
                touch(&path);
                Some(plain)
            }
            Err(error) => {
                tracing::debug!(?error, "a cached file didn't open; removing it");
                remove(&path);
                None
            }
        }
    }

    /// Keeps `plain` sealed, then trims the cache to its limit.
    pub fn put(&self, entry: &Entry, plain: &[u8]) -> io::Result<()> {
        let sealed = blob::seal_all(&self.key, BlobKind::Cache, entry.blob_id, plain);
        let path = self.dir.join(&entry.name);
        let partial = self.dir.join(format!("{}.partial", entry.name));
        fs::write(&partial, sealed)?;
        fs::rename(&partial, &path)?;
        self.trim(&path)
    }

    pub fn size(&self) -> io::Result<u64> {
        Ok(self.entries()?.iter().map(|(_, size, _)| size).sum())
    }

    pub fn clear(&self) -> io::Result<()> {
        for (_, _, path) in self.entries()? {
            remove_file(&path)?;
        }
        Ok(())
    }

    /// Removes the least recently used entries until the rest fit, never
    /// `keep`.
    fn trim(&self, keep: &Path) -> io::Result<()> {
        let mut entries = self.entries()?;
        let mut total: u64 = entries.iter().map(|(_, size, _)| size).sum();
        entries.sort();
        for (_, size, path) in entries {
            if total <= self.limit {
                break;
            }
            if path == keep {
                continue;
            }
            remove_file(&path)?;
            total -= size;
        }
        Ok(())
    }

    /// Every entry with its time and size. Files being written (`.partial`)
    /// aren't entries yet.
    fn entries(&self) -> io::Result<Vec<(SystemTime, u64, PathBuf)>> {
        let mut entries = Vec::new();
        for item in fs::read_dir(&self.dir)? {
            let item = item?;
            let is_entry = item.file_name().to_str().is_some_and(|name| {
                name.len() == 64 && name.bytes().all(|b| b.is_ascii_hexdigit())
            });
            let metadata = item.metadata()?;
            if is_entry && metadata.is_file() {
                let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                entries.push((modified, metadata.len(), item.path()));
            }
        }
        Ok(entries)
    }
}

/// Removed already counts as removed.
fn remove_file(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

fn remove(path: &Path) {
    if let Err(error) = remove_file(path) {
        tracing::warn!(%error, "a cached file couldn't be removed");
    }
}

/// Marks the file as just used.
fn touch(path: &Path) {
    let touched = fs::File::options()
        .write(true)
        .open(path)
        .and_then(|file| file.set_modified(SystemTime::now()));
    if let Err(error) = touched {
        tracing::debug!(%error, "a cached file's time couldn't be updated");
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn cache(dir: &Path, limit: u64) -> MediaCache {
        MediaCache::open(dir.join(DIR), BlobKey::from_bytes([7; 32]), limit).unwrap()
    }

    fn entry(id: i64) -> Entry {
        Entry::new("127.0.0.1:8443", id, AttachmentVariant::Original)
    }

    fn age(cache: &MediaCache, entry: &Entry, seconds: u64) {
        fs::File::options()
            .write(true)
            .open(cache.dir.join(&entry.name))
            .unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(seconds))
            .unwrap();
    }

    #[test]
    fn cache_round_trips_and_evicts_oldest() {
        let dir = tempfile::tempdir().unwrap();
        let one = blob::sealed_len(1_000);
        let cache = cache(dir.path(), one * 3);
        for id in 1..=3 {
            cache.put(&entry(id), &[id as u8; 1_000]).unwrap();
            age(&cache, &entry(id), 100 - id as u64);
        }
        assert_eq!(cache.get(&entry(2)), Some(vec![2; 1_000]));

        cache.put(&entry(4), &[4; 1_000]).unwrap();

        assert_eq!(cache.get(&entry(1)), None);
        assert_eq!(cache.get(&entry(2)), Some(vec![2; 1_000]));
        assert_eq!(cache.get(&entry(4)), Some(vec![4; 1_000]));
        assert_eq!(cache.size().unwrap(), one * 3);
    }

    #[test]
    fn cache_files_hold_no_plaintext() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache(dir.path(), LIMIT);
        let marker = b"a very recognisable marker string";
        cache.put(&entry(1), marker).unwrap();

        for item in fs::read_dir(&cache.dir).unwrap() {
            let bytes = fs::read(item.unwrap().path()).unwrap();
            assert!(!bytes.windows(marker.len()).any(|window| window == marker));
        }
    }

    #[test]
    fn a_cache_file_cannot_be_renamed_to_another_entry() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache(dir.path(), LIMIT);
        let (a, b) = (entry(1), entry(2));
        cache.put(&a, b"picture a").unwrap();
        cache.put(&b, b"picture b").unwrap();

        fs::copy(cache.dir.join(&a.name), cache.dir.join(&b.name)).unwrap();

        assert_eq!(cache.get(&b), None);
        assert!(!cache.dir.join(&b.name).exists());
        assert_eq!(cache.get(&a), Some(b"picture a".to_vec()));
    }

    #[test]
    fn clear_empties_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache(dir.path(), LIMIT);
        cache.put(&entry(1), b"one").unwrap();
        cache.put(&entry(2), b"two").unwrap();

        cache.clear().unwrap();

        assert_eq!(cache.size().unwrap(), 0);
        assert_eq!(cache.get(&entry(1)), None);
    }
}
