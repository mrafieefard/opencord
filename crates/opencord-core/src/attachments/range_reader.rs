//! HTTP range reader for streaming attachments (media uploads plan, Task 34).
//! Fetches 1 MiB blocks on demand with an 8-block LRU cache and 2-block read-ahead.

use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex, PoisonError};

use bytes::Bytes;
use reqwest::StatusCode;
use tokio::runtime::Handle;

use super::transfer::{Call, network, refusal};
use crate::api::types::CoreError;
use crate::client::Client;

const BLOCK_SIZE: u64 = 1024 * 1024; // 1 MiB
const LRU_CAPACITY: usize = 8;
const READ_AHEAD_BLOCKS: usize = 2;

struct LruCache {
    blocks: Vec<(usize, Bytes)>, // most recently used at back
    in_flight: HashSet<usize>,
}

impl LruCache {
    fn new() -> Self {
        Self {
            blocks: Vec::with_capacity(LRU_CAPACITY),
            in_flight: HashSet::new(),
        }
    }

    fn get(&mut self, block_idx: usize) -> Option<Bytes> {
        if let Some(pos) = self.blocks.iter().position(|(idx, _)| *idx == block_idx) {
            let entry = self.blocks.remove(pos);
            self.blocks.push(entry);
            Some(self.blocks.last().unwrap().1.clone())
        } else {
            None
        }
    }

    fn insert(&mut self, block_idx: usize, data: Bytes) {
        if let Some(pos) = self.blocks.iter().position(|(idx, _)| *idx == block_idx) {
            self.blocks.remove(pos);
        } else if self.blocks.len() >= LRU_CAPACITY {
            self.blocks.remove(0); // evict oldest
        }
        self.blocks.push((block_idx, data));
    }
}

pub struct RangeReader {
    client: Client,
    server_key: String,
    attachment_id: i64,
    handle: Handle,
    total_size: u64,
    position: u64,
    cache: Arc<Mutex<LruCache>>,
}

impl RangeReader {
    pub async fn open(
        client: Client,
        server_key: String,
        attachment_id: i64,
    ) -> Result<Self, CoreError> {
        let handle = Handle::current();
        let path = format!("/media/attachments/{attachment_id}");
        let mut call = Call::get(path, None);
        call.headers.push(("range", "bytes=0-0".to_owned()));

        let response = client.transfers().send(&client, &server_key, call).await?;
        let status = response.status();
        if !status.is_success() && status != StatusCode::PARTIAL_CONTENT {
            return Err(refusal(response).await);
        }

        let total_size = if let Some(content_range) = response.headers().get("content-range") {
            let s = content_range
                .to_str()
                .map_err(|e| CoreError::InvalidInput {
                    message: format!("invalid Content-Range: {e}"),
                })?;
            let total_str = s
                .rsplit('/')
                .next()
                .ok_or_else(|| CoreError::InvalidInput {
                    message: format!("malformed Content-Range: {s}"),
                })?;
            total_str
                .trim()
                .parse::<u64>()
                .map_err(|e| CoreError::InvalidInput {
                    message: format!("invalid size in Content-Range {s}: {e}"),
                })?
        } else if let Some(content_length) = response.headers().get("content-length") {
            content_length
                .to_str()
                .unwrap_or("0")
                .parse::<u64>()
                .unwrap_or(0)
        } else {
            0
        };

        let mut cache = LruCache::new();
        if status == StatusCode::OK {
            let body = response.bytes().await.map_err(|e| network(&e))?;
            cache.insert(0, body);
        }

        Ok(Self {
            client,
            server_key,
            attachment_id,
            handle,
            total_size,
            position: 0,
            cache: Arc::new(Mutex::new(cache)),
        })
    }

    pub fn total_size(&self) -> u64 {
        self.total_size
    }

    fn get_or_fetch_block(&mut self, block_idx: usize) -> std::io::Result<Bytes> {
        // 1. Check cache
        {
            let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(data) = cache.get(block_idx) {
                return Ok(data);
            }
        }

        // 2. Fetch block synchronously on runtime handle
        let client = self.client.clone();
        let server_key = self.server_key.clone();
        let attachment_id = self.attachment_id;
        let total_size = self.total_size;

        let block = if tokio::runtime::Handle::try_current().is_ok() {
            tokio::task::block_in_place(|| {
                self.handle.block_on(async move {
                    fetch_block_remote(&client, &server_key, attachment_id, block_idx, total_size)
                        .await
                })
            })
        } else {
            self.handle.block_on(async move {
                fetch_block_remote(&client, &server_key, attachment_id, block_idx, total_size).await
            })
        }
        .map_err(|e| std::io::Error::other(e.to_string()))?;

        // 3. Insert into cache
        {
            let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
            cache.insert(block_idx, block.clone());
        }

        // 4. Trigger read-ahead for next 2 blocks in the background
        self.trigger_read_ahead(block_idx);

        Ok(block)
    }

    fn trigger_read_ahead(&self, current_block: usize) {
        if self.total_size == 0 {
            return;
        }
        let max_block = (self.total_size.saturating_sub(1) / BLOCK_SIZE) as usize;
        for ahead_idx in (current_block + 1)..=(current_block + READ_AHEAD_BLOCKS).min(max_block) {
            let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
            if cache.get(ahead_idx).is_none() && cache.in_flight.insert(ahead_idx) {
                let cache_clone = self.cache.clone();
                let client = self.client.clone();
                let server_key = self.server_key.clone();
                let attachment_id = self.attachment_id;
                let total_size = self.total_size;

                self.handle.spawn(async move {
                    if let Ok(data) = fetch_block_remote(
                        &client,
                        &server_key,
                        attachment_id,
                        ahead_idx,
                        total_size,
                    )
                    .await
                    {
                        let mut c = cache_clone.lock().unwrap_or_else(PoisonError::into_inner);
                        c.in_flight.remove(&ahead_idx);
                        c.insert(ahead_idx, data);
                    } else {
                        let mut c = cache_clone.lock().unwrap_or_else(PoisonError::into_inner);
                        c.in_flight.remove(&ahead_idx);
                    }
                });
            }
        }
    }
}

async fn fetch_block_remote(
    client: &Client,
    server_key: &str,
    attachment_id: i64,
    block_idx: usize,
    total_size: u64,
) -> Result<Bytes, CoreError> {
    let start = block_idx as u64 * BLOCK_SIZE;
    if start >= total_size {
        return Ok(Bytes::new());
    }
    let end = ((block_idx as u64 + 1) * BLOCK_SIZE - 1).min(total_size.saturating_sub(1));
    let path = format!("/media/attachments/{attachment_id}");
    let mut call = Call::get(path, None);
    call.headers.push(("range", format!("bytes={start}-{end}")));

    let response = client.transfers().send(client, server_key, call).await?;
    let status = response.status();
    if !status.is_success() && status != StatusCode::PARTIAL_CONTENT {
        return Err(refusal(response).await);
    }
    let bytes = response.bytes().await.map_err(|e| network(&e))?;
    Ok(bytes)
}

impl Read for RangeReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.position >= self.total_size || buf.is_empty() {
            return Ok(0);
        }

        let block_idx = (self.position / BLOCK_SIZE) as usize;
        let offset = (self.position % BLOCK_SIZE) as usize;

        let block_data = self.get_or_fetch_block(block_idx)?;
        if offset >= block_data.len() {
            return Ok(0);
        }

        let available = &block_data[offset..];
        let to_copy = buf.len().min(available.len());
        buf[..to_copy].copy_from_slice(&available[..to_copy]);
        self.position += to_copy as u64;
        Ok(to_copy)
    }
}

impl Seek for RangeReader {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let new_pos = match pos {
            SeekFrom::Start(offset) => offset as i64,
            SeekFrom::Current(offset) => self.position as i64 + offset,
            SeekFrom::End(offset) => self.total_size as i64 + offset,
        };
        if new_pos < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "cannot seek before byte 0",
            ));
        }
        self.position = new_pos as u64;
        Ok(self.position)
    }
}
