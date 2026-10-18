use bytes::Bytes;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

use super::StorageError;

#[derive(Debug)]
pub struct LocalStorage {
    root: PathBuf,
}

impl LocalStorage {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn path(&self, key: &str) -> Result<PathBuf, StorageError> {
        let (prefix, id_str) = key
            .split_once('/')
            .ok_or_else(|| StorageError::Unavailable("bad key".to_string()))?;
        if prefix != "a" && prefix != "t" {
            return Err(StorageError::Unavailable("bad key".to_string()));
        }
        let id: i64 = id_str
            .parse()
            .map_err(|_| StorageError::Unavailable("bad key".to_string()))?;
        if id < 0 {
            return Err(StorageError::Unavailable("bad key".to_string()));
        }
        let shard = format!("{:02x}", (id >> 22) & 0xff);
        Ok(self.root.join(prefix).join(shard).join(id.to_string()))
    }

    pub async fn put_file(&self, key: &str, path: &Path) -> Result<(), StorageError> {
        let dest = self.path(key)?;
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        match tokio::fs::rename(path, &dest).await {
            Ok(()) => Ok(()),
            Err(err)
                if err.raw_os_error() == Some(18)
                    || err.kind() == std::io::ErrorKind::CrossesDevices =>
            {
                let mut src = tokio::fs::File::open(path).await?;
                let mut dst = tokio::fs::File::create(&dest).await?;
                tokio::io::copy(&mut src, &mut dst).await?;
                dst.sync_all().await?;
                drop(src);
                drop(dst);
                tokio::fs::remove_file(path).await?;
                Ok(())
            }
            Err(err) => Err(err.into()),
        }
    }

    pub async fn put_bytes(&self, key: &str, bytes: Vec<u8>) -> Result<(), StorageError> {
        let dest = self.path(key)?;
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut file = tokio::fs::File::create(&dest).await?;
        file.write_all(&bytes).await?;
        file.sync_all().await?;
        Ok(())
    }

    pub async fn get_range(&self, key: &str, start: u64, end: u64) -> Result<Bytes, StorageError> {
        let path = self.path(key)?;
        let mut file = tokio::fs::File::open(&path).await?;
        if end <= start {
            return Ok(Bytes::new());
        }
        let len = (end - start) as usize;
        file.seek(std::io::SeekFrom::Start(start)).await?;
        let mut buf = vec![0u8; len];
        file.read_exact(&mut buf).await?;
        Ok(Bytes::from(buf))
    }

    pub async fn size(&self, key: &str) -> Result<Option<u64>, StorageError> {
        let path = self.path(key)?;
        match tokio::fs::metadata(&path).await {
            Ok(meta) => Ok(Some(meta.len())),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    pub async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let path = self.path(key)?;
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_round_trip_and_ranges() {
        let dir = tempfile::tempdir().unwrap();
        let storage = LocalStorage::new(dir.path().to_path_buf());

        storage
            .put_bytes("a/123", b"0123456789".to_vec())
            .await
            .unwrap();

        let range = storage.get_range("a/123", 2, 5).await.unwrap();
        assert_eq!(range.as_ref(), b"234");

        let size = storage.size("a/123").await.unwrap();
        assert_eq!(size, Some(10));

        storage.delete("a/123").await.unwrap();
        storage.delete("a/123").await.unwrap();

        let size_after = storage.size("a/123").await.unwrap();
        assert_eq!(size_after, None);
    }

    #[tokio::test]
    async fn put_file_moves_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let storage = LocalStorage::new(dir.path().to_path_buf());

        let src = dir.path().join("source.dat");
        tokio::fs::write(&src, b"hello world").await.unwrap();

        storage.put_file("a/456", &src).await.unwrap();
        assert!(!src.exists());

        let content = storage.get_range("a/456", 0, 11).await.unwrap();
        assert_eq!(content.as_ref(), b"hello world");
    }

    #[tokio::test]
    async fn keys_cannot_escape_the_root() {
        let dir = tempfile::tempdir().unwrap();
        let storage = LocalStorage::new(dir.path().to_path_buf());

        assert!(storage.path("../x").is_err());
        assert!(storage.path("a/../../x").is_err());
        assert!(storage.path("z/1").is_err());
    }
}
