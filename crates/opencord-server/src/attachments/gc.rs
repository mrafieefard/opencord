//! Garbage collection and blob deletion for attachments.

use std::sync::Arc;

use crate::db;
use crate::state::AppState;

pub fn start(state: &Arc<AppState>) {
    let sweep_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(600));
        interval.tick().await; // consume first immediate tick
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    sweep(&sweep_state, crate::state::now_ms()).await;
                }
                () = sweep_state.shutdown.cancelled() => return,
            }
        }
    });

    let drain_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        interval.tick().await; // consume first immediate tick
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    drain_blobs(&drain_state, crate::state::now_ms()).await;
                }
                () = drain_state.shutdown.cancelled() => return,
            }
        }
    });
}

pub async fn sweep(state: &AppState, now: i64) {
    let mut conn = match state.db.acquire().await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(?e, "database acquire failed in sweep");
            return;
        }
    };

    let expired_ids = match db::attachments::expired(&mut conn, now).await {
        Ok(ids) => ids,
        Err(e) => {
            tracing::error!(?e, "failed to query expired attachments");
            return;
        }
    };

    for id in expired_ids {
        if let Err(e) = db::attachments::delete(&mut conn, id).await {
            tracing::error!(
                upload_id = id,
                ?e,
                "failed to delete expired attachment row"
            );
        }
        let temp_path = state.attachments.temp_path(id);
        tokio::fs::remove_file(&temp_path).await.ok();
    }
}

pub async fn drain_blobs(state: &AppState, now: i64) {
    let mut conn = match state.db.acquire().await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(?e, "database acquire failed in drain_blobs");
            return;
        }
    };

    let due = match db::storage::due_blobs(&mut conn, now, 100).await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(?e, "failed to query due blob deletions");
            return;
        }
    };

    for blob in due {
        let backend_opt = state.attachments.backend(blob.storage_id);
        let res = match backend_opt {
            Some(backend) => backend.delete(&blob.object_key).await,
            None => {
                // An unknown backend counts as done.
                Ok(())
            }
        };

        match res {
            Ok(()) => {
                let _ = db::storage::blob_done(&mut conn, blob.id).await;
            }
            Err(e) => {
                tracing::warn!(blob_id = blob.id, key = %blob.object_key, ?e, "blob deletion failed, scheduling retry");
                let attempts = blob.attempts + 1;
                let delay_minutes = 1i64
                    .checked_shl(blob.attempts as u32)
                    .unwrap_or(360)
                    .min(360);
                let next_at = now + delay_minutes * 60 * 1000;
                let _ = db::storage::blob_retry(&mut conn, blob.id, attempts, next_at).await;
            }
        }
    }
}
