//! Files in messages, upload settings and storage (media uploads plan,
//! milestone A4), for the Flutter app.

use flutter_rust_bridge::frb;

use super::client::{client, on_runtime};
use super::types::{
    CoreError, Message, OutgoingFile, S3Config, StorageInfo, StorageTestResult, StorageTestTarget,
    UploadSettings,
};

/// MANAGE_SERVER. `None` leaves a setting as it is.
pub async fn update_upload_settings(
    server_key: String,
    blocked_extensions: Option<Vec<String>>,
    voice_message_max_seconds: Option<u32>,
) -> Result<UploadSettings, CoreError> {
    on_runtime(move |client| async move {
        client
            .update_upload_settings(&server_key, blocked_extensions, voice_message_max_seconds)
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
