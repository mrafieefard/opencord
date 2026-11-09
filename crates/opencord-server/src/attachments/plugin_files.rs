// SPDX-License-Identifier: MPL-2.0

//! Plugin files sent as message attachments (plan M2 Task 13): the member
//! upload path (row, encrypted temp file, processing, storage), fed from disk.

use std::sync::Arc;
use tokio::io::AsyncReadExt;

use opencord_common::upload::PIECE_BYTES;

use crate::attachments::{open_key, process, temp};
use crate::db;
use crate::db::attachments::AttachmentRow;
use crate::error::ApiError;
use crate::handlers::uploads::{NewUpload, insert_new};
use crate::state::AppState;

/// A ready attachment in `channel_id`, uploaded by `uploader_id` (the plugin's user).
/// The caller has already run the checks (count, blocked extension, size).
pub async fn attach(
    state: &Arc<AppState>,
    uploader_id: i64,
    channel_id: i64,
    filename: String,
    mut file: tokio::fs::File,
    size: u64,
) -> Result<AttachmentRow, ApiError> {
    let mut conn = state.db.acquire().await?;
    let row = insert_new(
        state,
        &mut conn,
        NewUpload {
            uploader_id,
            channel_id,
            kind: "file",
            filename,
            size_bytes: size,
            width: None,
            height: None,
            duration_ms: None,
        },
    )
    .await?;
    drop(conn);

    let id = row.id;
    let temp_path = state.attachments.temp_path(id);

    let result: Result<AttachmentRow, ApiError> = async {
        let key = open_key(state.certificates.secrets(), &row)?;

        if size == 0 {
            temp::append_piece(&temp_path, &key, id, 0, 0, &[])
                .await
                .map_err(|e| {
                    tracing::error!(upload_id = id, ?e, "writing empty temp piece failed");
                    ApiError::internal()
                })?;
        } else {
            let mut offset = 0;
            while offset < size {
                let chunk_size = (PIECE_BYTES as u64).min(size - offset) as usize;
                let mut buf = vec![0u8; chunk_size];
                file.read_exact(&mut buf).await.map_err(|e| {
                    tracing::error!(upload_id = id, ?e, "reading plugin file failed");
                    ApiError::internal()
                })?;
                temp::append_piece(&temp_path, &key, id, offset, size, &buf)
                    .await
                    .map_err(|e| {
                        tracing::error!(upload_id = id, ?e, "writing temp piece failed");
                        ApiError::internal()
                    })?;
                offset += chunk_size as u64;
            }
        }

        let now = crate::state::now_ms();
        let mut conn = state.db.acquire().await?;
        db::attachments::set_received(&mut conn, id, size as i64, now).await?;
        db::attachments::set_state(&mut conn, id, "processing", now).await?;
        drop(conn);

        let _permit = state
            .attachments
            .jobs()
            .acquire_owned()
            .await
            .map_err(|_| ApiError::internal())?;
        process::run(state, id).await?;

        let mut conn = state.db.acquire().await?;
        let ready_row = db::attachments::find(&mut conn, id).await?;
        match ready_row {
            Some(r) if r.state == "ready" => Ok(r),
            _ => Err(ApiError::internal()),
        }
    }
    .await;

    match result {
        Ok(ready_row) => Ok(ready_row),
        Err(err) => {
            let _ = tokio::fs::remove_file(&temp_path).await;
            if let Ok(mut conn) = state.db.acquire().await {
                let _ = db::attachments::delete(&mut conn, id).await;
            }
            Err(err)
        }
    }
}
