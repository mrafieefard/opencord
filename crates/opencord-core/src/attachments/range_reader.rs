//! Reads an attachment over HTTP ranges, for the media player (Milestone A7,
//! Task 34): 1 MiB blocks fetched on demand, the 8 used last kept, and the
//! next 2 fetched ahead in the background.

use std::collections::{HashSet, VecDeque};
use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};

use bytes::Bytes;
use reqwest::StatusCode;
use tokio::runtime::Handle;

use super::transfer::{Call, network, refusal};
use crate::api::types::CoreError;
use crate::client::Client;

const BLOCK_SIZE: u64 = 1024 * 1024;
const KEPT_BLOCKS: usize = 8;
const READ_AHEAD_BLOCKS: usize = 2;

#[derive(Default)]
struct Blocks {
    /// Most recently used last.
    kept: VecDeque<(usize, Bytes)>,
    fetching: HashSet<usize>,
}

impl Blocks {
    fn get(&mut self, index: usize) -> Option<Bytes> {
        let at = self.kept.iter().position(|(i, _)| *i == index)?;
        let entry = self.kept.remove(at)?;
        let bytes = entry.1.clone();
        self.kept.push_back(entry);
        Some(bytes)
    }

    fn keep(&mut self, index: usize, bytes: Bytes) {
        self.kept.retain(|(i, _)| *i != index);
        if self.kept.len() >= KEPT_BLOCKS {
            self.kept.pop_front();
        }
        self.kept.push_back((index, bytes));
    }
}

/// The blocks, and the signal that a fetch finished.
#[derive(Default)]
struct Shared {
    blocks: Mutex<Blocks>,
    fetched: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Blocks> {
        self.blocks.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn finish(&self, index: usize, bytes: Option<Bytes>) {
        let mut blocks = self.lock();
        blocks.fetching.remove(&index);
        if let Some(bytes) = bytes {
            blocks.keep(index, bytes);
        }
        self.fetched.notify_all();
    }
}

/// Where to fetch from.
#[derive(Clone)]
struct Origin {
    client: Client,
    server_key: String,
    attachment_id: i64,
    total_size: u64,
}

impl Origin {
    async fn fetch(&self, index: usize) -> Result<Bytes, CoreError> {
        let start = index as u64 * BLOCK_SIZE;
        let end = (start + BLOCK_SIZE).min(self.total_size) - 1;
        let path = format!("/media/attachments/{}", self.attachment_id);
        let mut call = Call::get(path, None);
        call.headers.push(("range", format!("bytes={start}-{end}")));
        let response = self
            .client
            .transfers()
            .send(&self.client, &self.server_key, call)
            .await?;
        if response.status() != StatusCode::PARTIAL_CONTENT {
            return Err(refusal(response).await);
        }
        let bytes = response.bytes().await.map_err(|e| network(&e))?;
        if bytes.len() as u64 != end - start + 1 {
            return Err(CoreError::Connection {
                message: format!("block {index} came back with {} bytes", bytes.len()),
            });
        }
        Ok(bytes)
    }
}

pub struct RangeReader {
    origin: Origin,
    handle: Handle,
    position: u64,
    shared: Arc<Shared>,
    /// Told `true` while a read waits for the network, then `false`.
    on_wait: Option<Box<dyn Fn(bool) + Send>>,
}

impl RangeReader {
    pub async fn open(
        client: Client,
        server_key: String,
        attachment_id: i64,
    ) -> Result<Self, CoreError> {
        let path = format!("/media/attachments/{attachment_id}");
        let mut call = Call::get(path, None);
        call.headers.push(("range", "bytes=0-0".to_owned()));
        let response = client.transfers().send(&client, &server_key, call).await?;
        // `bytes 0-0/<size>`, or `bytes */0` for an empty file.
        let total_size = match response.status() {
            StatusCode::PARTIAL_CONTENT | StatusCode::RANGE_NOT_SATISFIABLE => response
                .headers()
                .get("content-range")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.rsplit('/').next())
                .and_then(|total| total.trim().parse::<u64>().ok())
                .ok_or_else(|| CoreError::Connection {
                    message: "the server sent no usable Content-Range".to_owned(),
                })?,
            _ => return Err(refusal(response).await),
        };
        Ok(Self {
            origin: Origin {
                client,
                server_key,
                attachment_id,
                total_size,
            },
            handle: Handle::current(),
            position: 0,
            shared: Arc::default(),
            on_wait: None,
        })
    }

    pub fn total_size(&self) -> u64 {
        self.origin.total_size
    }

    /// `on_wait(true)` while a read waits for the network, `false` after.
    pub fn on_wait(&mut self, on_wait: impl Fn(bool) + Send + 'static) {
        self.on_wait = Some(Box::new(on_wait));
    }

    fn block(&mut self, index: usize) -> std::io::Result<Bytes> {
        let mut waited = false;
        let result = loop {
            let mut blocks = self.shared.lock();
            if let Some(bytes) = blocks.get(index) {
                break Ok(bytes);
            }
            if !waited && let Some(on_wait) = &self.on_wait {
                on_wait(true);
            }
            waited = true;
            if blocks.fetching.contains(&index) {
                // Fetched ahead already: wait for it rather than fetch twice.
                drop(
                    self.shared
                        .fetched
                        .wait_while(blocks, |b| b.fetching.contains(&index))
                        .unwrap_or_else(PoisonError::into_inner),
                );
                continue;
            }
            blocks.fetching.insert(index);
            drop(blocks);
            let origin = self.origin.clone();
            let fetch = async move { origin.fetch(index).await };
            let fetched = if Handle::try_current().is_ok() {
                tokio::task::block_in_place(|| self.handle.block_on(fetch))
            } else {
                self.handle.block_on(fetch)
            };
            self.shared.finish(index, fetched.as_ref().ok().cloned());
            break fetched.map_err(|e| std::io::Error::other(e.to_string()));
        };
        if waited && let Some(on_wait) = &self.on_wait {
            on_wait(false);
        }
        self.read_ahead(index);
        result
    }

    fn read_ahead(&self, index: usize) {
        let last = (self.origin.total_size.saturating_sub(1) / BLOCK_SIZE) as usize;
        let mut blocks = self.shared.lock();
        for ahead in (index + 1)..=(index + READ_AHEAD_BLOCKS).min(last) {
            if blocks.kept.iter().any(|(i, _)| *i == ahead) || !blocks.fetching.insert(ahead) {
                continue;
            }
            let origin = self.origin.clone();
            let shared = self.shared.clone();
            self.handle.spawn(async move {
                // A failure here is retried by the read that needs the block.
                shared.finish(ahead, origin.fetch(ahead).await.ok());
            });
        }
    }
}

impl Read for RangeReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.position >= self.origin.total_size || buf.is_empty() {
            return Ok(0);
        }
        let index = (self.position / BLOCK_SIZE) as usize;
        let offset = (self.position % BLOCK_SIZE) as usize;
        let block = self.block(index)?;
        let available = &block[offset.min(block.len())..];
        let count = buf.len().min(available.len());
        buf[..count].copy_from_slice(&available[..count]);
        self.position += count as u64;
        Ok(count)
    }
}

impl Seek for RangeReader {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let target = match pos {
            SeekFrom::Start(offset) => Some(offset),
            SeekFrom::Current(offset) => self.position.checked_add_signed(offset),
            SeekFrom::End(offset) => self.origin.total_size.checked_add_signed(offset),
        };
        self.position = target.ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "seek before byte 0")
        })?;
        Ok(self.position)
    }
}
