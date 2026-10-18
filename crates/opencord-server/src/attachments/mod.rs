//! Media attachments and storage backends (Phase 4).

pub mod http;
pub mod settings;
pub mod storage;
pub mod temp;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex as StdMutex, RwLock};

use sqlx::SqliteConnection;
use tokio::sync::{Mutex as TokioMutex, OwnedMutexGuard, Semaphore};

use crate::config::Config;
use crate::secrets::Secrets;
pub use settings::UploadSettings;
pub use storage::{Backend, LocalStorage, ObjectKind, StorageError, object_key};

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

    pub async fn load(
        &self,
        _conn: &mut SqliteConnection,
        _secrets: &Secrets,
    ) -> Result<(), anyhow::Error> {
        // Milestone A3 adds loading S3 storage backends from DB.
        Ok(())
    }

    pub fn backend(&self, id: i64) -> Option<Arc<Backend>> {
        self.backends.read().unwrap().get(&id).cloned()
    }

    pub fn active(&self) -> (i64, Arc<Backend>) {
        let id = self.active.load(Ordering::Relaxed);
        let backend = self
            .backend(id)
            .expect("active storage backend must be present");
        (id, backend)
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
            let mut locks = self.upload_locks.lock().unwrap();
            locks
                .entry(id)
                .or_insert_with(|| Arc::new(TokioMutex::new(())))
                .clone()
        };
        lock.lock_owned().await
    }
}
