//! Background job processing for finished uploads.

use std::path::Path;
use std::sync::Arc;

use opencord_common::blob::{self, BlobKey, BlobKind, HEADER_LEN, SEGMENT_OVERHEAD, SEGMENT_PLAIN};
use opencord_common::sniff::{SniffKind, Sniffed, sniff};
use opencord_common::upload::{IMAGE_PROCESS_MAX_BYTES, VOICE_FILENAME, voice_max_bytes};
use opencord_proto::v1 as proto;
use tokio::io::AsyncReadExt;

use crate::attachments::http::attachment_to_proto;
use crate::attachments::storage::{Backend, ObjectKind, StorageError, object_key};
use crate::attachments::{images, open_key, temp, voice};
use crate::db;
use crate::db::attachments::{AttachmentRow, ReadyFields};
use crate::error::ApiError;
use crate::state::{AppState, Audience};

pub fn spawn_job(state: Arc<AppState>, id: i64) {
    tokio::spawn(async move {
        if state.attachments.is_processing_held() {
            return;
        }
        let _permit = match state.attachments.jobs().acquire_owned().await {
            Ok(p) => p,
            Err(_) => return,
        };
        if state.attachments.is_processing_held() {
            return;
        }
        if let Err(err) = run(&state, id).await {
            tracing::warn!(upload_id = id, ?err, "attachment processing failed");
        }
    });
}

pub async fn run(state: &Arc<AppState>, id: i64) -> Result<(), ApiError> {
    // Only a video uses its poster, but no poster may outlive its job.
    let poster = state.attachments.take_poster(id);
    let mut conn = state.db.acquire().await?;
    let row = db::attachments::find(&mut conn, id).await?;
    drop(conn);
    // Cancelled while waiting for a job slot.
    let Some(row) = row.filter(|row| row.state == "processing") else {
        return Ok(());
    };

    let temp_path = state.attachments.temp_path(id);
    let result = run_inner(state, &row, poster, &temp_path).await;
    tokio::fs::remove_file(&temp_path).await.ok();
    if let Err(err) = &result {
        let now = crate::state::now_ms();
        let failed = async {
            let mut conn = state.db.acquire().await?;
            db::attachments::set_failed(&mut conn, id, err.code as i32, &err.message, now).await
        };
        if let Err(error) = failed.await {
            tracing::error!(
                upload_id = id,
                ?error,
                "a failed upload couldn't be marked failed"
            );
        }
        state.broadcast(
            proto::event::Kind::UploadFailed(proto::UploadFailed {
                upload_id: id,
                error: Some(err.to_proto()),
            }),
            Audience::User(row.uploader_id),
        );
    }
    result
}

async fn run_inner(
    state: &Arc<AppState>,
    row: &AttachmentRow,
    poster: Option<Vec<u8>>,
    temp_path: &Path,
) -> Result<(), ApiError> {
    let id = row.id;
    let key = open_key(state.certificates.secrets(), row)?;
    let first = read_first_segment(temp_path, &key, id, row.size_bytes as u64)
        .await
        .map_err(|error| {
            tracing::error!(
                upload_id = id,
                ?error,
                "an upload's first segment failed to open"
            );
            ApiError::internal()
        })?;
    let sniffed = sniff(&first);

    let (mut fields, thumbnail) = if row.kind == "voice" {
        (
            voice_fields(state, row, &key, temp_path, sniffed).await?,
            None,
        )
    } else if sniffed.kind() == SniffKind::Image && row.size_bytes as u64 <= IMAGE_PROCESS_MAX_BYTES
    {
        image_fields(row, &key, temp_path, sniffed).await?
    } else if sniffed.kind() == SniffKind::Video {
        video_fields(row, sniffed, poster).await
    } else if sniffed.kind() == SniffKind::Audio {
        let fields = ReadyFields {
            duration_ms: row.duration_ms,
            ..base_fields("audio", sniffed, &row.filename)
        };
        (fields, None)
    } else {
        (base_fields("file", sniffed, &row.filename), None)
    };

    let Some((storage_id, backend)) = choose_storage(state, row).await? else {
        // Cancelled while processing; nothing was stored.
        return Ok(());
    };
    fields.storage_id = storage_id;
    let has_thumbnail = thumbnail.is_some();
    if let Err(error) = store(&backend, &key, id, temp_path, thumbnail).await {
        discard_stored(state, storage_id, id, has_thumbnail).await;
        return Err(error.into());
    }

    let mut conn = state.db.acquire().await?;
    if !db::attachments::set_ready(&mut conn, id, &fields, crate::state::now_ms()).await? {
        // Cancelled while processing. Deleting the row queued only what it
        // knew of, so queue what was just stored.
        discard_stored(state, storage_id, id, has_thumbnail).await;
        return Ok(());
    }
    let row = db::attachments::find(&mut conn, id)
        .await?
        .ok_or_else(|| ApiError::not_found("upload"))?;
    state.broadcast(
        proto::event::Kind::UploadReady(proto::UploadReady {
            attachment: Some(attachment_to_proto(&row)),
        }),
        Audience::User(row.uploader_id),
    );
    Ok(())
}

/// The active storage, written on the row before anything is stored there,
/// under the write lock. So a storage that is changed or removed counts this
/// file even while it is being written (`handlers/storage.rs`). None when the
/// upload was cancelled.
async fn choose_storage(
    state: &AppState,
    row: &AttachmentRow,
) -> Result<Option<(i64, Arc<Backend>)>, ApiError> {
    let _writes = state.write_lock().await;
    let (storage_id, backend) = state.attachments.active()?;
    let mut conn = state.db.acquire().await?;
    if !db::attachments::set_storage_if(&mut conn, row.id, row.storage_id, storage_id).await? {
        return Ok(None);
    }
    Ok(Some((storage_id, backend)))
}

/// Ready fields with nothing measured yet.
fn base_fields(kind: &str, sniffed: Sniffed, filename: &str) -> ReadyFields {
    ReadyFields {
        kind: kind.to_owned(),
        content_type: sniffed.content_type().to_owned(),
        filename: filename.to_owned(),
        storage_id: 0,
        width: None,
        height: None,
        duration_ms: None,
        animated: false,
        waveform: None,
        has_thumbnail: false,
        thumbnail_width: None,
        thumbnail_height: None,
    }
}

async fn voice_fields(
    state: &AppState,
    row: &AttachmentRow,
    key: &BlobKey,
    temp_path: &Path,
    sniffed: Sniffed,
) -> Result<ReadyFields, ApiError> {
    if sniffed != Sniffed::Ogg {
        return Err(ApiError::voice_message_invalid("not an ogg file"));
    }
    let max_seconds = state.guild().upload_settings.voice_message_max_seconds;
    let size = row.size_bytes as u64;
    // The owner may have shortened the limit since the upload began.
    if size > voice_max_bytes(max_seconds) {
        return Err(ApiError::voice_message_too_long(max_seconds));
    }
    let bytes = temp::read_plain(temp_path, key, row.id, size, size)
        .await
        .map_err(|error| {
            tracing::error!(upload_id = row.id, ?error, "a voice message failed to open");
            ApiError::internal()
        })?;
    // Decoding minutes of Opus is CPU work.
    let info = tokio::task::spawn_blocking(move || voice::check(&bytes, max_seconds))
        .await
        .map_err(|_| ApiError::internal())??;
    Ok(ReadyFields {
        content_type: "audio/ogg".to_owned(),
        duration_ms: Some(info.duration_ms.into()),
        waveform: Some(info.waveform),
        ..base_fields("voice", sniffed, VOICE_FILENAME)
    })
}

async fn image_fields(
    row: &AttachmentRow,
    key: &BlobKey,
    temp_path: &Path,
    sniffed: Sniffed,
) -> Result<(ReadyFields, Option<Vec<u8>>), ApiError> {
    let bytes = temp::read_plain(
        temp_path,
        key,
        row.id,
        row.size_bytes as u64,
        IMAGE_PROCESS_MAX_BYTES,
    )
    .await
    .map_err(|error| {
        tracing::error!(upload_id = row.id, ?error, "an image failed to open");
        ApiError::internal()
    })?;
    let processed = tokio::task::spawn_blocking(move || images::process(&bytes))
        .await
        .map_err(|_| ApiError::internal())?;
    let base = base_fields("image", sniffed, &row.filename);
    Ok(match processed {
        Ok(image) => {
            let fields = ReadyFields {
                width: Some(image.width.into()),
                height: Some(image.height.into()),
                animated: image.animated,
                has_thumbnail: true,
                thumbnail_width: Some(image.thumb_w.into()),
                thumbnail_height: Some(image.thumb_h.into()),
                ..base
            };
            (fields, Some(image.thumbnail))
        }
        // A picture that won't decode is still a file people can download.
        Err(error) => {
            tracing::debug!(upload_id = row.id, %error, "an image stays a file");
            let fields = ReadyFields {
                kind: "file".to_owned(),
                ..base
            };
            (fields, None)
        }
    })
}

async fn video_fields(
    row: &AttachmentRow,
    sniffed: Sniffed,
    poster: Option<Vec<u8>>,
) -> (ReadyFields, Option<Vec<u8>>) {
    let thumbnail = match poster {
        Some(poster) => tokio::task::spawn_blocking(move || images::thumbnail_from_poster(&poster))
            .await
            .ok()
            .and_then(Result::ok),
        None => None,
    };
    let base = ReadyFields {
        width: row.width,
        height: row.height,
        duration_ms: row.duration_ms,
        ..base_fields("video", sniffed, &row.filename)
    };
    match thumbnail {
        Some((bytes, width, height)) => {
            let fields = ReadyFields {
                has_thumbnail: true,
                thumbnail_width: Some(width.into()),
                thumbnail_height: Some(height.into()),
                ..base
            };
            (fields, Some(bytes))
        }
        None => (base, None),
    }
}

async fn store(
    backend: &Backend,
    key: &BlobKey,
    id: i64,
    temp_path: &Path,
    thumbnail: Option<Vec<u8>>,
) -> Result<(), StorageError> {
    backend
        .put_file(&object_key(ObjectKind::File, id), temp_path)
        .await?;
    if let Some(thumbnail) = thumbnail {
        let sealed = blob::seal_all(key, BlobKind::Thumbnail, id, &thumbnail);
        backend
            .put_bytes(&object_key(ObjectKind::Thumbnail, id), sealed)
            .await?;
    }
    Ok(())
}

/// Queues what a job may have stored for an upload that won't keep it; the
/// blob worker deletes it and retries on failure.
async fn discard_stored(state: &AppState, storage_id: i64, id: i64, thumbnail: bool) {
    let queued = async {
        let mut conn = state.db.acquire().await?;
        let file = object_key(ObjectKind::File, id);
        db::storage::queue_blob(&mut conn, storage_id, &file).await?;
        if thumbnail {
            let thumbnail = object_key(ObjectKind::Thumbnail, id);
            db::storage::queue_blob(&mut conn, storage_id, &thumbnail).await?;
        }
        Ok::<_, sqlx::Error>(())
    };
    if let Err(error) = queued.await {
        tracing::error!(
            upload_id = id,
            ?error,
            "stored objects couldn't be queued for deletion"
        );
    }
}

/// The plaintext of segment 0: enough to sniff the type.
async fn read_first_segment(
    path: &Path,
    key: &BlobKey,
    id: i64,
    total: u64,
) -> std::io::Result<Vec<u8>> {
    let mut file = tokio::fs::File::open(path).await?;
    let sealed_len = HEADER_LEN + SEGMENT_OVERHEAD + total.min(SEGMENT_PLAIN);
    let mut sealed = vec![0u8; sealed_len as usize];
    file.read_exact(&mut sealed).await?;
    let last = total <= SEGMENT_PLAIN;
    blob::open_segment(
        key,
        BlobKind::File,
        id,
        0,
        last,
        &sealed[HEADER_LEN as usize..],
    )
    .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string()))
}
