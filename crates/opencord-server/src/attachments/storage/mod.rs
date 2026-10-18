//! Storage backends for media attachments.

pub mod local;

pub use local::LocalStorage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    File,
    Thumbnail,
}

pub fn object_key(kind: ObjectKind, id: i64) -> String {
    match kind {
        ObjectKind::File => format!("a/{id}"),
        ObjectKind::Thumbnail => format!("t/{id}"),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("not found")]
    NotFound,
    #[error("storage is unavailable: {0}")]
    Unavailable(String),
    #[error("storage is full")]
    Full,
}

impl From<std::io::Error> for StorageError {
    fn from(err: std::io::Error) -> Self {
        if err.kind() == std::io::ErrorKind::NotFound {
            StorageError::NotFound
        } else if err.kind() == std::io::ErrorKind::StorageFull || err.raw_os_error() == Some(28) {
            StorageError::Full
        } else {
            StorageError::Unavailable(err.to_string())
        }
    }
}

#[derive(Debug)]
pub enum Backend {
    Local(LocalStorage),
}

impl Backend {
    pub async fn put_file(&self, key: &str, path: &std::path::Path) -> Result<(), StorageError> {
        match self {
            Self::Local(local) => local.put_file(key, path).await,
        }
    }

    pub async fn put_bytes(&self, key: &str, bytes: Vec<u8>) -> Result<(), StorageError> {
        match self {
            Self::Local(local) => local.put_bytes(key, bytes).await,
        }
    }

    pub async fn get_range(
        &self,
        key: &str,
        start: u64,
        end: u64,
    ) -> Result<bytes::Bytes, StorageError> {
        match self {
            Self::Local(local) => local.get_range(key, start, end).await,
        }
    }

    pub async fn size(&self, key: &str) -> Result<Option<u64>, StorageError> {
        match self {
            Self::Local(local) => local.size(key).await,
        }
    }

    pub async fn delete(&self, key: &str) -> Result<(), StorageError> {
        match self {
            Self::Local(local) => local.delete(key).await,
        }
    }
}
