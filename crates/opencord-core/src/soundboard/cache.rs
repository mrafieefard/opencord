//! Soundboard files on this device (plan §11.2, §11.4): one cache for every
//! server, named by SHA-256 so a sound on several servers is kept once,
//! at most 200 MB, the least recently played going first. A file's
//! modification time is when it was last played or fetched.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use sha2::{Digest, Sha256};

/// Inside the app's data directory.
const DIR: &str = "sounds";

#[derive(Debug, Clone)]
pub struct SoundCache {
    dir: PathBuf,
    limit: u64,
}

impl SoundCache {
    pub fn new(data_dir: &Path, limit: u64) -> Self {
        Self {
            dir: data_dir.join(DIR),
            limit,
        }
    }

    fn path(&self, sha256: &[u8; 32]) -> PathBuf {
        self.dir.join(format!("{}.opus", hex::encode(sha256)))
    }

    pub fn contains(&self, sha256: &[u8; 32]) -> bool {
        self.path(sha256).is_file()
    }

    /// The file, if it is here and intact; a damaged one is removed. Its
    /// time is updated, so it is kept longer.
    pub fn get(&self, sha256: &[u8; 32]) -> Option<Vec<u8>> {
        let path = self.path(sha256);
        let bytes = fs::read(&path).ok()?;
        if Sha256::digest(&bytes).as_slice() != sha256 {
            let _ = fs::remove_file(&path);
            return None;
        }
        if let Ok(file) = fs::File::options().write(true).open(&path) {
            let _ = file.set_modified(SystemTime::now());
        }
        Some(bytes)
    }

    /// Keeps `bytes` if they hash to `sha256`, then trims the cache to its
    /// limit. Returns whether they did.
    pub fn put(&self, sha256: &[u8; 32], bytes: &[u8]) -> io::Result<bool> {
        if Sha256::digest(bytes).as_slice() != sha256 {
            return Ok(false);
        }
        fs::create_dir_all(&self.dir)?;
        let path = self.path(sha256);
        let partial = path.with_extension("opus.partial");
        fs::write(&partial, bytes)?;
        fs::rename(&partial, &path)?;
        self.trim(sha256)?;
        Ok(true)
    }

    /// Removes the least recently used files until the rest fit, never
    /// `keep`.
    fn trim(&self, keep: &[u8; 32]) -> io::Result<()> {
        let keep = self.path(keep);
        let mut files: Vec<(SystemTime, u64, PathBuf)> = fs::read_dir(&self.dir)?
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
        for (_, size, path) in files {
            if total <= self.limit {
                break;
            }
            if path == keep {
                continue;
            }
            fs::remove_file(&path)?;
            total -= size;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn hash(bytes: &[u8]) -> [u8; 32] {
        Sha256::digest(bytes).into()
    }

    fn age(cache: &SoundCache, sha256: &[u8; 32], seconds: u64) {
        let file = fs::File::options()
            .write(true)
            .open(cache.path(sha256))
            .unwrap();
        file.set_modified(SystemTime::now() - Duration::from_secs(seconds))
            .unwrap();
    }

    #[test]
    fn a_file_comes_back_only_as_it_was_stored() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SoundCache::new(dir.path(), 1_000);
        let sound = b"a sound".to_vec();

        assert!(!cache.put(&hash(b"something else"), &sound).unwrap());
        assert!(cache.put(&hash(&sound), &sound).unwrap());
        assert_eq!(cache.get(&hash(&sound)), Some(sound.clone()));

        // A file damaged on disk is not played, and is fetched again.
        fs::write(cache.path(&hash(&sound)), b"a sounD").unwrap();
        assert_eq!(cache.get(&hash(&sound)), None);
        assert!(!cache.contains(&hash(&sound)));
    }

    #[test]
    fn the_least_recently_played_go_when_the_cache_is_full() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SoundCache::new(dir.path(), 25);
        let (old, played, new) = (vec![1; 10], vec![2; 10], vec![3; 10]);
        cache.put(&hash(&old), &old).unwrap();
        cache.put(&hash(&played), &played).unwrap();
        age(&cache, &hash(&old), 60);
        age(&cache, &hash(&played), 30);
        cache.get(&hash(&played));

        cache.put(&hash(&new), &new).unwrap();

        assert!(!cache.contains(&hash(&old)));
        assert!(cache.contains(&hash(&played)));
        assert!(cache.contains(&hash(&new)));
    }
}
