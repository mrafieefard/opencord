// SPDX-License-Identifier: MPL-2.0

//! Uploads from plugin views (plan M2 Task 12, design 2026-11-07 §13.3).
//!
//! A view's picker uploads through the member's own upload path: the row is
//! theirs, with channel id 0, under the same checks, size limits and
//! counts as a message's file. Which plugin and view it is for is kept
//! here. Once it is ready, the plugin hears `file_uploaded`; `files.put`
//! may then copy it into the plugin's files within
//! [`VIEW_UPLOAD_KEPT_MS`] of it finishing, which deletes it. Unkept, the
//! attachments sweep deletes it after that hour (`db::attachments::expired`).

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use sqlx::SqliteConnection;
use tokio::io::AsyncWriteExt as _;

use crate::attachments::http::{batch_end, read_plain};
use crate::attachments::open_key;
use crate::db::attachments::{self, AttachmentRow, VIEW_UPLOAD_KEPT_MS};
use crate::plugins::PluginEvent;
use crate::plugins::events::id;
use crate::plugins::permissions::PluginPermission;
use crate::state::AppState;

/// Whose view an upload is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewUpload {
    pub plugin_id: String,
    pub view_id: i64,
    /// The member who uploads it.
    pub user_id: i64,
}

/// The uploads made for plugin views, by upload id. In memory only.
///
/// **An entry leaves** when its plugin keeps the upload (`files.put`), when
/// its row is deleted (the member cancels it, or the sweep: an hour after it
/// finished, a day while unfinished, an hour after it failed), when its
/// plugin is uninstalled, and when the server restarts (its row then goes
/// with the sweep, unannounced). A plugin's restart or stop keeps them: its
/// uploads stay keepable for their hour. A view closing keeps them too: the
/// upload is the member's file, and the plugin may already have heard of it.
#[derive(Debug, Default)]
pub struct ViewUploads {
    uploads: Mutex<HashMap<i64, ViewUpload>>,
}

impl ViewUploads {
    pub fn insert(&self, upload_id: i64, upload: ViewUpload) {
        self.lock().insert(upload_id, upload);
    }

    pub fn get(&self, upload_id: i64) -> Option<ViewUpload> {
        self.lock().get(&upload_id).cloned()
    }

    pub fn forget(&self, upload_id: i64) {
        self.lock().remove(&upload_id);
    }

    /// The plugin was uninstalled.
    pub fn forget_plugin(&self, plugin_id: &str) {
        self.lock()
            .retain(|_, upload| upload.plugin_id != plugin_id);
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<i64, ViewUpload>> {
        self.uploads.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The upload just became ready: if it is for a view, its plugin hears
/// `file_uploaded` with the view and the file, while it takes uploads. The
/// view must still be open (`view_calls`); after it closed, nothing is
/// announced and the upload waits out its hour.
pub fn finished(state: &AppState, row: &AttachmentRow) {
    let Some(upload) = state.attachments.view_uploads().get(row.id) else {
        return;
    };
    let takes = state
        .plugins
        .get(&upload.plugin_id)
        .is_some_and(|handle| handle.holds(PluginPermission::AcceptUploads));
    tracing::debug!(
        plugin = %upload.plugin_id,
        view = upload.view_id,
        upload_id = row.id,
        size = row.size_bytes,
        announced = takes,
        "a view's upload finished"
    );
    if !takes {
        return;
    }
    state.plugins.deliver(
        &upload.plugin_id,
        PluginEvent {
            name: "file_uploaded".to_owned(),
            payload: serde_json::json!({
                "view_id": upload.view_id,
                "file": {
                    "id": id(row.id),
                    "name": row.filename,
                    "size": row.size_bytes,
                    "content_type": row.content_type,
                },
            }),
            reply: None,
            command: None,
        },
    );
}

/// The upload, if `plugin_id` may keep it now: one of its views' uploads,
/// finished, and finished at most [`VIEW_UPLOAD_KEPT_MS`] ago.
pub async fn keepable(
    state: &AppState,
    conn: &mut SqliteConnection,
    plugin_id: &str,
    upload_id: i64,
    now_ms: i64,
) -> Result<Option<AttachmentRow>, sqlx::Error> {
    let Some(upload) = state
        .attachments
        .view_uploads()
        .get(upload_id)
        .filter(|upload| upload.plugin_id == plugin_id)
    else {
        return Ok(None);
    };
    let row = attachments::find(conn, upload_id).await?;
    Ok(row.filter(|row| {
        row.state == "ready"
            && row.channel_id == 0
            && row.uploader_id == upload.user_id
            && row.updated_at >= now_ms - VIEW_UPLOAD_KEPT_MS
    }))
}

/// Writes the upload's bytes to `file`, a batch of segments at a time. A
/// failure to read the stored upload is logged here; the plugin is told
/// only that it couldn't be read.
pub async fn copy_to(
    state: &AppState,
    row: &AttachmentRow,
    file: &mut tokio::fs::File,
) -> Result<(), String> {
    const UNREADABLE: &str = "the upload couldn't be read";
    let unreadable = |error: &dyn std::fmt::Display| {
        tracing::warn!(upload_id = row.id, %error, "reading a view upload to keep failed");
        UNREADABLE.to_owned()
    };
    let key = open_key(state.certificates.secrets(), row).map_err(|_| UNREADABLE.to_owned())?;
    let backend = state
        .attachments
        .backend(row.storage_id)
        .ok_or_else(|| unreadable(&"its storage isn't loaded"))?;
    let len = u64::try_from(row.size_bytes).map_err(|error| unreadable(&error))?;
    let mut start = 0;
    while start < len {
        let end = batch_end(start, len - 1);
        let plain = read_plain(&backend, &key, row.id, len, start, end)
            .await
            .map_err(|error| unreadable(&error))?;
        file.write_all(&plain)
            .await
            .map_err(|error| error.to_string())?;
        start = end + 1;
    }
    file.flush().await.map_err(|error| error.to_string())
}

/// The plugin kept the upload: it is no longer the plugin's to keep, and
/// its row and stored file go.
pub async fn kept(state: &AppState, conn: &mut SqliteConnection, upload_id: i64) {
    state.attachments.view_uploads().forget(upload_id);
    if let Err(error) = attachments::delete(conn, upload_id).await {
        // Harmless: it can't be kept again, and the sweep deletes its row
        // within the hour.
        tracing::warn!(upload_id, ?error, "a kept view upload wasn't deleted");
    }
}
