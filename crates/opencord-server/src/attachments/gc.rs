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
                if let Err(e) = db::storage::blob_done(&mut conn, blob.id).await {
                    tracing::error!(blob_id = blob.id, ?e, "a deleted blob stayed queued");
                }
            }
            Err(e) => {
                tracing::warn!(blob_id = blob.id, key = %blob.object_key, ?e, "blob deletion failed, scheduling retry");
                let next_at = now + retry_delay_ms(blob.attempts);
                if let Err(e) =
                    db::storage::blob_retry(&mut conn, blob.id, blob.attempts + 1, next_at).await
                {
                    tracing::error!(blob_id = blob.id, ?e, "a blob retry wasn't scheduled");
                }
            }
        }
    }
}

/// 2^attempts minutes, at most 6 hours.
fn retry_delay_ms(attempts: i64) -> i64 {
    const MAX_MINUTES: i64 = 6 * 60;
    // 2^9 minutes is already past the cap; this also keeps the shift small.
    let minutes = (1i64 << attempts.clamp(0, 9)).min(MAX_MINUTES);
    minutes * 60 * 1000
}

#[cfg(test)]
mod tests {
    use super::retry_delay_ms;

    #[test]
    fn retries_back_off_to_six_hours() {
        const MINUTE: i64 = 60_000;
        assert_eq!(retry_delay_ms(0), MINUTE);
        assert_eq!(retry_delay_ms(3), 8 * MINUTE);
        assert_eq!(retry_delay_ms(8), 256 * MINUTE);
        assert_eq!(retry_delay_ms(9), 360 * MINUTE);
        // Weeks of failures must not overflow the shift.
        assert_eq!(retry_delay_ms(63), 360 * MINUTE);
        assert_eq!(retry_delay_ms(i64::MAX), 360 * MINUTE);
    }
}
