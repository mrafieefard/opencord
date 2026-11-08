//! Plugins for the Flutter app.

use super::client::on_runtime;
use super::types::{
    CoreError, CreatedPluginDevToken, Plugin, PluginConfigValue, PluginDevToken, PluginLogLine,
    PluginReview,
};

pub async fn upload_plugin_package(
    server_key: String,
    bytes: Vec<u8>,
) -> Result<PluginReview, CoreError> {
    on_runtime(move |client| async move { client.upload_plugin_package(&server_key, bytes).await })
        .await
}

pub async fn fetch_plugins(server_key: String) -> Result<Vec<Plugin>, CoreError> {
    on_runtime(move |client| async move { client.fetch_plugins(&server_key).await }).await
}

pub async fn fetch_plugin_reviews(server_key: String) -> Result<Vec<PluginReview>, CoreError> {
    on_runtime(move |client| async move { client.fetch_plugin_reviews(&server_key).await }).await
}

pub async fn confirm_plugin_review(
    server_key: String,
    review_id: String,
    granted: Vec<String>,
    config: Vec<PluginConfigValue>,
    accept_license: bool,
) -> Result<Plugin, CoreError> {
    on_runtime(move |client| async move {
        client
            .confirm_plugin_review(&server_key, review_id, granted, config, accept_license)
            .await
    })
    .await
}

pub async fn discard_plugin_review(server_key: String, review_id: String) -> Result<(), CoreError> {
    on_runtime(
        move |client| async move { client.discard_plugin_review(&server_key, review_id).await },
    )
    .await
}

pub async fn set_plugin_permissions(
    server_key: String,
    plugin_id: String,
    granted: Vec<String>,
) -> Result<Plugin, CoreError> {
    on_runtime(move |client| async move {
        client
            .set_plugin_permissions(&server_key, plugin_id, granted)
            .await
    })
    .await
}

pub async fn set_plugin_config(
    server_key: String,
    plugin_id: String,
    config: Vec<PluginConfigValue>,
) -> Result<Plugin, CoreError> {
    on_runtime(move |client| async move {
        client
            .set_plugin_config(&server_key, plugin_id, config)
            .await
    })
    .await
}

pub async fn set_plugin_running(
    server_key: String,
    plugin_id: String,
    running: bool,
) -> Result<Plugin, CoreError> {
    on_runtime(move |client| async move {
        client
            .set_plugin_running(&server_key, plugin_id, running)
            .await
    })
    .await
}

pub async fn restart_plugin(server_key: String, plugin_id: String) -> Result<Plugin, CoreError> {
    on_runtime(move |client| async move { client.restart_plugin(&server_key, plugin_id).await })
        .await
}

pub async fn uninstall_plugin(
    server_key: String,
    plugin_id: String,
    delete_data: bool,
) -> Result<(), CoreError> {
    on_runtime(move |client| async move {
        client
            .uninstall_plugin(&server_key, plugin_id, delete_data)
            .await
    })
    .await
}

pub async fn fetch_plugin_log(
    server_key: String,
    plugin_id: String,
    before: Option<u64>,
    limit: u32,
) -> Result<Vec<PluginLogLine>, CoreError> {
    on_runtime(move |client| async move {
        client
            .fetch_plugin_log(&server_key, plugin_id, before, limit)
            .await
    })
    .await
}

pub async fn watch_plugin_log(
    server_key: String,
    plugin_id: String,
    watch: bool,
) -> Result<(), CoreError> {
    on_runtime(
        move |client| async move { client.watch_plugin_log(&server_key, plugin_id, watch).await },
    )
    .await
}

pub async fn create_plugin_dev_token(
    server_key: String,
    plugin_id: String,
    days: u32,
) -> Result<CreatedPluginDevToken, CoreError> {
    on_runtime(move |client| async move {
        client
            .create_plugin_dev_token(&server_key, plugin_id, days)
            .await
    })
    .await
}

pub async fn revoke_plugin_dev_token(server_key: String, token_id: i64) -> Result<(), CoreError> {
    on_runtime(
        move |client| async move { client.revoke_plugin_dev_token(&server_key, token_id).await },
    )
    .await
}

pub async fn fetch_plugin_dev_tokens(server_key: String) -> Result<Vec<PluginDevToken>, CoreError> {
    on_runtime(move |client| async move { client.fetch_plugin_dev_tokens(&server_key).await }).await
}
