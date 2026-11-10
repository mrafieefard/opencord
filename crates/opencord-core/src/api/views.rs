//! Plugin views for the Flutter app (plugins milestone 2 plan, Task 15):
//! the channels plugins draw, their batches and inputs, and the files
//! their renderer reads.

use flutter_rust_bridge::frb;

use super::client::{client, on_runtime, runtime};
use super::types::{Channel, CoreError, ViewOutput};
use crate::frb_generated::StreamSink;

/// Every plugin view's batches, as JSON in the wire format. Opening it
/// again replaces the earlier stream; nothing is kept while none is open.
/// A view's first batches can come before `view_open` returns its id.
pub fn view_output_stream(sink: StreamSink<ViewOutput>) -> Result<(), CoreError> {
    let client = client()?;
    let runtime = runtime()?;
    let mut outputs = client.view_outputs();
    runtime.spawn(async move {
        while let Some(output) = outputs.recv().await {
            if sink.add(output).is_err() {
                break;
            }
        }
    });
    Ok(())
}

/// What `Identify` lists in `features` from the next connection on;
/// `["plugin_users"]` until called. The desktop adds `"plugin_channels"`
/// before connecting.
#[frb(sync)]
pub fn set_plugin_features(features: Vec<String>) -> Result<(), CoreError> {
    client()?.set_plugin_features(features)
}

/// Opens the plugin channel `channel_id`; the core's id for the view,
/// kept when it opens again on a new session. `context_json` is the
/// renderer's first `context` input, without `kind`.
pub async fn view_open(
    server_key: String,
    channel_id: i64,
    context_json: String,
) -> Result<String, CoreError> {
    on_runtime(move |client| async move {
        client
            .view_open(&server_key, channel_id, &context_json)
            .await
    })
    .await
}

/// An input from the view's renderer, as JSON in the wire format.
#[frb(sync)]
pub fn view_input(view_id: String, json: String) -> Result<(), CoreError> {
    client()?.view_input(&view_id, &json);
    Ok(())
}

/// Closes the view; nothing more arrives for it.
#[frb(sync)]
pub fn view_close(view_id: String) -> Result<(), CoreError> {
    client()?.view_close(&view_id);
    Ok(())
}

/// The cached file of a plugin's asset under `assets/`. An empty `sha256`
/// takes the hash its views were given, or learns it by fetching.
pub async fn plugin_asset_file(
    server_key: String,
    plugin_id: String,
    path: String,
    sha256: String,
) -> Result<String, CoreError> {
    on_runtime(move |client| async move {
        client
            .plugin_asset_file(&server_key, &plugin_id, &path, &sha256)
            .await
    })
    .await
}

/// A file holding the image an `image` block names: its source (`asset`,
/// `file`, `file_token`, `url` or `bytes`) and `version`, as JSON. A new
/// version is a new path. `attachment` is refused: read it with
/// `attachment_bytes`.
pub async fn plugin_image_file(
    server_key: String,
    plugin_id: String,
    source_json: String,
) -> Result<String, CoreError> {
    on_runtime(move |client| async move {
        client
            .plugin_image_file(&server_key, &plugin_id, &source_json)
            .await
    })
    .await
}

/// Uploads the file a view's picker (block `node_id`) chose; the picker
/// hears `upload` or `upload_error` through the view. Fails only for a
/// view that isn't open.
pub async fn view_upload(view_id: String, node_id: u64, path: String) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.view_upload(&view_id, node_id, path).await }).await
}

/// A channel of the type `type_key` a plugin running on the server
/// registered.
pub async fn create_plugin_channel(
    server_key: String,
    plugin_id: String,
    type_key: String,
    name: String,
    topic: Option<String>,
    parent_id: Option<i64>,
) -> Result<Channel, CoreError> {
    on_runtime(move |client| async move {
        client
            .create_plugin_channel(&server_key, plugin_id, type_key, name, topic, parent_id)
            .await
    })
    .await
}
