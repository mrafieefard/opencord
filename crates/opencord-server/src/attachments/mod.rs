//! Media attachments and storage backends (Phase 4).

pub mod gc;
pub mod http;
pub mod images;
pub mod process;
pub mod range;
pub mod settings;
pub mod storage;
pub mod temp;
pub mod voice;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex as StdMutex, PoisonError, RwLock};

use opencord_common::blob::BlobKey;
use sqlx::SqliteConnection;
use tokio::sync::{Mutex as TokioMutex, OwnedMutexGuard, Semaphore};

use crate::config::Config;
use crate::db::attachments::AttachmentRow;
use crate::db::storage::StorageRow;
use crate::error::ApiError;
use crate::secrets::Secrets;
pub use settings::UploadSettings;
pub use storage::{
    Backend, LocalStorage, ObjectKind, S3Settings, S3Storage, StorageError, object_key,
};

#[derive(Debug)]
#[allow(dead_code)]
pub struct Attachments {
    temp_dir: PathBuf,
    max_file_bytes: u64,
    backends: RwLock<HashMap<i64, Arc<Backend>>>,
    active: AtomicI64,
    jobs: Arc<Semaphore>, // 2 permits
    upload_locks: StdMutex<HashMap<i64, Arc<TokioMutex<()>>>>,
    posters: StdMutex<HashMap<i64, Vec<u8>>>,
    hold_processing: AtomicBool, // test hook
    http: reqwest::Client,
}

impl Attachments {
    pub fn new(config: &Config, http: reqwest::Client) -> Self {
        let mut backends = HashMap::new();
        backends.insert(
            1,
            Arc::new(Backend::Local(LocalStorage::new(config.attachments_dir()))),
        );
        Self {
            temp_dir: config.uploads_temp_dir(),
            max_file_bytes: config.max_file_bytes(),
            backends: RwLock::new(backends),
            active: AtomicI64::new(1),
            jobs: Arc::new(Semaphore::new(2)),
            upload_locks: StdMutex::new(HashMap::new()),
            posters: StdMutex::new(HashMap::new()),
            hold_processing: AtomicBool::new(false),
            http,
        }
    }

    /// Builds the S3 backends and restores the active one. A storage that
    /// can't be built stops the server: its files would be unreadable.
    pub async fn load(&self, conn: &mut SqliteConnection, secrets: &Secrets) -> anyhow::Result<()> {
        let rows = crate::db::storage::list(conn).await?;
        let mut loaded = Vec::new();
        for row in rows.iter().filter(|row| row.kind == "s3") {
            let settings = s3_settings(row, secrets)
                .map_err(|error| anyhow::anyhow!("storage \"{}\": {error}", row.name))?;
            let storage = S3Storage::new(&settings, self.http.clone())
                .map_err(|error| anyhow::anyhow!("storage \"{}\": {error}", row.name))?;
            loaded.push((row.id, Arc::new(Backend::S3(Box::new(storage)))));
        }
        let active = rows.iter().find(|row| row.active).map_or(1, |row| row.id);
        self.backends
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .extend(loaded);
        self.active.store(active, Ordering::Release);
        Ok(())
    }

    /// Adds or replaces a backend. Callers write the database first.
    pub fn set_backend(&self, id: i64, backend: Backend) {
        self.backends
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, Arc::new(backend));
    }

    pub fn remove_backend(&self, id: i64) {
        self.backends
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&id);
    }

    /// New uploads go to `id` from now on. Its backend must be loaded.
    pub fn set_active(&self, id: i64) {
        self.active.store(id, Ordering::Release);
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    pub fn active_id(&self) -> i64 {
        self.active.load(Ordering::Acquire)
    }

    pub fn active(&self) -> Result<(i64, Arc<Backend>), ApiError> {
        let id = self.active_id();
        let backend = self.backend(id).ok_or_else(|| {
            tracing::error!(storage_id = id, "the active storage isn't loaded");
            ApiError::internal()
        })?;
        Ok((id, backend))
    }

    pub fn backend(&self, id: i64) -> Option<Arc<Backend>> {
        self.backends
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&id)
            .cloned()
    }

    pub fn temp_path(&self, id: i64) -> PathBuf {
        self.temp_dir.join(format!("{id}.part"))
    }

    pub fn temp_dir(&self) -> &Path {
        &self.temp_dir
    }

    pub fn max_file_bytes(&self) -> u64 {
        self.max_file_bytes
    }

    pub async fn lock_upload(&self, id: i64) -> OwnedMutexGuard<()> {
        let lock = {
            let mut locks = self
                .upload_locks
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            // Forget locks nobody holds or waits for, so the map stays as
            // small as the number of uploads in flight.
            locks.retain(|_, lock| Arc::strong_count(lock) > 1);
            locks
                .entry(id)
                .or_insert_with(|| Arc::new(TokioMutex::new(())))
                .clone()
        };
        lock.lock_owned().await
    }

    pub fn hold_processing(&self, hold: bool) {
        self.hold_processing.store(hold, Ordering::Relaxed);
    }

    pub fn is_processing_held(&self) -> bool {
        self.hold_processing.load(Ordering::Relaxed)
    }

    pub fn set_poster(&self, id: i64, poster: Vec<u8>) {
        self.posters
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, poster);
    }

    pub fn take_poster(&self, id: i64) -> Option<Vec<u8>> {
        self.posters
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&id)
    }

    pub fn jobs(&self) -> Arc<Semaphore> {
        self.jobs.clone()
    }
}

/// What an S3 storage's secret key is sealed under in the database.
pub(crate) fn storage_secret_label(id: i64) -> String {
    format!("storage-secret:{id}")
}

/// An S3 row's settings, with its secret opened.
pub(crate) fn s3_settings(row: &StorageRow, secrets: &Secrets) -> Result<S3Settings, String> {
    let sealed = row
        .secret_enc
        .as_deref()
        .ok_or_else(|| "it has no secret key".to_owned())?;
    let secret_access_key = secrets
        .open_string(&storage_secret_label(row.id), sealed)
        .map_err(|_| {
            "its secret key couldn't be opened with this server's secrets file".to_owned()
        })?;
    Ok(S3Settings {
        endpoint: row.endpoint.clone().unwrap_or_default(),
        region: row.region.clone().unwrap_or_default(),
        bucket: row.bucket.clone().unwrap_or_default(),
        key_prefix: row.key_prefix.clone(),
        path_style: row.path_style,
        allow_http: row.allow_http,
        access_key_id: row.access_key_id.clone().unwrap_or_default(),
        secret_access_key,
    })
}

/// What an attachment's key is sealed under in the database.
pub(crate) fn key_label(id: i64) -> String {
    format!("attachment-key:{id}")
}

/// The key that encrypts an attachment's file and thumbnail.
pub(crate) fn open_key(secrets: &Secrets, row: &AttachmentRow) -> Result<BlobKey, ApiError> {
    let bytes = secrets
        .open(&key_label(row.id), &row.key_enc)
        .map_err(|error| {
            tracing::error!(
                attachment_id = row.id,
                ?error,
                "an attachment key failed to open"
            );
            ApiError::internal()
        })?;
    let bytes: [u8; 32] = bytes.try_into().map_err(|_| ApiError::internal())?;
    Ok(BlobKey::from_bytes(bytes))
}

pub async fn start(state: &Arc<crate::state::AppState>) -> anyhow::Result<()> {
    let mut conn = state.db.acquire().await?;
    let uploading = crate::db::attachments::in_state(&mut conn, "uploading").await?;
    let processing = crate::db::attachments::in_state(&mut conn, "processing").await?;
    drop(conn);

    // Temp files with no uploading or processing row are left from uploads
    // that were cancelled or expired while the server was down.
    let live: std::collections::HashSet<i64> = uploading
        .iter()
        .chain(&processing)
        .map(|row| row.id)
        .collect();
    let temp_dir = state.attachments.temp_dir();
    if let Ok(mut entries) = tokio::fs::read_dir(temp_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let is_orphan = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|name| name.strip_suffix(".part"))
                .and_then(|id_str| id_str.parse::<i64>().ok())
                .is_some_and(|id| !live.contains(&id));
            if is_orphan {
                tokio::fs::remove_file(&path).await.ok();
            }
        }
    }

    for row in processing {
        process::spawn_job(state.clone(), row.id);
    }
    gc::start(state);
    Ok(())
}
