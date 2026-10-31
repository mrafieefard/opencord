//! Background job processing for finished uploads.

use std::path::Path;
use std::sync::Arc;

use opencord_common::blob::{self, BlobKey, BlobKind, SEGMENT_PLAIN};
use opencord_common::mp4;
use opencord_common::sniff::{SniffKind, Sniffed, sniff};
use opencord_common::upload::{
    IMAGE_PROCESS_MAX_BYTES, VOICE_FILENAME, VOICE_MAX_SECONDS, voice_max_bytes,
};
use opencord_proto::v1 as proto;

use crate::attachments::http::attachment_to_proto;
use crate::attachments::storage::{Backend, ObjectKind, StorageError, object_key};
use crate::attachments::{images, open_key, temp, voice};
use crate::db;
use crate::db::attachments::{AttachmentRow, ReadyFields};
use crate::db::local_songs::SongRow;
use crate::error::ApiError;
use crate::music::local;
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
    if row.kind == "music" {
        return music_upload(state, row, &key, temp_path).await;
    }
    let size = row.size_bytes as u64;
    let first = temp::read_range(temp_path, &key, id, size, 0, size.min(SEGMENT_PLAIN))
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
    // Sound alone in an MP4 is audio, whatever the brand says.
    let audio_only = matches!(sniffed, Sniffed::Mp4 | Sniffed::QuickTime)
        && mp4_has_video(temp_path, &key, id, size).await == Some(false);

    let (mut fields, thumbnail) = if row.kind == "voice" {
        (voice_fields(row, &key, temp_path, sniffed).await?, None)
    } else if sniffed.kind() == SniffKind::Image && row.size_bytes as u64 <= IMAGE_PROCESS_MAX_BYTES
    {
        image_fields(row, &key, temp_path, sniffed).await?
    } else if audio_only {
        let fields = ReadyFields {
            duration_ms: row.duration_ms,
            ..base_fields("audio", Sniffed::M4a, &row.filename)
        };
        (fields, None)
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

/// How long a song may take to check.
const SONG_CHECK_TIME: std::time::Duration = std::time::Duration::from_secs(120);
/// Shorter songs are refused.
const SONG_MIN_MS: u64 = 1_000;

/// A song for the music player (design 2026-10-24): checked by decoding all
/// of it, then kept once per server by its SHA-256. A song the server
/// already has is not stored again; its uploader becomes an owner.
async fn music_upload(
    state: &Arc<AppState>,
    row: &AttachmentRow,
    key: &BlobKey,
    temp_path: &Path,
) -> Result<(), ApiError> {
    let id = row.id;
    let size = row.size_bytes as u64;
    let (plain, sha256) = local::open_upload(state, key, id, size, temp_path)
        .await
        .map_err(|error| {
            tracing::error!(upload_id = id, ?error, "an uploaded song failed to open");
            ApiError::internal()
        })?;
    let path = plain.path().to_owned();
    let probed = tokio::time::timeout(
        SONG_CHECK_TIME,
        tokio::task::spawn_blocking(move || local::decode::probe(&path)),
    )
    .await
    .map_err(|_| ApiError::invalid_argument("The song took too long to read"))?
    .map_err(|_| ApiError::internal())?
    .map_err(|error| ApiError::invalid_argument(capitalized(&error.to_string())))?;
    drop(plain);
    if probed.duration_ms < SONG_MIN_MS {
        return Err(ApiError::invalid_argument(
            "A song must last at least a second",
        ));
    }

    let mut conn = state.db.acquire().await?;
    let known = db::local_songs::find_by_hash(&mut conn, &sha256).await?;
    drop(conn);
    if let Some(song) = known {
        return keep_known(state, row, song.id).await;
    }
    let Some((storage_id, backend)) = choose_storage(state, row).await? else {
        return Ok(());
    };
    if let Err(error) = store(&backend, key, id, temp_path, None).await {
        discard_stored(state, storage_id, id, false).await;
        return Err(error.into());
    }
    let now = crate::state::now_ms();
    let fields = ReadyFields {
        storage_id,
        duration_ms: i64::try_from(probed.duration_ms).ok(),
        ..base_fields("music", Sniffed::Unknown, &row.filename)
    };
    let song = SongRow {
        id,
        sha256: sha256.to_vec(),
        size_bytes: row.size_bytes,
        filename: row.filename.clone(),
        title: probed.title.unwrap_or_else(|| {
            row.filename
                .rsplit_once('.')
                .map_or(row.filename.as_str(), |(stem, _)| stem)
                .to_owned()
        }),
        artist: probed
            .artist
            .unwrap_or_else(|| crate::music::display_name(state, row.uploader_id)),
        duration_ms: i64::try_from(probed.duration_ms).unwrap_or(i64::MAX),
        uploader_id: row.uploader_id,
        created_at: now,
    };
    let mut tx = state.db.begin().await?;
    if !db::attachments::set_ready(&mut tx, id, &fields, now).await? {
        // Cancelled while it was checked.
        drop(tx);
        discard_stored(state, storage_id, id, false).await;
        return Ok(());
    }
    db::attachments::set_state(&mut tx, id, "music", now).await?;
    match db::local_songs::insert(&mut tx, &song).await {
        Ok(()) => {}
        // The same song finished meanwhile from another upload.
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            drop(tx);
            discard_stored(state, storage_id, id, false).await;
            let mut conn = state.db.acquire().await?;
            let known = db::local_songs::find_by_hash(&mut conn, &sha256).await?;
            drop(conn);
            return match known {
                Some(known) => keep_known(state, row, known.id).await,
                None => Err(ApiError::internal()),
            };
        }
        Err(error) => return Err(error.into()),
    }
    db::local_songs::add_owner(&mut tx, id, row.uploader_id, now).await?;
    tx.commit().await?;

    let mut conn = state.db.acquire().await?;
    let stored = db::attachments::find(&mut conn, id).await?;
    drop(conn);
    if let Some(stored) = stored {
        state.broadcast(
            proto::event::Kind::UploadReady(proto::UploadReady {
                attachment: Some(attachment_to_proto(&stored)),
            }),
            Audience::User(row.uploader_id),
        );
    }
    Ok(())
}

/// The server has the song already: its uploader owns it too, and this
/// upload goes.
async fn keep_known(state: &AppState, row: &AttachmentRow, song_id: i64) -> Result<(), ApiError> {
    let mut conn = state.db.acquire().await?;
    db::local_songs::add_owner(&mut conn, song_id, row.uploader_id, crate::state::now_ms()).await?;
    db::attachments::delete(&mut conn, row.id).await?;
    state.broadcast(
        proto::event::Kind::UploadReady(proto::UploadReady {
            attachment: Some(attachment_to_proto(row)),
        }),
        Audience::User(row.uploader_id),
    );
    Ok(())
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
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
    row: &AttachmentRow,
    key: &BlobKey,
    temp_path: &Path,
    sniffed: Sniffed,
) -> Result<ReadyFields, ApiError> {
    if sniffed != Sniffed::Ogg {
        return Err(ApiError::voice_message_invalid("not an ogg file"));
    }
    let size = row.size_bytes as u64;
    if size > voice_max_bytes(VOICE_MAX_SECONDS) {
        return Err(ApiError::voice_message_too_long(VOICE_MAX_SECONDS));
    }
    let bytes = temp::read_plain(temp_path, key, row.id, size, size)
        .await
        .map_err(|error| {
            tracing::error!(upload_id = row.id, ?error, "a voice message failed to open");
            ApiError::internal()
        })?;
    // Decoding minutes of Opus is CPU work.
    let info = tokio::task::spawn_blocking(move || voice::check(&bytes, VOICE_MAX_SECONDS))
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

/// The most of an MP4's track list read to tell its sound from a video;
/// a longer one stays a video.
const MOOV_MOST: u64 = 16 * 1024 * 1024;
/// The top-level boxes looked through for the track list.
const TOP_BOXES_MOST: usize = 64;

/// Whether an MP4 upload holds a picture, from its track list (`moov`),
/// which may come before or after the media; `None` when it can't be found
/// or read within those bounds.
async fn mp4_has_video(path: &Path, key: &BlobKey, id: i64, total: u64) -> Option<bool> {
    let mut offset = 0u64;
    for _ in 0..TOP_BOXES_MOST {
        let left = total.checked_sub(offset)?;
        let head = temp::read_range(path, key, id, total, offset, left.min(16))
            .await
            .ok()?;
        let header = mp4::box_header(&head)?;
        let size = header.size.unwrap_or(left);
        if header.kind == *b"moov" {
            let len = size.min(left).checked_sub(header.header_len)?;
            if len > MOOV_MOST {
                return None;
            }
            let moov = temp::read_range(path, key, id, total, offset + header.header_len, len)
                .await
                .ok()?;
            return mp4::has_video_track(&moov);
        }
        offset = offset.checked_add(size)?;
    }
    None
}
