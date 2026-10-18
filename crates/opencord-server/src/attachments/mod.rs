//! Media attachments and storage backends (Phase 4).

pub mod http;
pub mod images;
pub mod process;
pub mod settings;
pub mod storage;
pub mod temp;
pub mod voice;

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

    pub fn hold_processing(&self, hold: bool) {
        self.hold_processing.store(hold, Ordering::Relaxed);
    }

    pub fn is_processing_held(&self) -> bool {
        self.hold_processing.load(Ordering::Relaxed)
    }

    pub fn set_poster(&self, id: i64, poster: Vec<u8>) {
        self.posters.lock().unwrap().insert(id, poster);
    }

    pub fn take_poster(&self, id: i64) -> Option<Vec<u8>> {
        self.posters.lock().unwrap().remove(&id)
    }

    pub fn jobs(&self) -> Arc<Semaphore> {
        self.jobs.clone()
    }
}

pub async fn start(state: &Arc<crate::state::AppState>) -> anyhow::Result<()> {
    // 1. Delete orphan temp files: temp files with no uploading or processing row.
    let temp_dir = state.attachments.temp_dir();
    if tokio::fs::try_exists(temp_dir).await.unwrap_or(false) {
        let mut active_ids = std::collections::HashSet::new();
        if let Ok(mut conn) = state.db.acquire().await {
            if let Ok(rows) = crate::db::attachments::in_state(&mut conn, "uploading").await {
                for r in rows {
                    active_ids.insert(r.id);
                }
            }
            if let Ok(rows) = crate::db::attachments::in_state(&mut conn, "processing").await {
                for r in rows {
                    active_ids.insert(r.id);
                }
            }
        }
        if let Ok(mut entries) = tokio::fs::read_dir(temp_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                let is_orphan = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .and_then(|name| name.strip_suffix(".part"))
                    .and_then(|id_str| id_str.parse::<i64>().ok())
                    .is_some_and(|id| !active_ids.contains(&id));
                if is_orphan {
                    tokio::fs::remove_file(&path).await.ok();
                }
            }
        }
    }

    // 2. Resume processing rows:
    if let Ok(mut conn) = state.db.acquire().await
        && let Ok(rows) = crate::db::attachments::in_state(&mut conn, "processing").await
    {
        for row in rows {
            process::spawn_job(state.clone(), row.id);
        }
    }

    Ok(())
}
