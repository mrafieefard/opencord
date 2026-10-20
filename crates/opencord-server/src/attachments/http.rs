//! HTTP endpoints for media uploads and downloads.
#![allow(
    clippy::result_large_err,
    reason = "handlers return a finished response on either side"
)]

use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::{Path, State};
use axum::http::header::{
    ACCEPT_RANGES, CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE,
    RANGE, X_CONTENT_TYPE_OPTIONS,
};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{AppendHeaders, IntoResponse, Response};
use axum::routing::{get, post, put};
use bytes::Bytes;
use futures_util::StreamExt;
use opencord_common::blob::{self, BlobKey, BlobKind, SEGMENT_PLAIN};
use opencord_common::permissions::Permissions;
use opencord_common::upload::{PIECE_BYTES, POSTER_MAX_BYTES};
use opencord_proto::v1 as proto;
use proto::response::Result as Reply;
use sqlx::SqliteConnection;

use crate::attachments::storage::{Backend, ObjectKind, object_key};
use crate::attachments::{open_key, range, temp};
use crate::db::attachments::{self, AttachmentRow};
use crate::error::ApiError;
use crate::media;
use crate::state::AppState;

/// Segments read from storage per request while streaming a download.
const READ_SEGMENTS: u64 = 16;
const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

type HttpResult = Result<Response, Response>;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/media/uploads/{id}", put(put_upload).get(get_upload))
        .route("/media/uploads/{id}/finish", post(finish_upload))
        .route("/media/attachments/{id}", get(get_attachment))
        .route("/media/attachments/{id}/thumbnail", get(get_thumbnail))
}

fn fail(error: ApiError) -> Response {
    media::reply_error(&error)
}

fn internal(error: impl std::fmt::Debug) -> Response {
    tracing::error!(?error, "a media request failed");
    fail(ApiError::internal())
}

fn status_reply(status: StatusCode, row: &AttachmentRow) -> Response {
    media::reply(status, Reply::UploadStatus(to_status_proto(row)))
}

/// The caller's own upload, in any state. Anyone else's is "not found".
async fn own_upload(
    conn: &mut SqliteConnection,
    user_id: i64,
    id: i64,
) -> Result<AttachmentRow, Response> {
    match attachments::find(conn, id).await {
        Ok(Some(row)) if row.uploader_id == user_id => Ok(row),
        Ok(_) => Err(fail(ApiError::not_found("upload"))),
        Err(error) => Err(internal(error)),
    }
}

async fn put_upload(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    body: Body,
) -> HttpResult {
    // The token comes first, so strangers can't make the server buffer pieces.
    let user_id = media::authorize(&state, &headers).map_err(fail)?;
    let offset = headers
        .get("Upload-Offset")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(|| {
            fail(ApiError::invalid_argument(
                "missing or invalid Upload-Offset header",
            ))
        })?;
    let piece = to_bytes(body, PIECE_BYTES as usize + 1)
        .await
        .map_err(|_| fail(ApiError::invalid_argument("piece too large")))?;

    let _guard = state.attachments.lock_upload(id).await;
    let mut conn = state.db.acquire().await.map_err(internal)?;
    let row = own_upload(&mut conn, user_id, id).await?;
    if row.state != "uploading" || offset != row.received_bytes as u64 {
        return Ok(status_reply(StatusCode::CONFLICT, &row));
    }
    let size = row.size_bytes as u64;
    let expected = (size - offset).min(u64::from(PIECE_BYTES));
    if piece.len() as u64 != expected {
        return Err(fail(ApiError::invalid_argument("incorrect piece length")));
    }
    // Once every byte is in, an empty piece would truncate the last segment.
    if piece.is_empty() && size > 0 {
        return Err(fail(ApiError::invalid_argument("piece cannot be empty")));
    }

    let key = open_key(state.certificates.secrets(), &row).map_err(fail)?;
    temp::append_piece(
        &state.attachments.temp_path(id),
        &key,
        id,
        offset,
        size,
        &piece,
    )
    .await
    .map_err(internal)?;
    let received = (offset + piece.len() as u64) as i64;
    attachments::set_received(&mut conn, id, received, crate::state::now_ms())
        .await
        .map_err(internal)?;
    let row = AttachmentRow {
        received_bytes: received,
        ..row
    };
    Ok(status_reply(StatusCode::OK, &row))
}

async fn get_upload(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> HttpResult {
    let user_id = media::authorize(&state, &headers).map_err(fail)?;
    let mut conn = state.db.acquire().await.map_err(internal)?;
    let row = own_upload(&mut conn, user_id, id).await?;
    Ok(status_reply(StatusCode::OK, &row))
}

async fn finish_upload(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    body: Body,
) -> HttpResult {
    let user_id = media::authorize(&state, &headers).map_err(fail)?;
    let poster = to_bytes(body, POSTER_MAX_BYTES)
        .await
        .map_err(|_| fail(ApiError::invalid_argument("poster too large")))?;

    let _guard = state.attachments.lock_upload(id).await;
    let mut conn = state.db.acquire().await.map_err(internal)?;
    let row = own_upload(&mut conn, user_id, id).await?;
    if row.state != "uploading" {
        return Ok(status_reply(StatusCode::OK, &row));
    }
    if row.received_bytes != row.size_bytes {
        return Err(fail(ApiError::invalid_argument("upload incomplete")));
    }
    if row.size_bytes == 0 {
        // A zero-byte file is one empty last segment, whether or not the
        // client sent an empty piece.
        let key = open_key(state.certificates.secrets(), &row).map_err(fail)?;
        temp::append_piece(&state.attachments.temp_path(id), &key, id, 0, 0, &[])
            .await
            .map_err(internal)?;
    }

    attachments::set_state(&mut conn, id, "processing", crate::state::now_ms())
        .await
        .map_err(internal)?;
    if !poster.is_empty() {
        state.attachments.set_poster(id, poster.to_vec());
    }
    crate::attachments::process::spawn_job(state.clone(), id);
    let row = AttachmentRow {
        state: "processing".to_owned(),
        ..row
    };
    Ok(status_reply(StatusCode::ACCEPTED, &row))
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
    let attachment =
        matches!(row.state.as_str(), "ready" | "sent").then(|| attachment_to_proto(row));
    let error = (row.state == "failed")
        .then(|| {
            row.error_code.map(|code| proto::Error {
                code: code as i32,
                message: row.error_message.clone().unwrap_or_default(),
                retry_after_ms: None,
            })
        })
        .flatten();
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

/// A sent attachment the caller may read right now: its message still exists
/// and the channel still lets them read its history.
async fn readable_attachment(
    state: &AppState,
    headers: &HeaderMap,
    id: i64,
) -> Result<AttachmentRow, Response> {
    let not_found = || fail(ApiError::not_found("attachment"));
    let user_id = media::authorize(state, headers).map_err(fail)?;
    let mut conn = state.db.acquire().await.map_err(internal)?;
    let row = attachments::find(&mut conn, id)
        .await
        .map_err(internal)?
        .filter(|row| row.state == "sent")
        .ok_or_else(not_found)?;
    let message_id = row.message_id.ok_or_else(not_found)?;
    let message = crate::db::messages::find(&mut conn, message_id)
        .await
        .map_err(internal)?;
    if !message.is_some_and(|message| !message.deleted) {
        return Err(not_found());
    }
    {
        let guild = state.guild();
        crate::permissions::require_channel(
            &guild,
            user_id,
            row.channel_id,
            Permissions::VIEW_CHANNEL | Permissions::READ_HISTORY,
        )
        .map_err(fail)?;
    }
    Ok(row)
}

fn bad_gateway(id: i64, error: impl std::fmt::Display) -> Response {
    tracing::warn!(attachment_id = id, %error, "reading a stored attachment failed");
    StatusCode::BAD_GATEWAY.into_response()
}

fn disposition(filename: &str) -> String {
    format!(
        "attachment; filename*=UTF-8''{}",
        percent_encode_filename(filename)
    )
}

async fn get_attachment(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> HttpResult {
    let row = readable_attachment(&state, &headers, id).await?;
    let len = row.size_bytes as u64;
    let range =
        range::parse(headers.get(RANGE).and_then(|v| v.to_str().ok()), len).map_err(|_| {
            let content_range = [(CONTENT_RANGE, format!("bytes */{len}"))];
            (StatusCode::RANGE_NOT_SATISFIABLE, content_range).into_response()
        })?;
    let key = Arc::new(open_key(state.certificates.secrets(), &row).map_err(fail)?);
    let backend = state
        .attachments
        .backend(row.storage_id)
        .ok_or_else(|| bad_gateway(id, "its storage isn't loaded"))?;

    let mut reply_headers = vec![
        (CONTENT_TYPE, row.content_type.clone()),
        (ACCEPT_RANGES, "bytes".to_owned()),
        (CACHE_CONTROL, "private, no-store".to_owned()),
        (X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
        (CONTENT_DISPOSITION, disposition(&row.filename)),
    ];
    let Some((start, end)) = range.or_else(|| (len > 0).then(|| (0, len - 1))) else {
        reply_headers.push((CONTENT_LENGTH, "0".to_owned()));
        return Ok((StatusCode::OK, AppendHeaders(reply_headers), Body::empty()).into_response());
    };

    // The first batch is read before answering, so a storage failure is a
    // 502 rather than a body that stops short.
    let first_end = batch_end(start, end);
    let first = read_plain(&backend, &key, id, len, start, first_end)
        .await
        .map_err(|error| bad_gateway(id, error))?;
    let rest = futures_util::stream::try_unfold(first_end + 1, move |cursor| {
        let backend = Arc::clone(&backend);
        let key = Arc::clone(&key);
        async move {
            if cursor > end {
                return Ok(None);
            }
            let batch = batch_end(cursor, end);
            let plain = read_plain(&backend, &key, id, len, cursor, batch)
                .await
                .map_err(|error| {
                    // Ending the stream with an error aborts the response.
                    tracing::warn!(attachment_id = id, %error, "a download stopped early");
                    std::io::Error::other(error)
                })?;
            Ok::<_, std::io::Error>(Some((Bytes::from(plain), batch + 1)))
        }
    });
    let body = Body::from_stream(
        futures_util::stream::once(async move { Ok(Bytes::from(first)) }).chain(rest),
    );

    reply_headers.push((CONTENT_LENGTH, (end - start + 1).to_string()));
    let status = if range.is_some() {
        reply_headers.push((CONTENT_RANGE, format!("bytes {start}-{end}/{len}")));
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    };
    Ok((status, AppendHeaders(reply_headers), body).into_response())
}

/// The last byte of a read starting at `start`: at most [`READ_SEGMENTS`]
/// segments, never past `end`.
fn batch_end(start: u64, end: u64) -> u64 {
    ((start / SEGMENT_PLAIN + READ_SEGMENTS) * SEGMENT_PLAIN - 1).min(end)
}

/// Plaintext bytes `start..=end` of a stored file of `len` bytes.
async fn read_plain(
    backend: &Backend,
    key: &BlobKey,
    id: i64,
    len: u64,
    start: u64,
    end: u64,
) -> Result<Vec<u8>, String> {
    let span = blob::locate(len, start, end);
    let sealed = backend
        .get_range(
            &object_key(ObjectKind::File, id),
            span.sealed_start,
            span.sealed_end,
        )
        .await
        .map_err(|error| error.to_string())?;
    blob::open_span(key, BlobKind::File, id, len, &span, &sealed).map_err(|error| error.to_string())
}

async fn get_thumbnail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> HttpResult {
    let row = readable_attachment(&state, &headers, id).await?;
    if !row.has_thumbnail {
        return Err(fail(ApiError::not_found("thumbnail")));
    }
    let key = open_key(state.certificates.secrets(), &row).map_err(fail)?;
    let backend = state
        .attachments
        .backend(row.storage_id)
        .ok_or_else(|| bad_gateway(id, "its storage isn't loaded"))?;
    let thumb_key = object_key(ObjectKind::Thumbnail, id);
    let size = backend
        .size(&thumb_key)
        .await
        .map_err(|error| bad_gateway(id, error))?
        .ok_or_else(|| bad_gateway(id, "the thumbnail is missing"))?;
    let sealed = backend
        .get_range(&thumb_key, 0, size)
        .await
        .map_err(|error| bad_gateway(id, error))?;
    let plain = blob::open_all(&key, BlobKind::Thumbnail, id, &sealed)
        .map_err(|error| bad_gateway(id, error))?;
    let content_type = if plain.starts_with(&PNG_MAGIC) {
        "image/png"
    } else {
        "image/jpeg"
    };
    let reply_headers = [
        (CONTENT_TYPE, content_type.to_owned()),
        (CACHE_CONTROL, "private, no-store".to_owned()),
        (X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
        (
            CONTENT_DISPOSITION,
            disposition(&format!("thumb-{}", row.filename)),
        ),
    ];
    Ok((StatusCode::OK, reply_headers, plain).into_response())
}

/// RFC 8187 encoding: every byte outside `A-Za-z0-9-._~` becomes `%XX`.
fn percent_encode_filename(name: &str) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(name.len());
    for &b in name.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filenames_cannot_break_out_of_the_header() {
        assert_eq!(
            disposition("a\"b\r\nc ü.txt"),
            "attachment; filename*=UTF-8''a%22b%0D%0Ac%20%C3%BC.txt"
        );
    }
}
