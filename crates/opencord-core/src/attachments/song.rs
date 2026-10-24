//! Songs dropped on the music player (design 2026-10-24). A file the server
//! already has, by its SHA-256, is queued at once; another is uploaded
//! first, with `MusicUploadProgress` events and a way to cancel it.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Instant;

use opencord_proto::v1 as proto;
use proto::request::Kind as Request;
use proto::response::Result as Response;
use sha2::{Digest, Sha256};

use super::upload::{PROGRESS_EVERY, Report, cancel_uploads, upload_one};
use crate::api::types::{CoreError, CoreEventPayload, MusicPlacement, OutgoingFile, UploadPhase};
use crate::client::{Client, unexpected};

/// Read at a time while hashing.
const HASH_CHUNK: usize = 1 << 20;

/// Queues the song at `path` in the voice channel, uploading it first when
/// the server doesn't have it; `play_now` plays it at once instead of last.
/// `upload_id` is the caller's, for its events and `cancel_song`.
pub(crate) async fn upload_song(
    client: &Client,
    key: &str,
    channel_id: i64,
    upload_id: String,
    path: String,
    play_now: bool,
) -> Result<(), CoreError> {
    let uploads = client.uploads();
    let guard = uploads.begin(key, &upload_id)?;
    let filename = Path::new(&path).file_name().map_or_else(
        || "song".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    let report = SongReport {
        client,
        key,
        channel_id,
        upload_id: &upload_id,
        filename,
        total: AtomicU64::new(0),
        sent: AtomicU64::new(0),
        last: Mutex::new(None),
    };
    report.phase(UploadPhase::Preparing);
    let created = Mutex::new(Vec::new());
    let result = tokio::select! {
        () = guard.token.cancelled() => Err(CoreError::Cancelled),
        result = queue(&report, path, play_now, &created) => result,
    };
    match &result {
        Ok(()) => report.phase(UploadPhase::Done),
        Err(error) => {
            let created = created.into_inner().unwrap_or_else(PoisonError::into_inner);
            cancel_uploads(client, key, &created).await;
            report.phase(match error {
                CoreError::Cancelled => UploadPhase::Cancelled,
                other => UploadPhase::Failed {
                    message: other.to_string(),
                },
            });
        }
    }
    result
}

/// Stops a song's upload; nothing once it is queued.
pub(crate) fn cancel_song(client: &Client, key: &str, upload_id: &str) {
    client.uploads().cancel(key, upload_id);
}

async fn queue(
    report: &SongReport<'_>,
    path: String,
    play_now: bool,
    created: &Mutex<Vec<i64>>,
) -> Result<(), CoreError> {
    let (sha256, size) = hash(path.clone()).await?;
    report.sized(size);
    let result_id = match find(report, &sha256, size).await? {
        Some(result_id) => result_id,
        None => {
            let file = OutgoingFile {
                path: Some(path),
                bytes: None,
                filename: report.filename.clone(),
                staged_id: None,
            };
            upload_one(file, proto::AttachmentKind::Music, report, created).await?;
            // Kept on the server now: nothing to cancel any more.
            created
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clear();
            find(report, &sha256, size)
                .await?
                .ok_or_else(|| CoreError::InvalidInput {
                    message: "The server didn't keep the song".to_owned(),
                })?
        }
    };
    let placement = if play_now {
        MusicPlacement::Now
    } else {
        MusicPlacement::End
    };
    report
        .client
        .music_enqueue(
            report.key,
            report.channel_id,
            vec![result_id],
            Vec::new(),
            None,
            placement,
        )
        .await?;
    Ok(())
}

/// The file's SHA-256 and size, read off the async runtime.
async fn hash(path: String) -> Result<([u8; 32], u64), CoreError> {
    tokio::task::spawn_blocking(move || {
        use std::io::Read;
        let unreadable = |error: std::io::Error| CoreError::InvalidInput {
            message: format!("{path} can't be read: {error}"),
        };
        let mut file = std::fs::File::open(&path).map_err(unreadable)?;
        let mut hash = Sha256::new();
        let mut buffer = vec![0u8; HASH_CHUNK];
        let mut size = 0u64;
        loop {
            let read = file.read(&mut buffer).map_err(unreadable)?;
            if read == 0 {
                break;
            }
            hash.update(&buffer[..read]);
            size += read as u64;
        }
        Ok((hash.finalize().into(), size))
    })
    .await
    .map_err(|error| CoreError::InvalidInput {
        message: error.to_string(),
    })?
}

/// A search result for the song when the server has it.
async fn find(
    report: &SongReport<'_>,
    sha256: &[u8; 32],
    size: u64,
) -> Result<Option<String>, CoreError> {
    let request = Request::MusicFindLocal(proto::MusicFindLocal {
        channel_id: report.channel_id,
        sha256: sha256.to_vec(),
        size_bytes: size,
    });
    match report.client.request(report.key, request).await? {
        Response::MusicLocalMatch(found) => Ok(found.result.map(|result| result.result_id)),
        other => Err(unexpected(&other)),
    }
}

/// A song's progress, as events for the music player.
struct SongReport<'a> {
    client: &'a Client,
    key: &'a str,
    channel_id: i64,
    upload_id: &'a str,
    filename: String,
    total: AtomicU64,
    sent: AtomicU64,
    /// When the last uploading event went out.
    last: Mutex<Option<Instant>>,
}

impl SongReport<'_> {
    fn event(&self, phase: UploadPhase) {
        self.client.emit(
            self.key,
            CoreEventPayload::MusicUploadProgress {
                channel_id: self.channel_id,
                upload_id: self.upload_id.to_owned(),
                filename: self.filename.clone(),
                sent_bytes: self.sent.load(Ordering::Relaxed),
                total_bytes: self.total.load(Ordering::Relaxed),
                phase,
            },
        );
    }
}

impl Report for SongReport<'_> {
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
    }

    /// Bytes sent so far; events at most 10 times a second.
    fn sent(&self, sent: u64) {
        self.sent.store(sent, Ordering::Relaxed);
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
        self.event(phase);
    }
}
