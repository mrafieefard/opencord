//! Background job processing for finished uploads.

use std::path::Path;
use std::sync::Arc;

use opencord_common::blob::{self, BlobKey, BlobKind, HEADER_LEN, SEGMENT_PLAIN, SEGMENT_SEALED};
use opencord_common::sniff::{SniffKind, Sniffed, sniff};
use opencord_common::upload::{IMAGE_PROCESS_MAX_BYTES, VOICE_FILENAME, voice_max_bytes};
use opencord_proto::v1 as proto;
use tokio::io::AsyncReadExt;

use crate::attachments::http::attachment_to_proto;
use crate::attachments::storage::{ObjectKind, object_key};
use crate::attachments::{images, temp, voice};
use crate::db;
use crate::db::attachments::ReadyFields;
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
    let mut conn = state.db.acquire().await?;
    let row = match db::attachments::find(&mut conn, id).await? {
        Some(r) => r,
        None => return Err(ApiError::not_found("upload")),
    };
    drop(conn);

    let uploader_id = row.uploader_id;
    let temp_path = state.attachments.temp_path(id);

    match run_inner(state, id, &row, &temp_path).await {
        Ok(()) => {
            tokio::fs::remove_file(&temp_path).await.ok();
            Ok(())
        }
        Err(err) => {
            let now = crate::state::now_ms();
            if let Ok(mut conn) = state.db.acquire().await {
                let _ =
                    db::attachments::set_failed(&mut conn, id, err.code as i32, &err.message, now)
                        .await;
            }
            tokio::fs::remove_file(&temp_path).await.ok();
            state.broadcast(
                proto::event::Kind::UploadFailed(proto::UploadFailed {
                    upload_id: id,
                    error: Some(err.to_proto()),
                }),
                Audience::User(uploader_id),
            );
            Err(err)
        }
    }
}

async fn run_inner(
    state: &Arc<AppState>,
    id: i64,
    row: &db::attachments::AttachmentRow,
    temp_path: &Path,
) -> Result<(), ApiError> {
    // 1. Open key
    let key_bytes = state
        .certificates
        .secrets()
        .open(&format!("attachment-key:{id}"), &row.key_enc)
        .map_err(|e| {
            tracing::error!(upload_id = id, ?e, "failed to open attachment key");
            ApiError::internal()
        })?;
    let key_arr: [u8; 32] = key_bytes.try_into().map_err(|_| ApiError::internal())?;
    let key = BlobKey::from_bytes(key_arr);

    // 2. Read first segment's plaintext -> sniff
    let total = row.size_bytes as u64;
    let first_plain = read_first_segment_plain(temp_path, &key, id, total)
        .await
        .map_err(|e| {
            tracing::error!(upload_id = id, ?e, "failed to read first segment plaintext");
            ApiError::internal()
        })?;
    let sniffed = sniff(&first_plain);

    // 3. Branching
    let (ready_fields, thumbnail_bytes) = if row.kind == "voice" {
        if sniffed != Sniffed::Ogg {
            return Err(ApiError::voice_message_invalid("not an ogg file"));
        }
        let max_sec = {
            let guild = state.guild();
            guild.upload_settings.voice_message_max_seconds
        };
        let bytes = temp::read_plain(temp_path, &key, id, total, voice_max_bytes(max_sec))
            .await
            .map_err(|e| ApiError::voice_message_invalid(e.to_string()))?;
        let voice_info = voice::check(&bytes, max_sec)?;
        let ready = ReadyFields {
            kind: "voice".to_string(),
            content_type: "audio/ogg".to_string(),
            filename: VOICE_FILENAME.to_string(),
            storage_id: 1,
            width: None,
            height: None,
            duration_ms: Some(voice_info.duration_ms as i64),
            animated: false,
            waveform: Some(voice_info.waveform),
            has_thumbnail: false,
            thumbnail_width: None,
            thumbnail_height: None,
        };
        (ready, None)
    } else if sniffed.kind() == SniffKind::Image && total <= IMAGE_PROCESS_MAX_BYTES {
        let bytes = temp::read_plain(temp_path, &key, id, total, IMAGE_PROCESS_MAX_BYTES)
            .await
            .map_err(|e| {
                tracing::error!(upload_id = id, ?e, "failed to read image plaintext");
                ApiError::internal()
            })?;
        let process_res = tokio::task::spawn_blocking(move || images::process(&bytes))
            .await
            .map_err(|_| ApiError::internal())?;
        match process_res {
            Ok(img) => {
                let ready = ReadyFields {
                    kind: "image".to_string(),
                    content_type: sniffed.content_type().to_string(),
                    filename: row.filename.clone(),
                    storage_id: 1,
                    width: Some(img.width as i64),
                    height: Some(img.height as i64),
                    duration_ms: None,
                    animated: img.animated,
                    waveform: None,
                    has_thumbnail: true,
                    thumbnail_width: Some(img.thumb_w as i64),
                    thumbnail_height: Some(img.thumb_h as i64),
                };
                (ready, Some(img.thumbnail))
            }
            Err(_) => {
                let ready = ReadyFields {
                    kind: "file".to_string(),
                    content_type: sniffed.content_type().to_string(),
                    filename: row.filename.clone(),
                    storage_id: 1,
                    width: None,
                    height: None,
                    duration_ms: None,
                    animated: false,
                    waveform: None,
                    has_thumbnail: false,
                    thumbnail_width: None,
                    thumbnail_height: None,
                };
                (ready, None)
            }
        }
    } else if sniffed.kind() == SniffKind::Video {
        let poster = state.attachments.take_poster(id);
        let thumb_opt = if let Some(poster_bytes) = poster {
            tokio::task::spawn_blocking(move || images::thumbnail_from_poster(&poster_bytes))
                .await
                .ok()
                .and_then(|r| r.ok())
        } else {
            None
        };
        let (has_thumbnail, thumb_w, thumb_h, thumb_bytes) = match thumb_opt {
            Some((bytes, w, h)) => (true, Some(w as i64), Some(h as i64), Some(bytes)),
            None => (false, None, None, None),
        };
        let ready = ReadyFields {
            kind: "video".to_string(),
            content_type: sniffed.content_type().to_string(),
            filename: row.filename.clone(),
            storage_id: 1,
            width: row.width,
            height: row.height,
            duration_ms: row.duration_ms,
            animated: false,
            waveform: None,
            has_thumbnail,
            thumbnail_width: thumb_w,
            thumbnail_height: thumb_h,
        };
        (ready, thumb_bytes)
    } else if sniffed.kind() == SniffKind::Audio {
        let ready = ReadyFields {
            kind: "audio".to_string(),
            content_type: sniffed.content_type().to_string(),
            filename: row.filename.clone(),
            storage_id: 1,
            width: None,
            height: None,
            duration_ms: row.duration_ms,
            animated: false,
            waveform: None,
            has_thumbnail: false,
            thumbnail_width: None,
            thumbnail_height: None,
        };
        (ready, None)
    } else {
        let ready = ReadyFields {
            kind: "file".to_string(),
            content_type: sniffed.content_type().to_string(),
            filename: row.filename.clone(),
            storage_id: 1,
            width: None,
            height: None,
            duration_ms: None,
            animated: false,
            waveform: None,
            has_thumbnail: false,
            thumbnail_width: None,
            thumbnail_height: None,
        };
        (ready, None)
    };

    // 4. Active storage backend
    let (active_id, backend) = state.attachments.active();
    let mut ready_fields = ready_fields;
    ready_fields.storage_id = active_id;

    // 5. Store file & thumbnail in backend
    backend
        .put_file(&object_key(ObjectKind::File, id), temp_path)
        .await?;

    if let Some(thumb) = thumbnail_bytes {
        let sealed_thumb = blob::seal_all(&key, BlobKind::Thumbnail, id, &thumb);
        backend
            .put_bytes(&object_key(ObjectKind::Thumbnail, id), sealed_thumb)
            .await?;
    }

    // 6. Set ready in DB
    let now = crate::state::now_ms();
    let mut conn = state.db.acquire().await?;
    db::attachments::set_ready(&mut conn, id, &ready_fields, now).await?;

    // Reload row to produce accurate proto
    let updated_row = db::attachments::find(&mut conn, id)
        .await?
        .ok_or_else(|| ApiError::not_found("attachment"))?;
    let attachment_proto = attachment_to_proto(&updated_row);

    // 7. Broadcast UploadReady to Audience::User(uploader)
    state.broadcast(
        proto::event::Kind::UploadReady(proto::UploadReady {
            attachment: Some(attachment_proto),
        }),
        Audience::User(row.uploader_id),
    );

    Ok(())
}

async fn read_first_segment_plain(
    path: &Path,
    key: &BlobKey,
    id: i64,
    total: u64,
) -> std::io::Result<Vec<u8>> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut header = [0u8; HEADER_LEN as usize];
    file.read_exact(&mut header).await?;
    let last = total <= SEGMENT_PLAIN;
    let sealed_len = if total == 0 {
        40
    } else if last {
        24 + (total as usize) + 16
    } else {
        SEGMENT_SEALED as usize
    };
    let mut sealed = vec![0u8; sealed_len];
    file.read_exact(&mut sealed).await?;
    blob::open_segment(key, BlobKind::File, id, 0, last, &sealed)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
}
