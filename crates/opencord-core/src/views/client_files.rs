//! What a view's renderer reads from disk (plan M2 Task 15, design §13.1):
//! the plugin's assets by their hash, and images of the other sources as
//! files.
//!
//! - **Assets** live in `<data>/cache/plugins/<sha256>`, beside the client
//!   files. The hash is the caller's, else the one the server listed for
//!   the plugin's views, else found by fetching the file once.
//! - **Images** from `file`, `file_token`, `url` and `bytes` live in this
//!   run's folder under `<data>/cache/plugin-images/`, each named by its
//!   source and version, so a raised version is a new path (the renderer
//!   caches by path). Earlier runs' folders are removed at start; at most
//!   [`IMAGE_FILES_KEPT`] stay, the oldest leaving first.
//! - **Attachments** are refused: the renderer reads them with
//!   `attachment_bytes`, through the encrypted media cache, so they never
//!   reach the disk in the clear.
//!
//! Maps and when their entries leave:
//! - `State::assets` (server and plugin → path → hash and size): a plugin's
//!   entries are replaced by each `ViewOpened` and `ViewScriptsUpdate`, and
//!   a fetch without a hash adds one; a server's entries leave on a new
//!   session, on a `PluginChannelTypesUpdate` and on `close_server`.
//! - `Shared::verified` (hashes checked this run): one per file, bounded by
//!   what the plugins ship; gone with the app.
//! - `Shared::images` (this run's image files, oldest first): at most
//!   [`IMAGE_FILES_KEPT`].

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use data_encoding::BASE64;
use opencord_plugins::limits::{
    IMAGE_FETCH_MAX_BYTES, IMAGE_MAX_RAW_BYTES, PACKAGE_MAX_FILE_BYTES,
};
use opencord_proto::v1 as proto;
use prost::Message as _;
use sha2::{Digest, Sha256};

use super::{Shared, Views, check_id, check_path, hash_of, lock, write_aside};
use crate::api::types::CoreError;
use crate::convert;
use crate::http::HttpResponse;

/// Image files this run keeps on disk.
pub(super) const IMAGE_FILES_KEPT: usize = 500;
/// A plugin's stored file shown as an image.
const FILE_IMAGE_MAX_BYTES: u64 = PACKAGE_MAX_FILE_BYTES;
/// An image's `url`, in bytes.
const URL_MAX_BYTES: usize = 2048;
/// A `files.token`, in characters.
const TOKEN_MAX_CHARS: usize = 64;
/// The keys an image's source is one of.
const SOURCES: [&str; 6] = ["asset", "file", "file_token", "url", "attachment", "bytes"];

/// A file of the plugin's package, by its hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Known {
    pub sha256: String,
    pub size: u64,
}

/// An image's source, checked.
enum Source {
    Asset(String),
    File(String),
    FileToken(String),
    Url(String),
    Bytes(Vec<u8>),
}

impl Views {
    /// The cached file of the plugin's asset at `path` (under `assets/`).
    /// An empty `sha256` takes the hash the server listed for the plugin's
    /// views, or fetches the file to learn it.
    pub(crate) async fn asset_file(
        &self,
        key: &str,
        plugin_id: &str,
        path: &str,
        sha256: &str,
    ) -> Result<String, CoreError> {
        check_id(plugin_id).map_err(invalid)?;
        check_asset(path)?;
        let known = if sha256.is_empty() {
            self.shared
                .lock()
                .assets
                .get(&(key.to_owned(), plugin_id.to_owned()))
                .and_then(|files| files.get(path))
                .cloned()
        } else {
            Some(Known {
                sha256: sha256.to_owned(),
                size: PACKAGE_MAX_FILE_BYTES,
            })
        };
        let cached = match known {
            Some(known) => {
                self.shared
                    .asset_by_hash(key, plugin_id, path, known)
                    .await?
            }
            None => self.shared.asset_fetched(key, plugin_id, path).await?,
        };
        Ok(shown(&cached))
    }

    /// A file holding the image `source_json` names (the `image` block's
    /// source and `version`), fetched through the plugin's routes.
    pub(crate) async fn image_file(
        &self,
        key: &str,
        plugin_id: &str,
        source_json: &str,
    ) -> Result<String, CoreError> {
        check_id(plugin_id).map_err(invalid)?;
        let (source, version) = source_of(source_json)?;
        let named = |kind: &str, value: &str| {
            let id = format!("{kind}\n{key}\n{plugin_id}\n{value}\n{version}");
            hex::encode(Sha256::digest(id.as_bytes()))
        };
        let shared = &self.shared;
        match source {
            Source::Asset(path) => self.asset_file(key, plugin_id, &path, "").await,
            Source::File(path) => {
                let route = format!("/plugins/{plugin_id}/files/{path}");
                let name = named("file", &path);
                shared.image(key, name, route, FILE_IMAGE_MAX_BYTES).await
            }
            Source::FileToken(token) => {
                let route = format!("/plugins/{plugin_id}/files/t/{token}");
                let name = named("file_token", &token);
                shared.image(key, name, route, FILE_IMAGE_MAX_BYTES).await
            }
            Source::Url(url) => {
                let route = format!("/plugins/{plugin_id}/images?url={}", encoded(&url));
                let name = named("url", &url);
                shared.image(key, name, route, IMAGE_FETCH_MAX_BYTES).await
            }
            Source::Bytes(bytes) => {
                let name = hex::encode(Sha256::digest(&bytes));
                shared.keep_image(name, &bytes).await
            }
        }
    }

    /// What's known of the assets of `key`'s plugins may be out of date:
    /// their files are asked for again by path.
    pub(crate) fn forget_assets(&self, key: &str) {
        self.shared
            .lock()
            .assets
            .retain(|(server, _), _| server != key);
    }
}

impl Shared {
    /// The files the server listed for a plugin's views on `key`: their
    /// assets' hashes, in place of what was known before.
    pub(super) fn remember_assets(
        &self,
        key: &str,
        plugin_id: &str,
        files: &[proto::PluginFileRef],
    ) {
        let assets = files
            .iter()
            .filter(|file| file.path.starts_with("assets/"))
            .map(|file| {
                let known = Known {
                    sha256: file.sha256.clone(),
                    size: file.size_bytes,
                };
                (file.path.clone(), known)
            })
            .collect();
        self.lock()
            .assets
            .insert((key.to_owned(), plugin_id.to_owned()), assets);
    }

    /// An asset by its hash: the cached copy when it was checked this run,
    /// else the cached or fetched copy, checked now.
    async fn asset_by_hash(
        &self,
        key: &str,
        plugin_id: &str,
        path: &str,
        known: Known,
    ) -> Result<PathBuf, CoreError> {
        let cached = self.cache_dir.join(&known.sha256);
        if lock(&self.verified).contains(&known.sha256) && exists(&cached).await {
            return Ok(cached);
        }
        let file = proto::PluginFileRef {
            path: path.to_owned(),
            sha256: known.sha256.clone(),
            size_bytes: known.size,
        };
        let bytes = self.file(key, plugin_id, &file).await.map_err(invalid)?;
        if !exists(&cached).await {
            write_aside(&cached, &bytes).await.map_err(storage)?;
        }
        lock(&self.verified).insert(known.sha256);
        Ok(cached)
    }

    /// An asset nobody gave the hash of: fetched, then known by its hash.
    async fn asset_fetched(
        &self,
        key: &str,
        plugin_id: &str,
        path: &str,
    ) -> Result<PathBuf, CoreError> {
        let route = format!("/plugins/{plugin_id}/assets/{path}");
        let body = self.fetch(key, route, PACKAGE_MAX_FILE_BYTES).await?;
        let size = body.len() as u64;
        let sha256 = hash_of(body.clone()).await;
        let cached = self.cache_dir.join(&sha256);
        if !exists(&cached).await {
            write_aside(&cached, &body).await.map_err(storage)?;
        }
        lock(&self.verified).insert(sha256.clone());
        self.lock()
            .assets
            .entry((key.to_owned(), plugin_id.to_owned()))
            .or_default()
            .insert(path.to_owned(), Known { sha256, size });
        Ok(cached)
    }

    /// The image file `name` of this run, fetched from `route` the first
    /// time it's asked for.
    async fn image(
        &self,
        key: &str,
        name: String,
        route: String,
        limit: u64,
    ) -> Result<String, CoreError> {
        let target = self.images_dir.join(&name);
        if exists(&target).await {
            return Ok(shown(&target));
        }
        let body = self.fetch(key, route, limit).await?;
        self.keep_image(name, &body).await
    }

    /// Writes this run's image file `name`, letting the oldest go past
    /// [`IMAGE_FILES_KEPT`].
    async fn keep_image(&self, name: String, bytes: &[u8]) -> Result<String, CoreError> {
        let target = self.images_dir.join(&name);
        if !exists(&target).await {
            write_aside(&target, bytes).await.map_err(storage)?;
        }
        let gone = {
            let mut images = lock(&self.images);
            if !images.contains(&name) {
                images.push_back(name);
            }
            let over = images.len().saturating_sub(IMAGE_FILES_KEPT);
            images.drain(..over).collect::<Vec<_>>()
        };
        for name in gone {
            if let Err(error) = tokio::fs::remove_file(self.images_dir.join(name)).await {
                tracing::debug!(kind = ?error.kind(), "an old plugin image couldn't be removed");
            }
        }
        Ok(shown(&target))
    }

    /// `GET route`'s body, at most `limit` bytes; a refusal carries the
    /// server's reason.
    async fn fetch(&self, key: &str, route: String, limit: u64) -> Result<Vec<u8>, CoreError> {
        let read = usize::try_from(limit)
            .unwrap_or(usize::MAX)
            .saturating_add(1);
        let response = self.transport.get(key, route, read).await?;
        if response.status != 200 {
            return Err(refusal(&response));
        }
        if response.body.len() as u64 > limit {
            return Err(invalid(format!(
                "the file is over {} MiB",
                limit / (1024 * 1024)
            )));
        }
        Ok(response.body)
    }
}

/// Where this run keeps its images; earlier runs' folders go, off the
/// caller's thread.
pub(super) fn images_dir(runtime: &tokio::runtime::Handle, data_dir: &Path) -> PathBuf {
    let root = data_dir.join("cache").join("plugin-images");
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis());
    let run = format!("{started}-{}", std::process::id());
    let dir = root.join(&run);
    runtime.spawn_blocking(move || {
        let Ok(entries) = std::fs::read_dir(&root) else {
            return;
        };
        for entry in entries.flatten() {
            if entry.file_name() != run.as_str()
                && let Err(error) = std::fs::remove_dir_all(entry.path())
            {
                tracing::warn!(kind = ?error.kind(), "an earlier run's plugin images couldn't be removed");
            }
        }
    });
    dir
}

/// The image's source and version, from the renderer's JSON.
fn source_of(json: &str) -> Result<(Source, u64), CoreError> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|error| invalid(format!("not JSON: {error}")))?;
    let serde_json::Value::Object(fields) = value else {
        return Err(invalid("an image's source is an object"));
    };
    let mut given = SOURCES.iter().filter(|name| fields.contains_key(**name));
    let (Some(&kind), None) = (given.next(), given.next()) else {
        return Err(invalid(
            "an image has one source: asset, file, file_token, url, attachment or bytes",
        ));
    };
    let version = match fields.get("version") {
        None | Some(serde_json::Value::Null) => 0,
        Some(version) => version
            .as_u64()
            .ok_or_else(|| invalid("an image's version is a whole number from 0"))?,
    };
    let text = || {
        fields[kind]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| invalid(format!("an image's {kind} is a string")))
    };
    let source = match kind {
        "asset" => {
            let path = text()?;
            check_asset(&path)?;
            Source::Asset(path)
        }
        "file" => {
            let path = text()?;
            check_path(&path).map_err(invalid)?;
            Source::File(path)
        }
        "file_token" => {
            let token = text()?;
            let fine = (1..=TOKEN_MAX_CHARS).contains(&token.len())
                && token
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
            if !fine {
                return Err(invalid("that isn't a file token"));
            }
            Source::FileToken(token)
        }
        "url" => {
            let url = text()?;
            if !url.starts_with("https://") || url.len() > URL_MAX_BYTES {
                return Err(invalid(format!(
                    "an image's url is https, at most {URL_MAX_BYTES} bytes"
                )));
            }
            Source::Url(url)
        }
        "bytes" => {
            let bytes = BASE64
                .decode(text()?.as_bytes())
                .map_err(|_| invalid("an image's bytes aren't base64"))?;
            if bytes.len() > IMAGE_MAX_RAW_BYTES {
                return Err(invalid(format!(
                    "an image's bytes are over {} KiB",
                    IMAGE_MAX_RAW_BYTES / 1024
                )));
            }
            Source::Bytes(bytes)
        }
        // "attachment"
        _ => {
            return Err(invalid(
                "an image's attachment is read with attachment_bytes, not as a file",
            ));
        }
    };
    Ok((source, version))
}

/// Only files under `assets/` are a plugin's assets.
fn check_asset(path: &str) -> Result<(), CoreError> {
    check_path(path).map_err(invalid)?;
    if !path.starts_with("assets/") {
        return Err(invalid(format!("{path:?} isn't under assets/")));
    }
    Ok(())
}

/// `text` as a URL query value: every byte but the unreserved ones as `%XX`.
fn encoded(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// The server's reason for refusing, from the error it answered with.
fn refusal(response: &HttpResponse) -> CoreError {
    let answered = proto::Response::decode(response.body.as_slice())
        .ok()
        .and_then(|answer| answer.result);
    match answered {
        Some(proto::response::Result::Error(error)) => convert::error(error),
        _ => CoreError::Connection {
            message: format!("the server answered HTTP {}", response.status),
        },
    }
}

async fn exists(path: &Path) -> bool {
    tokio::fs::try_exists(path).await.unwrap_or(false)
}

/// A path for the renderer.
fn shown(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn invalid(message: impl Into<String>) -> CoreError {
    CoreError::InvalidInput {
        message: message.into(),
    }
}

fn storage(error: std::io::Error) -> CoreError {
    CoreError::Storage {
        message: error.to_string(),
    }
}
