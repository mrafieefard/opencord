//! Moves every file to the active storage. Ciphertext is copied as is, so
//! nothing is decrypted or re-encrypted.

use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use opencord_proto::v1 as proto;
use sqlx::Connection;
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use super::{Backend, ObjectKind, StorageError, object_key};
use crate::db;
use crate::db::attachments::AttachmentRow;
use crate::error::ApiError;
use crate::state::{AppState, now_ms};

const PAGE: i64 = 50;
const AT_ONCE: usize = 2;
const RETRIES: u32 = 3;
const COPY_CHUNK: u64 = 8 * 1024 * 1024;
const PROGRESS_EVERY: Duration = Duration::from_secs(2);
/// Old copies outlive the switch by this much, so downloads that started
/// before it can finish.
const OLD_COPY_GRACE_MS: i64 = 60 * 60 * 1000;

#[derive(Debug, Default)]
pub struct Mover {
    /// The running or last move. Kept when it ends, so the page shows how it
    /// went.
    progress: Mutex<Option<proto::StorageMove>>,
    /// Present exactly while a move runs.
    cancel: Mutex<Option<CancellationToken>>,
    last_sent: Mutex<Option<Instant>>,
}

impl Mover {
    pub fn progress(&self) -> Option<proto::StorageMove> {
        *self.progress.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn is_running(&self) -> bool {
        self.cancel
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
    }

    /// The job stops before its next file.
    pub fn stop(&self) {
        if let Some(token) = self
            .cancel
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            token.cancel();
        }
    }

    /// Applies `change`; true when it's time to send the owner an update.
    fn update(&self, change: impl FnOnce(&mut proto::StorageMove)) -> bool {
        if let Some(progress) = self
            .progress
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_mut()
        {
            change(progress);
        }
        let mut last_sent = self
            .last_sent
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let due = last_sent.is_none_or(|at| at.elapsed() >= PROGRESS_EVERY);
        if due {
            *last_sent = Some(Instant::now());
        }
        due
    }
}

/// Starts moving every file that isn't on the active storage. The caller
/// holds the write lock, so the active storage can't change meanwhile.
pub async fn start(state: &Arc<AppState>) -> Result<(), ApiError> {
    let mover = state.attachments.mover();
    if mover.is_running() {
        return Err(ApiError::conflict("files are already being moved"));
    }
    let target = state.attachments.active_id();
    let (files, bytes) = {
        let mut conn = state.db.acquire().await?;
        db::attachments::count_not_on(&mut conn, target).await?
    };
    let token = CancellationToken::new();
    *mover.cancel.lock().unwrap_or_else(PoisonError::into_inner) = Some(token.clone());
    *mover
        .progress
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = Some(proto::StorageMove {
        to_storage_id: target,
        running: true,
        files_total: u64::try_from(files).unwrap_or(0),
        bytes_total: u64::try_from(bytes).unwrap_or(0),
        ..Default::default()
    });
    *mover
        .last_sent
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = None;
    tokio::spawn(run(Arc::clone(state), target, token));
    Ok(())
}

async fn run(state: Arc<AppState>, target: i64, cancel: CancellationToken) {
    let mut after_id = 0;
    while !cancel.is_cancelled() {
        let page = match next_page(&state, target, after_id).await {
            Ok(page) => page,
            Err(error) => {
                tracing::error!(%error, "the files to move couldn't be listed");
                break;
            }
        };
        let Some(last) = page.last() else { break };
        after_id = last.id;
        futures_util::stream::iter(page)
            .for_each_concurrent(AT_ONCE, |row| {
                let state = &state;
                let cancel = &cancel;
                async move {
                    if cancel.is_cancelled() {
                        return;
                    }
                    let moved = move_with_retries(state, &row, target, cancel).await;
                    let due = state.attachments.mover().update(|progress| {
                        if moved {
                            progress.files_done += 1;
                            progress.bytes_done += u64::try_from(row.size_bytes).unwrap_or(0);
                        } else {
                            progress.failed += 1;
                        }
                    });
                    if due {
                        send_progress(state).await;
                    }
                }
            })
            .await;
    }
    let mover = state.attachments.mover();
    mover.update(|progress| progress.running = false);
    *mover.cancel.lock().unwrap_or_else(PoisonError::into_inner) = None;
    send_progress(&state).await;
}

async fn next_page(
    state: &AppState,
    target: i64,
    after_id: i64,
) -> Result<Vec<AttachmentRow>, sqlx::Error> {
    let mut conn = state.db.acquire().await?;
    db::attachments::not_on(&mut conn, target, after_id, PAGE).await
}

async fn send_progress(state: &AppState) {
    if let Err(error) = super::manage::broadcast(state).await {
        tracing::warn!(?error, "the storage move's progress couldn't be sent");
    }
}

#[derive(Debug, thiserror::Error)]
enum MoveError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("storage {0} isn't loaded")]
    Missing(i64),
}

/// Three retries, 2, 4 and 8 seconds apart. False when the file stays put.
async fn move_with_retries(
    state: &AppState,
    row: &AttachmentRow,
    target: i64,
    cancel: &CancellationToken,
) -> bool {
    for attempt in 0..=RETRIES {
        match move_one(state, row, target).await {
            Ok(()) => return true,
            Err(error) if attempt < RETRIES && !cancel.is_cancelled() => {
                tracing::warn!(attachment_id = row.id, %error, attempt, "moving a file failed; trying again");
                tokio::time::sleep(Duration::from_secs(2 << attempt)).await;
            }
            Err(error) => {
                tracing::error!(attachment_id = row.id, %error, "a file couldn't be moved");
                return false;
            }
        }
    }
    false
}

async fn move_one(state: &AppState, row: &AttachmentRow, target: i64) -> Result<(), MoveError> {
    let from = row.storage_id;
    let source = state
        .attachments
        .backend(from)
        .ok_or(MoveError::Missing(from))?;
    let dest = state
        .attachments
        .backend(target)
        .ok_or(MoveError::Missing(target))?;
    let mut keys = vec![object_key(ObjectKind::File, row.id)];
    if row.has_thumbnail {
        keys.push(object_key(ObjectKind::Thumbnail, row.id));
    }
    for key in &keys {
        let path = state
            .attachments
            .temp_dir()
            .join(format!("move-{}.part", key.replace('/', "-")));
        let copied = copy(&source, &dest, key, &path).await;
        remove_temp(&path).await;
        copied?;
    }
    // Writes only (Working Rule 20): the row switches and the copies nobody
    // needs are queued together.
    let mut conn = state.db.acquire().await?;
    let mut tx = conn.begin().await?;
    let moved = db::attachments::set_storage_if(&mut tx, row.id, from, target).await?;
    if moved {
        let not_before = now_ms() + OLD_COPY_GRACE_MS;
        for key in &keys {
            db::storage::queue_blob_after(&mut tx, from, key, not_before).await?;
        }
    } else {
        // Deleted meanwhile: its old copies are already queued, and the new
        // ones are unused.
        for key in &keys {
            db::storage::queue_blob(&mut tx, target, key).await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

/// Through a temp file, in 8 MiB reads.
async fn copy(
    source: &Backend,
    dest: &Backend,
    key: &str,
    path: &Path,
) -> Result<(), StorageError> {
    let size = source.size(key).await?.ok_or(StorageError::NotFound)?;
    let mut file = tokio::fs::File::create(path).await?;
    let mut offset = 0;
    while offset < size {
        let end = (offset + COPY_CHUNK).min(size);
        file.write_all(&source.get_range(key, offset, end).await?)
            .await?;
        offset = end;
    }
    file.sync_all().await?;
    drop(file);
    dest.put_file(key, path).await
}

/// A local storage moved the file already; anything else is logged.
async fn remove_temp(path: &Path) {
    if let Err(error) = tokio::fs::remove_file(path).await
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(%error, "a move's temp file couldn't be deleted");
    }
}
