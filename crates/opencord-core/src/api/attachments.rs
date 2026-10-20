//! Files in messages, upload settings and storage (media uploads plan,
//! milestone A4), for the Flutter app.

use flutter_rust_bridge::frb;

use super::client::{client, on_blocking, on_runtime};
use super::types::{
    AttachmentVariant, CoreError, MediaHandle, Message, OutgoingFile, PlaybackItem, S3Config,
    StorageInfo, StorageTestResult, StorageTestTarget, UploadSettings, VoiceDraft,
};
use crate::attachments::clipboard;

/// MANAGE_SERVER. `None` leaves a setting as it is.
pub async fn update_upload_settings(
    server_key: String,
    blocked_extensions: Option<Vec<String>>,
) -> Result<UploadSettings, CoreError> {
    on_runtime(move |client| async move {
        client
            .update_upload_settings(&server_key, blocked_extensions)
            .await
    })
    .await
}

/// The storage requests are the owner's only.
pub async fn storage_get(server_key: String) -> Result<StorageInfo, CoreError> {
    on_runtime(move |client| async move { client.storage_get(&server_key).await }).await
}

/// Tests the bucket first; refused when the test fails.
pub async fn storage_add(
    server_key: String,
    name: String,
    s3: S3Config,
) -> Result<StorageInfo, CoreError> {
    on_runtime(move |client| async move { client.storage_add(&server_key, name, s3).await }).await
}

/// An empty secret keeps the saved one.
pub async fn storage_update(
    server_key: String,
    storage_id: i64,
    name: Option<String>,
    s3: Option<S3Config>,
) -> Result<StorageInfo, CoreError> {
    on_runtime(move |client| async move {
        client
            .storage_update(&server_key, storage_id, name, s3)
            .await
    })
    .await
}

pub async fn storage_test(
    server_key: String,
    target: StorageTestTarget,
) -> Result<StorageTestResult, CoreError> {
    on_runtime(move |client| async move { client.storage_test(&server_key, target).await }).await
}

/// New files go there from now on.
pub async fn storage_activate(
    server_key: String,
    storage_id: i64,
) -> Result<StorageInfo, CoreError> {
    on_runtime(move |client| async move { client.storage_activate(&server_key, storage_id).await })
        .await
}

pub async fn storage_remove(server_key: String, storage_id: i64) -> Result<StorageInfo, CoreError> {
    on_runtime(move |client| async move { client.storage_remove(&server_key, storage_id).await })
        .await
}

/// Moves every file to the active storage; StorageUpdate events follow it.
pub async fn storage_move_start(server_key: String) -> Result<StorageInfo, CoreError> {
    on_runtime(move |client| async move { client.storage_move_start(&server_key).await }).await
}

pub async fn storage_move_stop(server_key: String) -> Result<StorageInfo, CoreError> {
    on_runtime(move |client| async move { client.storage_move_stop(&server_key).await }).await
}

/// Uploads the files, two at a time, then sends the message with them.
/// UploadProgress events follow each file. On a failure or a cancel, the
/// uploads made so far are cancelled.
pub async fn send_message_with_files(
    server_key: String,
    channel_id: i64,
    content: String,
    nonce: String,
    files: Vec<OutgoingFile>,
) -> Result<Message, CoreError> {
    on_runtime(move |client| async move {
        client
            .send_message_with_files(&server_key, channel_id, content, nonce, files)
            .await
    })
    .await
}

/// Stops a send that is still uploading; it returns `Cancelled`.
#[frb(sync)]
pub fn cancel_send(server_key: String, nonce: String) -> Result<(), CoreError> {
    client()?.cancel_send(&server_key, &nonce);
    Ok(())
}

/// Starts recording a voice message.
#[frb(sync)]
pub fn voice_record_start(max_seconds: u32) -> Result<(), CoreError> {
    client()?.voice_record_start(max_seconds)
}

/// Stops recording and returns the drafted voice message.
#[frb(sync)]
pub fn voice_record_stop() -> Result<VoiceDraft, CoreError> {
    client()?.voice_record_stop()
}

/// Cancels the recording in progress without saving a draft.
#[frb(sync)]
pub fn voice_record_cancel() -> Result<(), CoreError> {
    client()?.voice_record_cancel();
    Ok(())
}

/// Discards a drafted voice message.
#[frb(sync)]
pub fn voice_draft_discard(draft_id: u64) -> Result<(), CoreError> {
    client()?.voice_draft_discard(draft_id);
    Ok(())
}

/// Uploads and sends a drafted voice message.
pub async fn send_voice_message(
    server_key: String,
    channel_id: i64,
    nonce: String,
    draft_id: u64,
) -> Result<Message, CoreError> {
    on_runtime(move |client| async move {
        client
            .send_voice_message(&server_key, channel_id, nonce, draft_id)
            .await
    })
    .await
}

/// Plays a voice message (attachment or draft) starting from `position_ms` at `speed`.
pub async fn voice_play(item: PlaybackItem, position_ms: u32, speed: f32) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.voice_play(item, position_ms, speed).await }).await
}

#[frb(sync)]
pub fn voice_pause() -> Result<(), CoreError> {
    client()?.voice_pause();
    Ok(())
}

#[frb(sync)]
pub fn voice_resume() -> Result<(), CoreError> {
    client()?.voice_resume();
    Ok(())
}

#[frb(sync)]
pub fn voice_seek(position_ms: u32) -> Result<(), CoreError> {
    client()?.voice_seek(position_ms);
    Ok(())
}

#[frb(sync)]
pub fn voice_set_speed(speed: f32) -> Result<(), CoreError> {
    client()?.voice_set_speed(speed)
}

#[frb(sync)]
pub fn voice_stop() -> Result<(), CoreError> {
    client()?.voice_stop();
    Ok(())
}

pub async fn media_open(server_key: String, attachment_id: i64) -> Result<MediaHandle, CoreError> {
    on_runtime(move |client| async move { client.media_open(server_key, attachment_id).await })
        .await
}

#[frb(sync)]
pub fn media_play() -> Result<(), CoreError> {
    client()?.media_play();
    Ok(())
}

#[frb(sync)]
pub fn media_pause() -> Result<(), CoreError> {
    client()?.media_pause();
    Ok(())
}

#[frb(sync)]
pub fn media_seek(position_ms: u32) -> Result<(), CoreError> {
    client()?.media_seek(position_ms);
    Ok(())
}

#[frb(sync)]
pub fn media_set_volume(volume: f32, muted: bool) -> Result<(), CoreError> {
    client()?.media_set_volume(volume, muted);
    Ok(())
}

#[frb(sync)]
pub fn media_close() -> Result<(), CoreError> {
    client()?.media_close();
    Ok(())
}

#[frb(sync)]
pub fn media_supported() -> bool {
    opencord_media::player::supported()
}

/// Saves the attachment at `path`; DownloadProgress events follow it.
pub async fn download_attachment(
    server_key: String,
    attachment_id: i64,
    path: String,
) -> Result<(), CoreError> {
    on_runtime(move |client| async move {
        client
            .download_attachment(&server_key, attachment_id, path)
            .await
    })
    .await
}

#[frb(sync)]
pub fn cancel_download(server_key: String, attachment_id: i64) -> Result<(), CoreError> {
    client()?.cancel_download(&server_key, attachment_id);
    Ok(())
}

/// A thumbnail, or an original of at most 64 MiB, through the cache. Only
/// for images, voice messages and audio; other files use
/// `download_attachment`, videos the media player.
pub async fn attachment_bytes(
    server_key: String,
    attachment_id: i64,
    variant: AttachmentVariant,
) -> Result<Vec<u8>, CoreError> {
    on_runtime(move |client| async move {
        client
            .attachment_bytes(&server_key, attachment_id, variant)
            .await
    })
    .await
}

/// Turns the attachment cache on, with the 32-byte key the app keeps in
/// the system keychain. Without it nothing is cached on disk.
#[frb(sync)]
pub fn media_cache_init(key: Vec<u8>) -> Result<(), CoreError> {
    client()?.media_cache_init(&key)
}

pub async fn media_cache_size() -> Result<u64, CoreError> {
    on_runtime(move |client| async move { client.media_cache_size().await }).await
}

pub async fn media_cache_clear() -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.media_cache_clear().await }).await
}

/// The clipboard's image as PNG; `None` when it holds text or nothing.
pub async fn clipboard_image() -> Result<Option<Vec<u8>>, CoreError> {
    on_blocking(clipboard::image).await
}

/// Puts an image (PNG, JPEG, GIF or WebP) on the clipboard.
pub async fn clipboard_set_image(bytes: Vec<u8>) -> Result<(), CoreError> {
    on_blocking(move || clipboard::set_image(&bytes)).await
}
