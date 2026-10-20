//! Sending a message with files (media uploads plan, Task 18). Each file
//! goes up in pieces, two files at a time. When every file is ready, the
//! message goes out with their ids.

use std::collections::HashMap;
use std::future::Future;
use std::io::SeekFrom;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use bytes::Bytes;
use futures_util::{StreamExt, TryStreamExt};
use opencord_common::upload::{IMAGE_PROCESS_MAX_BYTES, MAX_ATTACHMENTS, PIECE_BYTES};
use opencord_proto::v1 as proto;
use proto::request::Kind as Request;
use proto::response::Result as Response;
use reqwest::StatusCode;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use super::exif;
use super::transfer::{Call, upload_status};
use crate::api::types::{CoreError, CoreEventPayload, Message, OutgoingFile, UploadPhase};
use crate::client::{Client, unexpected};
use crate::convert;

/// Files going up at once, per message.
const AT_ONCE: usize = 2;
/// Uploading progress goes out at most this often, per file.
pub(super) const PROGRESS_EVERY: Duration = Duration::from_millis(100);
const PIECE_TIMEOUT: Duration = Duration::from_secs(120);
const STATUS_TIMEOUT: Duration = Duration::from_secs(30);
/// How often a file being processed is asked about, in case its event was
/// lost with the connection.
const POLL_EVERY: Duration = Duration::from_secs(5);
/// Tries after a network failure, 1, 2, 4, 8 and 16 seconds apart.
const TRIES: u32 = 6;

/// The server's verdict on a finished upload.
type Outcome = Result<proto::Attachment, proto::Error>;

#[derive(Default)]
pub(crate) struct Uploads {
    /// Sends that can still be cancelled, by server key and nonce. An entry
    /// leaves when its send returns.
    sends: Mutex<HashMap<(String, String), CancellationToken>>,
    /// Finished uploads waiting for their verdict, by server key and upload
    /// id. An entry leaves when the verdict comes or its send stops waiting.
    waiters: Mutex<HashMap<(String, i64), oneshot::Sender<Outcome>>>,
    /// Test hook: this many answers to pieces are dropped, as if the network
    /// lost them after the server took the piece.
    lose_answers: Mutex<u32>,
    /// Files the composer staged, uploading or ready, by server key and
    /// staged id. An entry leaves when a message takes it over, when it
    /// fails, or when the file is removed.
    pub(super) staged: Mutex<HashMap<(String, String), super::staged::Staged>>,
}

impl Uploads {
    /// From an `UploadReady` event.
    pub fn on_ready(&self, key: &str, attachment: proto::Attachment) {
        self.settle(key, attachment.id, Ok(attachment));
    }

    /// From an `UploadFailed` event.
    pub fn on_failed(&self, key: &str, upload_id: i64, error: proto::Error) {
        self.settle(key, upload_id, Err(error));
    }

    fn settle(&self, key: &str, upload_id: i64, outcome: Outcome) {
        let waiter = self.lock_waiters().remove(&(key.to_owned(), upload_id));
        if let Some(waiter) = waiter
            && waiter.send(outcome).is_err()
        {
            tracing::debug!(upload_id, "an upload was settled after its send stopped");
        }
    }

    /// Stops the send of the message with this nonce, if it is still
    /// uploading.
    pub fn cancel(&self, key: &str, nonce: &str) {
        if let Some(token) = self.lock_sends().get(&(key.to_owned(), nonce.to_owned())) {
            token.cancel();
        }
    }

    pub fn lose_answers(&self, count: u32) {
        *self
            .lose_answers
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = count;
    }

    fn answer_lost(&self) -> bool {
        let mut left = self
            .lose_answers
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let lost = *left > 0;
        *left = left.saturating_sub(1);
        lost
    }

    fn begin(&self, key: &str, nonce: &str) -> Result<SendGuard<'_>, CoreError> {
        let id = (key.to_owned(), nonce.to_owned());
        let token = CancellationToken::new();
        let mut sends = self.lock_sends();
        if sends.contains_key(&id) {
            return Err(invalid("this message is already being sent"));
        }
        sends.insert(id.clone(), token.clone());
        Ok(SendGuard {
            uploads: self,
            id,
            token,
        })
    }

    fn wait_for(&self, key: &str, upload_id: i64) -> WaitGuard<'_> {
        let id = (key.to_owned(), upload_id);
        let (sender, receiver) = oneshot::channel();
        self.lock_waiters().insert(id.clone(), sender);
        WaitGuard {
            uploads: self,
            id,
            receiver,
        }
    }

    fn lock_sends(&self) -> MutexGuard<'_, HashMap<(String, String), CancellationToken>> {
        self.sends.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn lock_waiters(&self) -> MutexGuard<'_, HashMap<(String, i64), oneshot::Sender<Outcome>>> {
        self.waiters.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A send that can be cancelled until this is dropped.
struct SendGuard<'a> {
    uploads: &'a Uploads,
    id: (String, String),
    token: CancellationToken,
}

impl Drop for SendGuard<'_> {
    fn drop(&mut self) {
        self.uploads.lock_sends().remove(&self.id);
    }
}

/// A verdict to come; nobody waits for it once this is dropped.
struct WaitGuard<'a> {
    uploads: &'a Uploads,
    id: (String, i64),
    receiver: oneshot::Receiver<Outcome>,
}

impl Drop for WaitGuard<'_> {
    fn drop(&mut self) {
        self.uploads.lock_waiters().remove(&self.id);
    }
}

/// Why the uploads stopped.
enum Stop {
    Cancelled,
    Failed { index: usize, error: CoreError },
}

/// Uploads `files` to the server at `key`, then sends the message with
/// them. On a failure or a cancel, every upload made so far is cancelled.
pub(crate) async fn send(
    client: &Client,
    key: &str,
    channel_id: i64,
    content: String,
    nonce: String,
    files: Vec<OutgoingFile>,
    kind: proto::AttachmentKind,
) -> Result<Message, CoreError> {
    if files.is_empty() || files.len() > MAX_ATTACHMENTS {
        return Err(invalid(format!(
            "a message holds 1 to {MAX_ATTACHMENTS} files"
        )));
    }
    if let Some(file) = files
        .iter()
        .find(|file| file.path.is_some() == file.bytes.is_some())
    {
        return Err(invalid(format!(
            "{} needs a path or bytes, not both",
            file.filename
        )));
    }
    let uploads = client.uploads();
    let guard = uploads.begin(key, &nonce)?;
    let progress = Progress {
        client,
        key,
        channel_id,
        nonce: &nonce,
        files: Mutex::new(vec![FileProgress::default(); files.len()]),
    };
    let created = Mutex::new(Vec::new());
    let uploaded = tokio::select! {
        () = guard.token.cancelled() => Err(Stop::Cancelled),
        result = upload_all(files, kind, &progress, &created) => result,
    };
    drop(guard);
    let created = created.into_inner().unwrap_or_else(PoisonError::into_inner);
    let attachments = match uploaded {
        Ok(attachments) => attachments,
        Err(stop) => {
            cancel_uploads(client, key, &created).await;
            return Err(progress.stopped(stop));
        }
    };
    progress.all(UploadPhase::Sending);
    let ids = attachments.iter().map(|attachment| attachment.id).collect();
    match client
        .send_message_with_ids(key, channel_id, content, nonce.clone(), ids)
        .await
    {
        Ok(message) => {
            progress.all(UploadPhase::Done);
            Ok(message)
        }
        Err(error) => {
            cancel_uploads(client, key, &created).await;
            progress.all(UploadPhase::Failed {
                message: error.to_string(),
            });
            Err(error)
        }
    }
}

/// Every file, two at a time; their attachments in the files' order.
async fn upload_all(
    files: Vec<OutgoingFile>,
    kind: proto::AttachmentKind,
    progress: &Progress<'_>,
    created: &Mutex<Vec<i64>>,
) -> Result<Vec<proto::Attachment>, Stop> {
    let mut done: Vec<(usize, proto::Attachment)> =
        futures_util::stream::iter(files.into_iter().enumerate())
            .map(|(index, file)| async move {
                let report = FileReport { progress, index };
                upload_or_take(file, kind, &report, created)
                    .await
                    .map(|attachment| (index, attachment))
                    .map_err(|error| Stop::Failed { index, error })
            })
            .buffer_unordered(AT_ONCE)
            .try_collect()
            .await?;
    done.sort_by_key(|(index, _)| *index);
    Ok(done.into_iter().map(|(_, attachment)| attachment).collect())
}

/// Where one file's upload says how far it got.
pub(super) trait Report: Sync {
    fn client(&self) -> &Client;
    fn key(&self) -> &str;
    fn channel_id(&self) -> i64;
    fn sized(&self, total: u64);
    fn sent(&self, sent: u64);
    fn phase(&self, phase: UploadPhase);
}

/// One file of a message being sent.
struct FileReport<'a> {
    progress: &'a Progress<'a>,
    index: usize,
}

impl Report for FileReport<'_> {
    fn client(&self) -> &Client {
        self.progress.client
    }

    fn key(&self) -> &str {
        self.progress.key
    }

    fn channel_id(&self) -> i64 {
        self.progress.channel_id
    }

    fn sized(&self, total: u64) {
        self.progress.sized(self.index, total);
    }

    fn sent(&self, sent: u64) {
        self.progress.sent(self.index, sent);
    }

    fn phase(&self, phase: UploadPhase) {
        self.progress.phase(self.index, phase);
    }
}

/// The file's staged upload when there is one to take over (going, or
/// done); otherwise it uploads now.
async fn upload_or_take(
    file: OutgoingFile,
    kind: proto::AttachmentKind,
    report: &impl Report,
    created: &Mutex<Vec<i64>>,
) -> Result<proto::Attachment, CoreError> {
    if let Some(staged_id) = &file.staged_id
        && let Some(staged) = report
            .client()
            .uploads()
            .take_staged(report.key(), staged_id)
        && let Some(attachment) = staged.finish(report).await
    {
        created
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(attachment.id);
        return Ok(attachment);
    }
    upload_one(file, kind, report, created).await
}

pub(super) async fn upload_one(
    file: OutgoingFile,
    kind: proto::AttachmentKind,
    report: &impl Report,
    created: &Mutex<Vec<i64>>,
) -> Result<proto::Attachment, CoreError> {
    report.phase(UploadPhase::Preparing);
    // What the sender's probe finds goes with the upload: a video's size,
    // length and poster, a sound's length. The server makes none of them.
    let mut hints = (None, None, None);
    let mut poster_bytes = None;
    if let Some(path_str) = &file.path
        && opencord_media::player::supported()
    {
        let p = Path::new(path_str);
        if let Ok(mut f) = tokio::fs::File::open(p).await {
            let mut head = [0u8; 1024];
            let read = f.read(&mut head).await.unwrap_or(0);
            if matches!(
                opencord_common::sniff::sniff(&head[..read]).kind(),
                opencord_common::sniff::SniffKind::Video | opencord_common::sniff::SniffKind::Audio
            ) {
                let p_buf = p.to_path_buf();
                // Without them the file still uploads; a video's tile then
                // shows a plain box.
                match tokio::task::spawn_blocking(move || {
                    opencord_media::player::probe_and_poster(&p_buf, 800)
                })
                .await
                .unwrap_or_else(|e| Err(opencord_media::player::PlayerError::Open(e.to_string())))
                {
                    Ok((probe, poster)) => {
                        // 0: a length FFmpeg couldn't tell.
                        let duration = (probe.duration_ms > 0).then_some(probe.duration_ms);
                        hints = if probe.has_video {
                            (Some(probe.width), Some(probe.height), duration)
                        } else {
                            (None, None, duration)
                        };
                        if let Some(p) = poster
                            && !p.is_empty()
                            && p.len() <= 2 * 1024 * 1024
                        {
                            poster_bytes = Some(Bytes::from(p));
                        }
                    }
                    Err(error) => tracing::warn!(%error, "a file uploads without its probe"),
                }
            }
        }
    }

    let mut source = Source::prepare(file).await?;
    report.sized(source.size);
    let (width, height, duration_ms) = hints;
    let request = Request::CreateUpload(proto::CreateUpload {
        channel_id: report.channel_id(),
        filename: source.filename.clone(),
        size_bytes: source.size,
        kind: kind as i32,
        width,
        height,
        duration_ms,
    });
    let upload = match report.client().request(report.key(), request).await? {
        Response::Upload(upload) => upload,
        other => return Err(unexpected(&other)),
    };
    created
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(upload.upload_id);
    report.phase(UploadPhase::Uploading);
    send_pieces(
        report.client(),
        report.key(),
        &mut source,
        &upload,
        |sent| {
            report.sent(sent);
        },
    )
    .await?;
    report.phase(UploadPhase::Processing);
    finish(
        report.client(),
        report.key(),
        upload.upload_id,
        poster_bytes,
    )
    .await
}

/// A file to upload, read from memory or from disk.
struct Source {
    filename: String,
    size: u64,
    body: Body,
}

enum Body {
    Bytes(Bytes),
    File(tokio::fs::File),
}

impl Source {
    async fn prepare(file: OutgoingFile) -> Result<Self, CoreError> {
        let OutgoingFile {
            path,
            bytes,
            filename,
            ..
        } = file;
        let named = |fallback: Option<&Path>| {
            let given = filename.trim();
            if !given.is_empty() {
                return given.to_owned();
            }
            fallback.and_then(Path::file_name).map_or_else(
                || "file".to_owned(),
                |name| name.to_string_lossy().into_owned(),
            )
        };
        let (filename, body) = match (path, bytes) {
            (_, Some(bytes)) => (named(None), Body::Bytes(bytes.into())),
            (Some(path), None) => {
                let path = Path::new(&path);
                let unreadable = |error: std::io::Error| {
                    invalid(format!("{} can't be read: {error}", path.display()))
                };
                let file = tokio::fs::File::open(path).await.map_err(unreadable)?;
                (named(Some(path)), Body::File(file))
            }
            (None, None) => return Err(invalid("a file needs a path or bytes")),
        };
        let body = without_metadata(body).await?;
        let size = match &body {
            Body::Bytes(bytes) => bytes.len() as u64,
            Body::File(file) => file
                .metadata()
                .await
                .map_err(|error| invalid(format!("{filename} can't be read: {error}")))?
                .len(),
        };
        Ok(Self {
            filename,
            size,
            body,
        })
    }

    /// `len` bytes from `offset`.
    async fn piece(&mut self, offset: u64, len: u64) -> Result<Bytes, CoreError> {
        let unreadable =
            |error: std::io::Error| invalid(format!("the file can't be read: {error}"));
        match &mut self.body {
            Body::Bytes(bytes) => Ok(bytes.slice(offset as usize..(offset + len) as usize)),
            Body::File(file) => {
                file.seek(SeekFrom::Start(offset))
                    .await
                    .map_err(unreadable)?;
                let mut piece = vec![0; len as usize];
                file.read_exact(&mut piece).await.map_err(unreadable)?;
                Ok(piece.into())
            }
        }
    }
}

/// A photo of at most 100 MiB without its metadata (Task 20), read into
/// memory to strip it. Anything else stays as it is.
async fn without_metadata(body: Body) -> Result<Body, CoreError> {
    let unreadable = |error: std::io::Error| invalid(format!("the file can't be read: {error}"));
    let bytes = match body {
        Body::Bytes(bytes) => bytes,
        Body::File(mut file) => {
            let size = file.metadata().await.map_err(unreadable)?.len();
            let mut head = [0; 12];
            let read = file.read(&mut head).await.map_err(unreadable)?;
            if size > IMAGE_PROCESS_MAX_BYTES || !exif::is_photo(&head[..read]) {
                return Ok(Body::File(file));
            }
            file.seek(SeekFrom::Start(0)).await.map_err(unreadable)?;
            let mut bytes = Vec::with_capacity(size as usize);
            file.read_to_end(&mut bytes).await.map_err(unreadable)?;
            Bytes::from(bytes)
        }
    };
    if bytes.len() as u64 > IMAGE_PROCESS_MAX_BYTES || !exif::is_photo(&bytes) {
        return Ok(Body::Bytes(bytes));
    }
    let stripped = tokio::task::spawn_blocking(move || match exif::strip_metadata(&bytes) {
        Some(stripped) => Bytes::from(stripped),
        None => bytes,
    })
    .await
    .map_err(|error| invalid(format!("the file can't be prepared: {error}")))?;
    Ok(Body::Bytes(stripped))
}

/// Sends the file from the start. A piece whose answer is lost is sent
/// again, and the server's 409 says where it really is.
async fn send_pieces(
    client: &Client,
    key: &str,
    source: &mut Source,
    upload: &proto::Upload,
    mut sent: impl FnMut(u64),
) -> Result<(), CoreError> {
    let piece_bytes = match upload.piece_bytes {
        0 => u64::from(PIECE_BYTES),
        bytes => u64::from(bytes),
    };
    let path = format!("/media/uploads/{}", upload.upload_id);
    let mut offset = 0;
    while offset < source.size {
        let len = (source.size - offset).min(piece_bytes);
        let piece = source.piece(offset, len).await?;
        let status = retrying(|| put_piece(client, key, &path, offset, piece.clone())).await?;
        if status.state != proto::UploadState::Uploading as i32
            || status.received_bytes > source.size
        {
            return Err(invalid("the server stopped taking this file"));
        }
        offset = status.received_bytes;
        sent(offset);
    }
    Ok(())
}

/// The upload's status after the piece: 200 when taken, 409 when the server
/// was elsewhere.
async fn put_piece(
    client: &Client,
    key: &str,
    path: &str,
    offset: u64,
    piece: Bytes,
) -> Result<proto::UploadStatus, CoreError> {
    let call = Call {
        method: reqwest::Method::PUT,
        path: path.to_owned(),
        headers: vec![("Upload-Offset", offset.to_string())],
        body: piece,
        timeout: Some(PIECE_TIMEOUT),
    };
    let response = client.transfers().send(client, key, call).await?;
    let (status, upload) = upload_status(response).await?;
    if status != StatusCode::OK && status != StatusCode::CONFLICT {
        return Err(invalid(format!("the server answered {status}")));
    }
    if client.uploads().answer_lost() {
        return Err(CoreError::Connection {
            message: "the answer was lost (test hook)".to_owned(),
        });
    }
    Ok(upload)
}

/// Finishes the upload and waits for the server's verdict: its event, or
/// what asking every 5 seconds says.
async fn finish(
    client: &Client,
    key: &str,
    upload_id: i64,
    poster: Option<Bytes>,
) -> Result<proto::Attachment, CoreError> {
    let uploads = client.uploads();
    // Before finishing, so a verdict that comes at once isn't missed.
    let mut waiter = uploads.wait_for(key, upload_id);
    let finish_path = format!("/media/uploads/{upload_id}/finish");
    let status = retrying(|| {
        let mut call = Call {
            method: reqwest::Method::POST,
            path: finish_path.clone(),
            headers: Vec::new(),
            body: poster.clone().unwrap_or_default(),
            timeout: Some(STATUS_TIMEOUT),
        };
        if poster.is_some() {
            call.headers.push(("content-type", "image/jpeg".to_owned()));
        }
        status_of(client, key, call)
    })
    .await?;
    if let Some(attachment) = settled(status)? {
        return Ok(attachment);
    }
    let status_path = format!("/media/uploads/{upload_id}");
    loop {
        tokio::select! {
            outcome = &mut waiter.receiver => {
                return match outcome {
                    Ok(Ok(attachment)) => Ok(attachment),
                    Ok(Err(error)) => Err(convert::error(error)),
                    Err(_) => Err(invalid("the upload's verdict was lost")),
                };
            }
            () = tokio::time::sleep(POLL_EVERY) => {
                let status = retrying(|| {
                    status_of(client, key, Call::get(status_path.clone(), Some(STATUS_TIMEOUT)))
                })
                .await?;
                if let Some(attachment) = settled(status)? {
                    return Ok(attachment);
                }
            }
        }
    }
}

async fn status_of(
    client: &Client,
    key: &str,
    call: Call,
) -> Result<proto::UploadStatus, CoreError> {
    let response = client.transfers().send(client, key, call).await?;
    upload_status(response).await.map(|(_, status)| status)
}

/// The attachment once the server is done with the file, `None` while it
/// still processes it.
fn settled(status: proto::UploadStatus) -> Result<Option<proto::Attachment>, CoreError> {
    match proto::UploadState::try_from(status.state) {
        Ok(proto::UploadState::Ready | proto::UploadState::Sent) => status
            .attachment
            .map(Some)
            .ok_or_else(|| invalid("the server's answer has no attachment")),
        Ok(proto::UploadState::Processing) => Ok(None),
        Ok(proto::UploadState::Failed) => Err(status.error.map_or_else(
            || invalid("the server couldn't take this file"),
            convert::error,
        )),
        _ => Err(invalid("the server's answer has no known state")),
    }
}

/// Runs `attempt` again after a network failure, up to six times in all.
async fn retrying<T, F, Fut>(mut attempt: F) -> Result<T, CoreError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, CoreError>>,
{
    let mut tries = 1;
    loop {
        match attempt().await {
            Err(CoreError::Connection { message }) if tries < TRIES => {
                tracing::debug!(%message, tries, "a transfer failed; trying again");
                tokio::time::sleep(Duration::from_secs(1 << (tries - 1))).await;
                tries += 1;
            }
            result => return result,
        }
    }
}

/// Uploads that won't be sent; the server would drop them within a day.
pub(super) async fn cancel_uploads(client: &Client, key: &str, upload_ids: &[i64]) {
    for &upload_id in upload_ids {
        let request = Request::CancelUpload(proto::CancelUpload { upload_id });
        if let Err(error) = client.request(key, request).await {
            tracing::warn!(upload_id, %error, "an upload couldn't be cancelled");
        }
    }
}

#[derive(Debug, Clone, Default)]
struct FileProgress {
    sent: u64,
    total: u64,
    last: Option<Instant>,
}

/// `UploadProgress` events for one message's files.
struct Progress<'a> {
    client: &'a Client,
    key: &'a str,
    channel_id: i64,
    nonce: &'a str,
    files: Mutex<Vec<FileProgress>>,
}

impl Progress<'_> {
    fn sized(&self, index: usize, total: u64) {
        if let Some(file) = self.lock().get_mut(index) {
            file.total = total;
        }
    }

    /// Bytes sent so far, at most 10 times a second.
    fn sent(&self, index: usize, sent: u64) {
        let due = {
            let mut files = self.lock();
            let Some(file) = files.get_mut(index) else {
                return;
            };
            file.sent = sent;
            let due = sent == file.total
                || file
                    .last
                    .is_none_or(|last| last.elapsed() >= PROGRESS_EVERY);
            if due {
                file.last = Some(Instant::now());
            }
            due
        };
        if due {
            self.phase(index, UploadPhase::Uploading);
        }
    }

    fn phase(&self, index: usize, phase: UploadPhase) {
        let (sent_bytes, total_bytes) = self
            .lock()
            .get(index)
            .map_or((0, 0), |file| (file.sent, file.total));
        self.client.emit(
            self.key,
            CoreEventPayload::UploadProgress {
                channel_id: self.channel_id,
                nonce: self.nonce.to_owned(),
                file_index: index as u32,
                sent_bytes,
                total_bytes,
                phase,
            },
        );
    }

    fn all(&self, phase: UploadPhase) {
        // Counted first: a guard in the loop's head would be held through it.
        let count = self.lock().len();
        for index in 0..count {
            self.phase(index, phase.clone());
        }
    }

    /// The file that failed says why; the others were cancelled.
    fn stopped(&self, stop: Stop) -> CoreError {
        match stop {
            Stop::Cancelled => {
                self.all(UploadPhase::Cancelled);
                CoreError::Cancelled
            }
            Stop::Failed { index, error } => {
                let count = self.lock().len();
                for other in (0..count).filter(|other| *other != index) {
                    self.phase(other, UploadPhase::Cancelled);
                }
                self.phase(
                    index,
                    UploadPhase::Failed {
                        message: error.to_string(),
                    },
                );
                error
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, Vec<FileProgress>> {
        self.files.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn invalid(message: impl Into<String>) -> CoreError {
    CoreError::InvalidInput {
        message: message.into(),
    }
}
