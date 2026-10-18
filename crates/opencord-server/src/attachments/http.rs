//! HTTP endpoints for media uploads.

use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use futures_util::StreamExt;
use opencord_common::blob::{BlobKey, BlobKind, SEGMENT_PLAIN};
use opencord_common::permissions::Permissions;
use opencord_common::upload::{PIECE_BYTES, POSTER_MAX_BYTES};
use opencord_proto::v1 as proto;
use proto::response::Result as Reply;

use crate::attachments::range;
use crate::attachments::storage::{ObjectKind, object_key};
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
        .route("/media/attachments/{id}", get(get_attachment))
        .route("/media/attachments/{id}/thumbnail", get(get_thumbnail))
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

async fn get_attachment(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> Response {
    // 1. Authorize the token
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
        Ok(None) => return media::reply_error(&ApiError::not_found("attachment")),
        Err(e) => {
            tracing::error!(?e, "database error");
            return media::reply_error(&ApiError::internal());
        }
    };

    // 2. The row must be sent, and its message must exist with deleted = FALSE
    if row.state != "sent" {
        return media::reply_error(&ApiError::not_found("attachment"));
    }
    let message_id = match row.message_id {
        Some(mid) => mid,
        None => return media::reply_error(&ApiError::not_found("attachment")),
    };

    let message = match crate::db::messages::find(&mut conn, message_id).await {
        Ok(Some(msg)) => msg,
        Ok(None) => return media::reply_error(&ApiError::not_found("attachment")),
        Err(e) => {
            tracing::error!(?e, "database error");
            return media::reply_error(&ApiError::internal());
        }
    };
    if message.deleted {
        return media::reply_error(&ApiError::not_found("attachment"));
    }

    // 3. require_channel(VIEW_CHANNEL | READ_HISTORY): not found -> 404; missing READ_HISTORY -> 403
    {
        let guild = state.guild();
        if let Err(err) = crate::permissions::require_channel(
            &guild,
            user_id,
            row.channel_id,
            Permissions::VIEW_CHANNEL | Permissions::READ_HISTORY,
        ) {
            return media::reply_error(&err);
        }
    }

    // 4. Parse the range: error -> 416 with Content-Range: bytes */{len}
    let range_header = headers.get("Range").and_then(|v| v.to_str().ok());
    let file_len = row.size_bytes as u64;
    let range_opt = match range::parse(range_header, file_len) {
        Ok(r) => r,
        Err(_) => {
            return Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header("Content-Range", format!("bytes */{file_len}"))
                .body(Body::empty())
                .unwrap();
        }
    };

    // Unseal the key
    let key_bytes = match state
        .certificates
        .secrets()
        .open(&format!("attachment-key:{id}"), &row.key_enc)
    {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(upload_id = id, ?e, "failed to unseal attachment key");
            return media::reply_error(&ApiError::internal());
        }
    };
    let Ok(key_arr) = <[u8; 32]>::try_from(key_bytes) else {
        return media::reply_error(&ApiError::internal());
    };
    let key = Arc::new(BlobKey::from_bytes(key_arr));

    let backend = match state.attachments.backend(row.storage_id) {
        Some(b) => b,
        None => return StatusCode::BAD_GATEWAY.into_response(),
    };

    let (start_byte, end_byte, is_partial) = match range_opt {
        Some((s, e)) => (s, e, true),
        None => {
            if file_len == 0 {
                (0, 0, false)
            } else {
                (0, file_len - 1, false)
            }
        }
    };

    let content_len = if file_len == 0 {
        0
    } else {
        end_byte - start_byte + 1
    };

    if file_len == 0 {
        return Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", &row.content_type)
            .header("Content-Length", "0")
            .header("Accept-Ranges", "bytes")
            .header("Cache-Control", "private, no-store")
            .header("X-Content-Type-Options", "nosniff")
            .header(
                "Content-Disposition",
                format!(
                    "attachment; filename*=UTF-8''{}",
                    percent_encode_filename(&row.filename)
                ),
            )
            .body(Body::empty())
            .unwrap();
    }

    // Read the first chunk to catch 502 before sending headers
    let first_end = ((start_byte / SEGMENT_PLAIN + 16) * SEGMENT_PLAIN - 1).min(end_byte);
    let first_span = opencord_common::blob::locate(file_len, start_byte, first_end);
    let first_sealed = match backend
        .get_range(
            &object_key(ObjectKind::File, id),
            first_span.sealed_start,
            first_span.sealed_end,
        )
        .await
    {
        Ok(s) => s,
        Err(_) => return StatusCode::BAD_GATEWAY.into_response(),
    };
    let first_plain = match opencord_common::blob::open_span(
        &key,
        BlobKind::File,
        id,
        file_len,
        &first_span,
        &first_sealed,
    ) {
        Ok(p) => p,
        Err(_) => return StatusCode::BAD_GATEWAY.into_response(),
    };

    let next_cursor = first_end + 1;
    let stream_state = if next_cursor <= end_byte {
        Some((
            next_cursor,
            end_byte,
            file_len,
            id,
            key.clone(),
            backend.clone(),
        ))
    } else {
        None
    };

    let stream = futures_util::stream::unfold(stream_state, |state_opt| async move {
        let (cur_start, end_byte, file_len, id, key, backend) = state_opt?;
        let cur_end = ((cur_start / SEGMENT_PLAIN + 16) * SEGMENT_PLAIN - 1).min(end_byte);
        let span = opencord_common::blob::locate(file_len, cur_start, cur_end);
        let sealed = backend
            .get_range(
                &object_key(ObjectKind::File, id),
                span.sealed_start,
                span.sealed_end,
            )
            .await
            .ok()?;
        let plain =
            opencord_common::blob::open_span(&key, BlobKind::File, id, file_len, &span, &sealed)
                .ok()?;

        let next_start = cur_end + 1;
        let next_state = if next_start <= end_byte {
            Some((next_start, end_byte, file_len, id, key, backend))
        } else {
            None
        };
        Some((
            Ok::<bytes::Bytes, std::io::Error>(bytes::Bytes::from(plain)),
            next_state,
        ))
    });

    let full_stream = futures_util::stream::once(async move {
        Ok::<bytes::Bytes, std::io::Error>(bytes::Bytes::from(first_plain))
    })
    .chain(stream);

    let body = Body::from_stream(full_stream);

    let status = if is_partial {
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    };

    let mut res_builder = Response::builder()
        .status(status)
        .header("Content-Type", &row.content_type)
        .header("Content-Length", content_len.to_string())
        .header("Accept-Ranges", "bytes")
        .header("Cache-Control", "private, no-store")
        .header("X-Content-Type-Options", "nosniff")
        .header(
            "Content-Disposition",
            format!(
                "attachment; filename*=UTF-8''{}",
                percent_encode_filename(&row.filename)
            ),
        );

    if is_partial {
        res_builder = res_builder.header(
            "Content-Range",
            format!("bytes {start_byte}-{end_byte}/{file_len}"),
        );
    }

    res_builder.body(body).unwrap()
}

async fn get_thumbnail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> Response {
    // 1. Authorize token
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
        Ok(None) => return media::reply_error(&ApiError::not_found("attachment")),
        Err(e) => {
            tracing::error!(?e, "database error");
            return media::reply_error(&ApiError::internal());
        }
    };

    // 2. The row must be sent, and its message must exist with deleted = FALSE
    if row.state != "sent" {
        return media::reply_error(&ApiError::not_found("attachment"));
    }
    let message_id = match row.message_id {
        Some(mid) => mid,
        None => return media::reply_error(&ApiError::not_found("attachment")),
    };

    let message = match crate::db::messages::find(&mut conn, message_id).await {
        Ok(Some(msg)) => msg,
        Ok(None) => return media::reply_error(&ApiError::not_found("attachment")),
        Err(e) => {
            tracing::error!(?e, "database error");
            return media::reply_error(&ApiError::internal());
        }
    };
    if message.deleted {
        return media::reply_error(&ApiError::not_found("attachment"));
    }

    // 3. require_channel(VIEW_CHANNEL | READ_HISTORY)
    {
        let guild = state.guild();
        if let Err(err) = crate::permissions::require_channel(
            &guild,
            user_id,
            row.channel_id,
            Permissions::VIEW_CHANNEL | Permissions::READ_HISTORY,
        ) {
            return media::reply_error(&err);
        }
    }

    // 4. has_thumbnail must be true, else 404
    if !row.has_thumbnail {
        return media::reply_error(&ApiError::not_found("thumbnail"));
    }

    // Open key
    let key_bytes = match state
        .certificates
        .secrets()
        .open(&format!("attachment-key:{id}"), &row.key_enc)
    {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(upload_id = id, ?e, "failed to unseal attachment key");
            return media::reply_error(&ApiError::internal());
        }
    };
    let Ok(key_arr) = <[u8; 32]>::try_from(key_bytes) else {
        return media::reply_error(&ApiError::internal());
    };
    let key = BlobKey::from_bytes(key_arr);

    let backend = match state.attachments.backend(row.storage_id) {
        Some(b) => b,
        None => return StatusCode::BAD_GATEWAY.into_response(),
    };

    // Fetch the whole sealed thumbnail
    let thumb_key = object_key(ObjectKind::Thumbnail, id);
    let size = match backend.size(&thumb_key).await {
        Ok(Some(s)) => s,
        _ => return StatusCode::BAD_GATEWAY.into_response(),
    };

    let sealed = match backend.get_range(&thumb_key, 0, size).await {
        Ok(b) => b,
        Err(_) => return StatusCode::BAD_GATEWAY.into_response(),
    };

    let plain = match opencord_common::blob::open_all(&key, BlobKind::Thumbnail, id, &sealed) {
        Ok(p) => p,
        Err(_) => return StatusCode::BAD_GATEWAY.into_response(),
    };

    let content_type = if plain.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        "image/png"
    } else {
        "image/jpeg"
    };

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", content_type)
        .header("Content-Length", plain.len().to_string())
        .header("Accept-Ranges", "bytes")
        .header("Cache-Control", "private, no-store")
        .header("X-Content-Type-Options", "nosniff")
        .header(
            "Content-Disposition",
            format!(
                "attachment; filename*=UTF-8''thumb-{}",
                percent_encode_filename(&row.filename)
            ),
        )
        .body(Body::from(plain))
        .unwrap()
}

fn percent_encode_filename(name: &str) -> String {
    let mut out = String::new();
    for &b in name.as_bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'.' || b == b'_' || b == b'~' {
            out.push(b as char);
        } else {
            use std::fmt::Write;
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}
