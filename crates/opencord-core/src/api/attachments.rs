//! Files in messages, upload settings and storage (media uploads plan,
//! milestone A4), for the Flutter app.

use super::client::on_runtime;
use super::types::{
    CoreError, S3Config, StorageInfo, StorageTestResult, StorageTestTarget, UploadSettings,
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
