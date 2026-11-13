//! A file a plugin view's picker chose (plan M2 Task 15, design §13.3):
//! uploaded for the view, not a channel, through the same pieces and
//! checks as a message's file. The server tells the plugin it arrived.

use std::sync::{Mutex, PoisonError};

use opencord_proto::v1 as proto;

use super::upload::{Report, cancel_uploads, upload_one};
use crate::api::types::{CoreError, OutgoingFile, UploadPhase};
use crate::client::Client;

/// Uploads the file at `path` for the server's view `view_id` on `key`;
/// the name the server kept and its size.
pub(crate) async fn upload(
    client: &Client,
    key: &str,
    view_id: i64,
    path: String,
) -> Result<(String, u64), CoreError> {
    let report = ViewReport {
        client,
        key,
        view_id,
    };
    let file = OutgoingFile {
        path: Some(path),
        bytes: None,
        filename: String::new(),
        staged_id: None,
    };
    let created = Mutex::new(Vec::new());
    match upload_one(file, proto::AttachmentKind::File, &report, &created).await {
        Ok(attachment) => Ok((attachment.filename, attachment.size_bytes)),
        Err(error) => {
            let created = created.into_inner().unwrap_or_else(PoisonError::into_inner);
            cancel_uploads(client, key, &created).await;
            Err(error)
        }
    }
}

/// A view's upload: no channel, and no progress events (the picker shows
/// a spinner until its answer).
struct ViewReport<'a> {
    client: &'a Client,
    key: &'a str,
    view_id: i64,
}

impl Report for ViewReport<'_> {
    fn client(&self) -> &Client {
        self.client
    }

    fn key(&self) -> &str {
        self.key
    }

    fn channel_id(&self) -> i64 {
        0
    }

    fn view_id(&self) -> Option<i64> {
        Some(self.view_id)
    }

    fn sized(&self, _total: u64) {}

    fn sent(&self, _sent: u64) {}

    fn phase(&self, _phase: UploadPhase) {}
}
