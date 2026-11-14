//! Plugin views (plugins design §11, §18): the calls the desktop's Flutter
//! API has in `opencord_core::api::views`, for the phone. JavaScript pulls a
//! view's batches as it pulls events and hands them to the plugin area's
//! renderer; the renderer's inputs come back through `view_input`.

use crate::MobileError;
use crate::convert;
use crate::ids::parse_id;
use crate::mobile::{mobile, on_runtime};
use crate::types::{ActivitySessionRecord, ViewOutputRecord};

/// What `Identify` lists in `features` from the next connection on;
/// `["plugin_users"]` until called. The phone adds `"plugin_channels"` and
/// `"activities"` when its renderer is in the build.
#[uniffi::export]
pub fn set_plugin_features(features: Vec<String>) -> Result<(), MobileError> {
    mobile()?.set_plugin_features(features)
}

/// Starts a new generation of the view stream; a `next_view_outputs` still
/// waiting under an older one returns an empty list at once. Views an
/// earlier JavaScript start left open are closed. Call once per start.
#[uniffi::export]
pub fn view_outputs_attach() -> Result<u64, MobileError> {
    Ok(mobile()?.view_outputs_attach())
}

/// Waits for at least one batch, then returns what is ready, up to `max`
/// (1–256), in order. Empty only when a newer generation has attached. A
/// view's first batches can come before `view_open` returns its id.
#[uniffi::export]
pub async fn next_view_outputs(
    generation: u64,
    max: u32,
) -> Result<Vec<ViewOutputRecord>, MobileError> {
    Ok(mobile()?.next_view_outputs(generation, max).await)
}

/// Opens the plugin channel `channel_id`; the core's id for the view.
/// `context_json` is the renderer's first `context` input, without `kind`.
#[uniffi::export]
pub async fn view_open(
    server_key: String,
    channel_id: String,
    context_json: String,
) -> Result<String, MobileError> {
    let channel_id = parse_id(&channel_id)?;
    let view_id = on_runtime(move |client| async move {
        Ok(client
            .view_open(&server_key, channel_id, &context_json)
            .await?)
    })
    .await?;
    mobile()?.note_view_opened(&view_id);
    Ok(view_id)
}

/// Opens the activity session `session_id`, running in the call
/// `channel_id`; as `view_open`. The server refuses it unless this device
/// is in that call.
#[uniffi::export]
pub async fn view_open_activity(
    server_key: String,
    session_id: String,
    channel_id: String,
    activity_key: String,
    context_json: String,
) -> Result<String, MobileError> {
    let session_id = parse_id(&session_id)?;
    let channel_id = parse_id(&channel_id)?;
    let view_id = on_runtime(move |client| async move {
        Ok(client
            .view_open_activity(
                &server_key,
                session_id,
                channel_id,
                &activity_key,
                &context_json,
            )
            .await?)
    })
    .await?;
    mobile()?.note_view_opened(&view_id);
    Ok(view_id)
}

/// An input from the view's renderer, as JSON (the wire format).
#[uniffi::export]
pub fn view_input(view_id: String, json: String) -> Result<(), MobileError> {
    mobile()?.client().view_input(&view_id, &json);
    Ok(())
}

/// Closes the view; nothing more arrives for it.
#[uniffi::export]
pub fn view_close(view_id: String) -> Result<(), MobileError> {
    let mobile = mobile()?;
    mobile.client().view_close(&view_id);
    mobile.note_view_closed(&view_id);
    Ok(())
}

/// The cached file of a plugin's asset under `assets/`, by the hash its
/// views were given, or learned by fetching.
#[uniffi::export]
pub async fn plugin_asset_file(
    server_key: String,
    plugin_id: String,
    path: String,
) -> Result<String, MobileError> {
    on_runtime(move |client| async move {
        Ok(client
            .plugin_asset_file(&server_key, &plugin_id, &path, "")
            .await?)
    })
    .await
}

/// A file holding the image an `image` block names: its source (`asset`,
/// `file`, `file_token`, `url` or `bytes`) and `version`, as JSON.
/// `attachment` is refused: read it with `attachment_file`.
#[uniffi::export]
pub async fn plugin_image_file(
    server_key: String,
    plugin_id: String,
    source_json: String,
) -> Result<String, MobileError> {
    on_runtime(move |client| async move {
        Ok(client
            .plugin_image_file(&server_key, &plugin_id, &source_json)
            .await?)
    })
    .await
}

/// Uploads the file a view's picker (block `node_id`) chose; the picker
/// hears `upload` or `upload_error` through the view. Fails only for a
/// view that isn't open.
#[uniffi::export]
pub async fn view_upload(view_id: String, node_id: u64, path: String) -> Result<(), MobileError> {
    on_runtime(move |client| async move { Ok(client.view_upload(&view_id, node_id, path).await?) })
        .await
}

/// Starts a plugin's activity in the call `channel_id`, which this device
/// is in; the running session if it already runs there. A fifth activity
/// fails with the server's `plugin_limit`.
#[uniffi::export]
pub async fn activity_start(
    server_key: String,
    channel_id: String,
    plugin_id: String,
    activity_key: String,
) -> Result<ActivitySessionRecord, MobileError> {
    let channel_id = parse_id(&channel_id)?;
    on_runtime(move |client| async move {
        let session = client
            .start_activity(&server_key, channel_id, plugin_id, activity_key)
            .await?;
        Ok(convert::activity_session(session))
    })
    .await
}

/// Ends an activity session for everyone in its call.
#[uniffi::export]
pub async fn activity_end(server_key: String, session_id: String) -> Result<(), MobileError> {
    let session_id = parse_id(&session_id)?;
    on_runtime(move |client| async move { Ok(client.end_activity(&server_key, session_id).await?) })
        .await
}
