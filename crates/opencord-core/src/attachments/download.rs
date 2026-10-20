//! Attachments coming down (media uploads plan, Tasks 18 and 19): small ones whole,
//! for the image viewer and the players, through the cache; any file
//! streamed to where the user saves it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use super::cache::{Entry, MediaCache};
use super::transfer::{Call, network, refusal};
use crate::api::types::{AttachmentVariant, CoreError, CoreEventPayload};
use crate::client::Client;

/// Progress goes out at most this often.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);
/// The most `bytes` reads into memory.
const SHOW_MAX_BYTES: u64 = 64 * 1024 * 1024;
/// Originals up to this size are cached; thumbnails always are.
const CACHE_MAX_BYTES: u64 = 20 * 1024 * 1024;

#[derive(Default)]
pub(crate) struct Downloads {
    /// Saves that can still be cancelled, by server key and attachment id.
    /// An entry leaves when its save returns.
    running: Mutex<HashMap<(String, i64), CancellationToken>>,
}

impl Downloads {
    pub fn cancel(&self, key: &str, attachment_id: i64) {
        if let Some(token) = self.lock().get(&(key.to_owned(), attachment_id)) {
            token.cancel();
        }
    }

    fn begin(&self, key: &str, attachment_id: i64) -> Result<DownloadGuard<'_>, CoreError> {
        let id = (key.to_owned(), attachment_id);
        let token = CancellationToken::new();
        let mut running = self.lock();
        if running.contains_key(&id) {
            return Err(CoreError::InvalidInput {
                message: "this file is already being saved".to_owned(),
            });
        }
        running.insert(id.clone(), token.clone());
        Ok(DownloadGuard {
            downloads: self,
            id,
            token,
        })
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<(String, i64), CancellationToken>> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A save that can be cancelled until this is dropped.
struct DownloadGuard<'a> {
    downloads: &'a Downloads,
    id: (String, i64),
    token: CancellationToken,
}

impl Drop for DownloadGuard<'_> {
    fn drop(&mut self) {
        self.downloads.lock().remove(&self.id);
    }
}

fn path_of(attachment_id: i64, variant: AttachmentVariant) -> String {
    match variant {
        AttachmentVariant::Original => format!("/media/attachments/{attachment_id}"),
        AttachmentVariant::Thumbnail => format!("/media/attachments/{attachment_id}/thumbnail"),
    }
}

/// Streams the attachment to `path`, through `path.part` until it is all
/// there. `DownloadProgress` events follow it, the last with `done` or
/// `error`.
pub(crate) async fn save(
    client: &Client,
    key: &str,
    attachment_id: i64,
    path: String,
) -> Result<(), CoreError> {
    let downloads = client.downloads();
    let guard = downloads.begin(key, attachment_id)?;
    let progress = Progress {
        client,
        key,
        attachment_id,
        last: Mutex::new(None),
    };
    let part = PathBuf::from(format!("{path}.part"));
    let streamed = tokio::select! {
        () = guard.token.cancelled() => Err(CoreError::Cancelled),
        result = stream_to(client, key, attachment_id, &part, &progress) => result,
    };
    drop(guard);
    let saved = match streamed {
        Ok(total) => tokio::fs::rename(&part, &path)
            .await
            .map(|()| total)
            .map_err(|error| CoreError::Storage {
                message: error.to_string(),
            }),
        Err(error) => Err(error),
    };
    match saved {
        Ok(total) => {
            progress.end(total, total, None);
            Ok(())
        }
        Err(error) => {
            if let Err(remove) = tokio::fs::remove_file(&part).await
                && remove.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(error = %remove, "a partial download couldn't be removed");
            }
            progress.end(0, 0, Some(error.to_string()));
            Err(error)
        }
    }
}

/// The file's size once it is all in `part`.
async fn stream_to(
    client: &Client,
    key: &str,
    attachment_id: i64,
    part: &Path,
    progress: &Progress<'_>,
) -> Result<u64, CoreError> {
    let call = Call::get(path_of(attachment_id, AttachmentVariant::Original), None);
    let mut response = client.transfers().send(client, key, call).await?;
    if !response.status().is_success() {
        return Err(refusal(response).await);
    }
    let total = response.content_length().unwrap_or(0);
    let unwritable = |error: std::io::Error| CoreError::Storage {
        message: error.to_string(),
    };
    let mut file = tokio::fs::File::create(part).await.map_err(unwritable)?;
    let mut received = 0;
    while let Some(chunk) = response.chunk().await.map_err(|error| network(&error))? {
        file.write_all(&chunk).await.map_err(unwritable)?;
        received += chunk.len() as u64;
        progress.received(received, total);
    }
    file.sync_all().await.map_err(unwritable)?;
    Ok(received)
}

/// `DownloadProgress` events for one save.
struct Progress<'a> {
    client: &'a Client,
    key: &'a str,
    attachment_id: i64,
    last: Mutex<Option<Instant>>,
}

impl Progress<'_> {
    /// At most 10 times a second.
    fn received(&self, received_bytes: u64, total_bytes: u64) {
        {
            let mut last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
            if last.is_some_and(|last| last.elapsed() < PROGRESS_EVERY) {
                return;
            }
            *last = Some(Instant::now());
        }
        self.emit(received_bytes, total_bytes, false, None);
    }

    fn end(&self, received_bytes: u64, total_bytes: u64, error: Option<String>) {
        let done = error.is_none();
        self.emit(received_bytes, total_bytes, done, error);
    }

    fn emit(&self, received_bytes: u64, total_bytes: u64, done: bool, error: Option<String>) {
        self.client.emit(
            self.key,
            CoreEventPayload::DownloadProgress {
                attachment_id: self.attachment_id,
                received_bytes,
                total_bytes,
                done,
                error,
            },
        );
    }
}

/// A thumbnail, or an original of at most 64 MiB, from the cache when it
/// is there. Only images, voice messages and audio are read this way.
pub(crate) async fn bytes(
    client: &Client,
    key: &str,
    attachment_id: i64,
    variant: AttachmentVariant,
) -> Result<Vec<u8>, CoreError> {
    let entry = Entry::new(key, attachment_id, variant);
    let cache = client.media_cache();
    if let Some(cache) = &cache {
        let (cache, entry) = (Arc::clone(cache), entry.clone());
        if let Some(bytes) = blocking(move || Ok(cache.get(&entry))).await? {
            return Ok(bytes);
        }
    }
    let call = Call::get(path_of(attachment_id, variant), None);
    let mut response = client.transfers().send(client, key, call).await?;
    if !response.status().is_success() {
        return Err(refusal(response).await);
    }
    let too_large = || CoreError::InvalidInput {
        message: "the file is too large to show".to_owned(),
    };
    if response
        .content_length()
        .is_some_and(|length| length > SHOW_MAX_BYTES)
    {
        return Err(too_large());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| network(&error))? {
        if (body.len() + chunk.len()) as u64 > SHOW_MAX_BYTES {
            return Err(too_large());
        }
        body.extend_from_slice(&chunk);
    }
    let worth_keeping =
        variant == AttachmentVariant::Thumbnail || body.len() as u64 <= CACHE_MAX_BYTES;
    if let Some(cache) = cache
        && worth_keeping
    {
        let plain = body.clone();
        let kept = blocking(move || {
            cache
                .put(&entry, &plain)
                .map_err(|error| CoreError::Storage {
                    message: error.to_string(),
                })
        })
        .await;
        if let Err(error) = kept {
            tracing::warn!(%error, "an attachment couldn't be cached");
        }
    }
    Ok(body)
}

/// Runs cache work (file reads and sealing) off the async threads.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, CoreError> + Send + 'static,
) -> Result<T, CoreError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| CoreError::Storage {
            message: error.to_string(),
        })?
}

/// The cache's size, or 0 without one.
pub(crate) async fn cache_size(cache: Option<Arc<MediaCache>>) -> Result<u64, CoreError> {
    let Some(cache) = cache else {
        return Ok(0);
    };
    blocking(move || {
        cache.size().map_err(|error| CoreError::Storage {
            message: error.to_string(),
        })
    })
    .await
}

pub(crate) async fn cache_clear(cache: Option<Arc<MediaCache>>) -> Result<(), CoreError> {
    let Some(cache) = cache else {
        return Ok(());
    };
    blocking(move || {
        cache.clear().map_err(|error| CoreError::Storage {
            message: error.to_string(),
        })
    })
    .await
}
