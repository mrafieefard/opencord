//! Files uploaded as soon as they are attached: the composer stages a file
//! and its upload starts. Sending the message takes the upload over,
//! finished or not; removing the file stops it, or cancels it on the server
//! once it is finished.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use opencord_proto::v1 as proto;
use proto::request::Kind as Request;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::upload::{PROGRESS_EVERY, Report, Uploads, cancel_uploads, upload_one};
use crate::api::types::{CoreError, CoreEventPayload, OutgoingFile, UploadPhase};
use crate::client::Client;

/// How a staged upload stands.
#[derive(Debug, Clone)]
pub(crate) enum StagedState {
    Running {
        sent: u64,
        total: u64,
        phase: UploadPhase,
    },
    Ready(proto::Attachment),
    /// Failed or stopped: sending the file uploads it again.
    Gone,
}

/// A staged upload, as the message that sends it finds it.
pub(crate) struct Staged {
    token: CancellationToken,
    state: watch::Receiver<StagedState>,
}

impl Staged {
    /// Waits for the upload, passing on how far it got; its attachment, or
    /// None when it failed or stopped. Dropped before then (the message's
    /// send was cancelled), it stops the upload.
    pub(super) async fn finish(mut self, report: &impl Report) -> Option<proto::Attachment> {
        let stop_if_dropped = self.token.clone().drop_guard();
        loop {
            let state = self.state.borrow_and_update().clone();
            match state {
                StagedState::Ready(attachment) => {
                    stop_if_dropped.disarm();
                    report.sized(attachment.size_bytes);
                    report.sent(attachment.size_bytes);
                    return Some(attachment);
                }
                StagedState::Gone => {
                    stop_if_dropped.disarm();
                    return None;
                }
                StagedState::Running { sent, total, phase } => {
                    report.sized(total);
                    if phase == UploadPhase::Uploading {
                        report.sent(sent);
                    } else {
                        report.phase(phase);
                    }
                }
            }
            if self.state.changed().await.is_err() {
                // The upload ended; its last state says how.
                stop_if_dropped.disarm();
                return match &*self.state.borrow() {
                    StagedState::Ready(attachment) => Some(attachment.clone()),
                    _ => None,
                };
            }
        }
    }
}

type StagedMap = std::collections::HashMap<(String, String), Staged>;

impl Uploads {
    fn lock_staged(&self) -> MutexGuard<'_, StagedMap> {
        self.staged.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The staged upload, now the message's: removing the file no longer
    /// touches it.
    pub(crate) fn take_staged(&self, key: &str, staged_id: &str) -> Option<Staged> {
        self.lock_staged()
            .remove(&(key.to_owned(), staged_id.to_owned()))
    }
}

/// Uploads `file`, staged in the composer as `staged_id`, with
/// `StagedUploadProgress` events as it goes. Returns once it is ready to be
/// sent, or failed, or was discarded.
pub(crate) async fn stage(
    client: &Client,
    key: &str,
    channel_id: i64,
    staged_id: String,
    file: OutgoingFile,
) -> Result<(), CoreError> {
    if file.path.is_some() == file.bytes.is_some() {
        return Err(CoreError::InvalidInput {
            message: format!("{} needs a path or bytes, not both", file.filename),
        });
    }
    let uploads = client.uploads();
    let id = (key.to_owned(), staged_id.clone());
    let (state, receiver) = watch::channel(StagedState::Running {
        sent: 0,
        total: 0,
        phase: UploadPhase::Preparing,
    });
    let token = CancellationToken::new();
    {
        let mut staged = uploads.lock_staged();
        if staged.contains_key(&id) {
            return Err(CoreError::InvalidInput {
                message: "this file is already being uploaded".to_owned(),
            });
        }
        staged.insert(
            id.clone(),
            Staged {
                token: token.clone(),
                state: receiver,
            },
        );
    }
    let report = StagedReport {
        client,
        key,
        channel_id,
        staged_id: &staged_id,
        state: &state,
        total: AtomicU64::new(0),
        last: Mutex::new(None),
    };
    let created = Mutex::new(Vec::new());
    let result = tokio::select! {
        () = token.cancelled() => Err(CoreError::Cancelled),
        result = upload_one(file, proto::AttachmentKind::File, &report, &created) => result,
    };
    match result {
        Ok(attachment) => {
            state.send_replace(StagedState::Ready(attachment));
            report.event(UploadPhase::Done);
            Ok(())
        }
        Err(error) => {
            let created = created.into_inner().unwrap_or_else(PoisonError::into_inner);
            cancel_uploads(client, key, &created).await;
            state.send_replace(StagedState::Gone);
            uploads.lock_staged().remove(&id);
            report.event(match &error {
                CoreError::Cancelled => UploadPhase::Cancelled,
                other => UploadPhase::Failed {
                    message: other.to_string(),
                },
            });
            Err(error)
        }
    }
}

/// Stops the staged upload, or cancels it on the server when it is ready;
/// nothing once a message has taken it over.
pub(crate) async fn discard(client: &Client, key: &str, staged_id: &str) {
    let Some(staged) = client
        .uploads()
        .lock_staged()
        .remove(&(key.to_owned(), staged_id.to_owned()))
    else {
        return;
    };
    staged.token.cancel();
    let ready = match &*staged.state.borrow() {
        StagedState::Ready(attachment) => Some(attachment.id),
        _ => None,
    };
    if let Some(upload_id) = ready {
        let request = Request::CancelUpload(proto::CancelUpload { upload_id });
        if let Err(error) = client.request(key, request).await {
            tracing::warn!(upload_id, %error, "a staged upload couldn't be cancelled");
        }
    }
}

/// A staged upload's progress: its state for the message that may take it
/// over, and events for the composer.
struct StagedReport<'a> {
    client: &'a Client,
    key: &'a str,
    channel_id: i64,
    staged_id: &'a str,
    state: &'a watch::Sender<StagedState>,
    total: AtomicU64,
    /// When the last uploading event went out.
    last: Mutex<Option<Instant>>,
}

impl StagedReport<'_> {
    fn event(&self, phase: UploadPhase) {
        let (sent_bytes, total_bytes) = match &*self.state.borrow() {
            StagedState::Running { sent, total, .. } => (*sent, *total),
            StagedState::Ready(attachment) => (attachment.size_bytes, attachment.size_bytes),
            StagedState::Gone => (0, self.total.load(Ordering::Relaxed)),
        };
        self.client.emit(
            self.key,
            CoreEventPayload::StagedUploadProgress {
                channel_id: self.channel_id,
                staged_id: self.staged_id.to_owned(),
                sent_bytes,
                total_bytes,
                phase,
            },
        );
    }
}

impl Report for StagedReport<'_> {
    fn client(&self) -> &Client {
        self.client
    }

    fn key(&self) -> &str {
        self.key
    }

    fn channel_id(&self) -> i64 {
        self.channel_id
    }

    fn sized(&self, total: u64) {
        self.total.store(total, Ordering::Relaxed);
        self.state.send_modify(|state| {
            if let StagedState::Running { total: known, .. } = state {
                *known = total;
            }
        });
    }

    /// Bytes sent so far; events at most 10 times a second.
    fn sent(&self, sent: u64) {
        self.state.send_modify(|state| {
            if let StagedState::Running {
                sent: known, phase, ..
            } = state
            {
                *known = sent;
                *phase = UploadPhase::Uploading;
            }
        });
        let total = self.total.load(Ordering::Relaxed);
        let due = {
            let mut last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
            let due = sent == total || last.is_none_or(|at| at.elapsed() >= PROGRESS_EVERY);
            if due {
                *last = Some(Instant::now());
            }
            due
        };
        if due {
            self.event(UploadPhase::Uploading);
        }
    }

    fn phase(&self, phase: UploadPhase) {
        self.state.send_modify(|state| {
            if let StagedState::Running { phase: known, .. } = state {
                *known = phase.clone();
            }
        });
        self.event(phase);
    }
}
