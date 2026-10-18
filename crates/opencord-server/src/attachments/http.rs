//! HTTP endpoints for media uploads.

use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::{get, post, put};
use opencord_common::blob::BlobKey;
use opencord_common::upload::{PIECE_BYTES, POSTER_MAX_BYTES};
use opencord_proto::v1 as proto;
use proto::response::Result as Reply;

use crate::attachments::temp;
use crate::db::attachments::{self, AttachmentRow};
use crate::error::ApiError;
use crate::media;
use crate::state::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/media/uploads/{id}", put(put_upload))
        .route("/media/uploads/{id}", get(get_upload))
        .route("/media/uploads/{id}/finish", post(finish_upload))
}

async fn put_upload(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let offset = match headers
        .get("Upload-Offset")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
    {
        Some(o) => o,
        None => {
            return media::reply_error(&ApiError::invalid_argument(
                "missing or invalid Upload-Offset header",
            ));
        }
    };

    let piece = match to_bytes(body, (PIECE_BYTES + 1) as usize).await {
        Ok(b) => b,
        Err(_) => return media::reply_error(&ApiError::invalid_argument("piece too large")),
    };

    let user_id = match media::authorize(&state, &headers) {
        Ok(id) => id,
        Err(err) => return media::reply_error(&err),
    };

    let _guard = state.attachments.lock_upload(id).await;

    let mut conn = match state.db.acquire().await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(?e, "database error");
            return media::reply_error(&ApiError::internal());
        }
    };

    let row = match attachments::find(&mut conn, id).await {
        Ok(Some(row)) => row,
        Ok(None) => return media::reply_error(&ApiError::not_found("upload")),
        Err(e) => {
            tracing::error!(?e, "database error");
            return media::reply_error(&ApiError::internal());
        }
    };

    if row.uploader_id != user_id {
        return media::reply_error(&ApiError::not_found("upload"));
    }

    if row.state != "uploading" {
        return media::reply(
            StatusCode::CONFLICT,
            Reply::UploadStatus(to_status_proto(&row)),
        );
    }

    if offset != row.received_bytes as u64 {
        return media::reply(
            StatusCode::CONFLICT,
            Reply::UploadStatus(to_status_proto(&row)),
        );
    }

    let expected_len = (row.size_bytes as u64 - offset).min(PIECE_BYTES as u64);
    if piece.len() as u64 != expected_len {
        return media::reply_error(&ApiError::invalid_argument("incorrect piece length"));
    }
    if piece.is_empty() && row.size_bytes > 0 {
        return media::reply_error(&ApiError::invalid_argument("piece cannot be empty"));
    }

    let key_bytes = match state
        .certificates
        .secrets()
        .open(&format!("attachment-key:{id}"), &row.key_enc)
    {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(?e, "failed to unseal attachment key");
            return media::reply_error(&ApiError::internal());
        }
    };
    let Ok(key_arr) = <[u8; 32]>::try_from(key_bytes) else {
        return media::reply_error(&ApiError::internal());
    };
    let key = BlobKey::from_bytes(key_arr);

    let temp_path = state.attachments.temp_path(id);
    if let Err(e) =
        temp::append_piece(&temp_path, &key, id, offset, row.size_bytes as u64, &piece).await
    {
        tracing::error!(?e, "failed to append piece to temp file");
        return media::reply_error(&ApiError::internal());
    }

    let new_received = (offset + piece.len() as u64) as i64;
    let now = crate::state::now_ms();
    if let Err(e) = attachments::set_received(&mut conn, id, new_received, now).await {
        tracing::error!(?e, "failed to update received bytes");
        return media::reply_error(&ApiError::internal());
    }

    let mut updated = row;
    updated.received_bytes = new_received;
    media::reply(
        StatusCode::OK,
        Reply::UploadStatus(to_status_proto(&updated)),
    )
}

async fn get_upload(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> Response {
    let user_id = match media::authorize(&state, &headers) {
        Ok(id) => id,
        Err(err) => return media::reply_error(&err),
    };

    let mut conn = match state.db.acquire().await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(?e, "database error");
            return media::reply_error(&ApiError::internal());
        }
    };

    let row = match attachments::find(&mut conn, id).await {
        Ok(Some(row)) => row,
        Ok(None) => return media::reply_error(&ApiError::not_found("upload")),
        Err(e) => {
            tracing::error!(?e, "database error");
            return media::reply_error(&ApiError::internal());
        }
    };

    if row.uploader_id != user_id {
        return media::reply_error(&ApiError::not_found("upload"));
    }

    media::reply(StatusCode::OK, Reply::UploadStatus(to_status_proto(&row)))
}

async fn finish_upload(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let poster = match to_bytes(body, POSTER_MAX_BYTES + 1).await {
        Ok(b) => b,
        Err(_) => return media::reply_error(&ApiError::invalid_argument("poster too large")),
    };

    if poster.len() > POSTER_MAX_BYTES {
        return media::reply_error(&ApiError::invalid_argument("poster too large"));
    }

    let user_id = match media::authorize(&state, &headers) {
        Ok(id) => id,
        Err(err) => return media::reply_error(&err),
    };

    let _guard = state.attachments.lock_upload(id).await;

    let mut conn = match state.db.acquire().await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(?e, "database error");
            return media::reply_error(&ApiError::internal());
        }
    };

    let row = match attachments::find(&mut conn, id).await {
        Ok(Some(row)) => row,
        Ok(None) => return media::reply_error(&ApiError::not_found("upload")),
        Err(e) => {
            tracing::error!(?e, "database error");
            return media::reply_error(&ApiError::internal());
        }
    };

    if row.uploader_id != user_id {
        return media::reply_error(&ApiError::not_found("upload"));
    }

    if row.state == "uploading" {
        if row.received_bytes != row.size_bytes {
            return media::reply_error(&ApiError::invalid_argument("upload incomplete"));
        }

        if !poster.is_empty() {
            state.attachments.set_poster(id, poster.to_vec());
        }

        let now = crate::state::now_ms();
        if let Err(e) = attachments::set_state(&mut conn, id, "processing", now).await {
            tracing::error!(?e, "failed to set state to processing");
            return media::reply_error(&ApiError::internal());
        }

        crate::attachments::process::spawn_job(state.clone(), id);

        let mut updated = row;
        updated.state = "processing".to_string();
        media::reply(
            StatusCode::ACCEPTED,
            Reply::UploadStatus(to_status_proto(&updated)),
        )
    } else {
        media::reply(StatusCode::OK, Reply::UploadStatus(to_status_proto(&row)))
    }
}

pub fn to_status_proto(row: &AttachmentRow) -> proto::UploadStatus {
    let state = match row.state.as_str() {
        "uploading" => proto::UploadState::Uploading,
        "processing" => proto::UploadState::Processing,
        "ready" => proto::UploadState::Ready,
        "sent" => proto::UploadState::Sent,
        "failed" => proto::UploadState::Failed,
        _ => proto::UploadState::Unspecified,
    };
    let attachment = if row.state == "ready" || row.state == "sent" {
        Some(attachment_to_proto(row))
    } else {
        None
    };
    let error = if row.state == "failed" {
        row.error_code.map(|code| proto::Error {
            code: code as i32,
            message: row.error_message.clone().unwrap_or_default(),
            retry_after_ms: None,
        })
    } else {
        None
    };
    proto::UploadStatus {
        upload_id: row.id,
        state: state.into(),
        received_bytes: row.received_bytes as u64,
        attachment,
        error,
    }
}

pub fn attachment_to_proto(row: &AttachmentRow) -> proto::Attachment {
    let kind = match row.kind.as_str() {
        "file" => proto::AttachmentKind::File,
        "image" => proto::AttachmentKind::Image,
        "video" => proto::AttachmentKind::Video,
        "audio" => proto::AttachmentKind::Audio,
        "voice" => proto::AttachmentKind::VoiceMessage,
        _ => proto::AttachmentKind::Unspecified,
    };
    proto::Attachment {
        id: row.id,
        filename: row.filename.clone(),
        content_type: row.content_type.clone(),
        size_bytes: row.size_bytes as u64,
        kind: kind.into(),
        width: row.width.map(|w| w as u32),
        height: row.height.map(|h| h as u32),
        duration_ms: row.duration_ms.map(|d| d as u32),
        waveform: row.waveform.clone().unwrap_or_default(),
        has_thumbnail: row.has_thumbnail,
        thumbnail_width: row.thumbnail_width.map(|w| w as u32),
        thumbnail_height: row.thumbnail_height.map(|h| h as u32),
        animated: row.animated,
    }
}
