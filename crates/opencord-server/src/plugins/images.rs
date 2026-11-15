// SPDX-License-Identifier: MPL-2.0

//! Images from the websites a plugin's manifest names, fetched by the
//! server for members (design 2026-11-07 §13, plan M2 Task 11): apps never
//! reach the internet themselves.
//!
//! `GET /plugins/{id}/images?url=<https URL>` with a member's media token.
//! The plugin holds `internet` and the URL's host is one of its manifest's
//! `domains`. The server fetches it with a client [`net`] builds from those
//! domains, so every address and redirect check of `http.request` applies.
//! Only PNG, JPEG, WebP and GIF, known by their first bytes, at most
//! [`IMAGE_FETCH_MAX_BYTES`] and [`IMAGE_FETCH_MAX_PIXELS`] (read from the
//! header; the pixels are never decoded here).
//!
//! Fetched images wait in `data/plugins/<id>/image-cache/<sha256 of url>`
//! for [`IMAGE_CACHE_FRESH`], at most [`IMAGE_CACHE_MAX_BYTES`] and
//! [`IMAGE_CACHE_MAX_ENTRIES`] per plugin, oldest first out. Requests for
//! a URL being fetched wait for that fetch.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};
use std::time::Duration;

use axum::extract::{Path as UrlPath, Query, State};
use axum::http::header::CACHE_CONTROL;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use futures_util::FutureExt as _;
use futures_util::future::{BoxFuture, Shared};
use image::{ImageFormat, ImageReader};
use opencord_common::sniff::{Sniffed, sniff};
use opencord_plugins::limits::{
    HTTP_CONCURRENT, HTTP_TIMEOUT, IMAGE_CACHE_FRESH, IMAGE_CACHE_MAX_BYTES,
    IMAGE_CACHE_MAX_ENTRIES, IMAGE_FETCH_MAX_BYTES, IMAGE_FETCH_MAX_PIXELS, VIEW_MAX_PROP_CHARS,
};
use opencord_proto::v1 as proto;
use proto::response::Result as Reply;
use reqwest::Url;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt as _;
use tokio::sync::Semaphore;

use super::PluginHandle;
use super::assets::serve_open;
use super::net::{self, NetError, NetGuard, SystemResolver};
use super::permissions::PluginPermission;
use crate::error::ApiError;
use crate::media::{authorize, reply, reply_error};
use crate::random;
use crate::state::{AppState, now_ms};

/// The folder in `data/plugins/<id>/` fetched images wait in.
pub const CACHE_FOLDER: &str = "image-cache";
/// Enough of a file to tell the four image types apart (WebP needs 12).
const SNIFF_BYTES: usize = 12;
const MB: u64 = 1024 * 1024;

/// What a member is answered when no image is served.
#[derive(Debug, Clone)]
struct Refusal {
    status: StatusCode,
    error: ApiError,
}

impl Refusal {
    fn new(status: StatusCode, error: ApiError) -> Refusal {
        Refusal { status, error }
    }

    fn invalid(message: impl Into<String>) -> Refusal {
        Refusal::new(StatusCode::BAD_REQUEST, ApiError::invalid_argument(message))
    }

    fn forbidden(message: impl Into<String>) -> Refusal {
        Refusal::new(StatusCode::FORBIDDEN, ApiError::forbidden(message))
    }

    fn internal() -> Refusal {
        Refusal::new(StatusCode::INTERNAL_SERVER_ERROR, ApiError::internal())
    }

    fn into_response(self) -> Response {
        reply(self.status, Reply::Error(self.error.to_proto()))
    }
}

/// Why a fetch kept nothing: what the member is answered, and what the
/// plugin's log says.
struct Failed {
    refusal: Refusal,
    reason: String,
}

impl Failed {
    /// The website's answer isn't one the server shows.
    fn refused(code: proto::ErrorCode, message: impl Into<String>) -> Failed {
        let message = message.into();
        Failed {
            refusal: Refusal::new(
                StatusCode::BAD_GATEWAY,
                ApiError::new(code, message.clone()),
            ),
            reason: message,
        }
    }

    fn too_large() -> Failed {
        Failed::refused(
            proto::ErrorCode::FileTooLarge,
            format!("the image is larger than {} MB", IMAGE_FETCH_MAX_BYTES / MB),
        )
    }

    /// The website couldn't be reached, or answered with an error. The
    /// member isn't told the details: they can name addresses.
    fn network(reason: String) -> Failed {
        Failed {
            refusal: Refusal::new(
                StatusCode::BAD_GATEWAY,
                ApiError::new(proto::ErrorCode::Internal, "the image couldn't be fetched"),
            ),
            reason,
        }
    }

    /// The cache's folder failed the server.
    fn disk(plugin_id: &str, what: &str, error: &io::Error) -> Failed {
        tracing::warn!(plugin = %plugin_id, kind = ?error.kind(), "plugin image cache: {what} failed");
        Failed {
            refusal: Refusal::internal(),
            reason: format!("the server couldn't keep it ({what} failed)"),
        }
    }
}

/// One fetched image, as the cache keeps it.
struct Entry {
    size: u64,
    content_type: &'static str,
    fetched_at_ms: i64,
}

/// A fresh image to serve.
#[derive(Debug, Clone)]
struct Hit {
    key: String,
    path: PathBuf,
    content_type: &'static str,
    fetched_at_ms: i64,
}

/// A fetch other requests for its URL wait on.
type Fetching = Shared<BoxFuture<'static, Result<Hit, Refusal>>>;

#[derive(Default)]
struct Cache {
    /// By the SHA-256 of the URL, in hex: the file's name.
    entries: HashMap<String, Entry>,
    bytes: u64,
    /// Fetches running, by the same key. An entry leaves as its fetch ends,
    /// however it ends ([`Unlist`]).
    fetching: HashMap<String, Fetching>,
    /// Set as the plugin is uninstalled: fetches still running keep nothing.
    forgotten: bool,
}

impl Cache {
    fn fresh(&self, folder: &Path, key: &str, now_ms: i64) -> Option<Hit> {
        let entry = self.entries.get(key)?;
        (now_ms < entry.fetched_at_ms.saturating_add(fresh_ms())).then(|| Hit {
            key: key.to_owned(),
            path: folder.join(key),
            content_type: entry.content_type,
            fetched_at_ms: entry.fetched_at_ms,
        })
    }

    fn remove(&mut self, key: &str) -> Option<Entry> {
        let entry = self.entries.remove(key)?;
        self.bytes = self.bytes.saturating_sub(entry.size);
        Some(entry)
    }
}

fn fresh_ms() -> i64 {
    i64::try_from(IMAGE_CACHE_FRESH.as_millis()).unwrap_or(i64::MAX)
}

/// The time fetched images are dated by; tests move it.
#[derive(Clone, Default)]
struct Clock(Arc<AtomicI64>);

impl Clock {
    fn now_ms(&self) -> i64 {
        now_ms().saturating_add(self.0.load(Ordering::Relaxed))
    }
}

/// One plugin's cache and fetches.
struct PluginImages {
    plugin_id: String,
    /// `data/plugins/<id>/image-cache`.
    folder: PathBuf,
    cache: Mutex<Cache>,
    /// The client, and the domains it was built for: a new version of the
    /// plugin brings new ones.
    client: Mutex<Option<(Arc<NetGuard>, reqwest::Client)>>,
    /// Fetches running at once.
    slots: Semaphore,
    /// Whether this boot's first fetch emptied the folder: what an earlier
    /// run left there isn't in [`Cache::entries`].
    emptied: tokio::sync::Mutex<bool>,
    clock: Clock,
}

impl PluginImages {
    fn cache(&self) -> MutexGuard<'_, Cache> {
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Empties the folder at this boot's first fetch, and makes it again
    /// whenever it's gone (the plugin uninstalled and installed again).
    async fn prepare(&self) -> Result<(), Failed> {
        let mut emptied = self.emptied.lock().await;
        if !*emptied {
            match tokio::fs::remove_dir_all(&self.folder).await {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(Failed::disk(&self.plugin_id, "emptying the cache", &error));
                }
            }
            *emptied = true;
        }
        // Not `create_dir_all`: an uninstalled plugin's folder stays gone.
        match tokio::fs::create_dir(&self.folder).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            Err(error) => Err(Failed::disk(&self.plugin_id, "making the cache", &error)),
        }
    }

    /// Moves the fetched file at `temp` into the cache as `key`, pushing
    /// out the oldest images until it fits. Touches the disk under the
    /// cache's lock, so a fetch of a key pushed out can't be deleted by it:
    /// run it off the async threads.
    fn keep(&self, key: &str, temp: &Path, entry: Entry) -> Result<Hit, Failed> {
        let mut cache = self.cache();
        if cache.forgotten {
            remove_quietly(&self.plugin_id, temp);
            return Err(Failed::network("the plugin was uninstalled".to_owned()));
        }
        // Replaced by the rename below.
        cache.remove(key);
        while cache.bytes.saturating_add(entry.size) > IMAGE_CACHE_MAX_BYTES
            || cache.entries.len() >= IMAGE_CACHE_MAX_ENTRIES
        {
            let oldest = cache
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.fetched_at_ms)
                .map(|(key, _)| key.clone());
            let Some(oldest) = oldest else { break };
            cache.remove(&oldest);
            remove_quietly(&self.plugin_id, &self.folder.join(&oldest));
        }
        let path = self.folder.join(key);
        if let Err(error) = std::fs::rename(temp, &path) {
            remove_quietly(&self.plugin_id, temp);
            return Err(Failed::disk(&self.plugin_id, "keeping an image", &error));
        }
        let hit = Hit {
            key: key.to_owned(),
            path,
            content_type: entry.content_type,
            fetched_at_ms: entry.fetched_at_ms,
        };
        cache.bytes = cache.bytes.saturating_add(entry.size);
        cache.entries.insert(key.to_owned(), entry);
        Ok(hit)
    }

    /// Forgets `hit` when its file is gone from the disk.
    fn lost(&self, hit: &Hit) {
        let mut cache = self.cache();
        if cache
            .entries
            .get(&hit.key)
            .is_some_and(|entry| entry.fetched_at_ms == hit.fetched_at_ms)
        {
            cache.remove(&hit.key);
        }
    }
}

/// Deletes a cache file; a failure leaves it to the next boot's emptying.
fn remove_quietly(plugin_id: &str, path: &Path) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::warn!(plugin = %plugin_id, kind = ?error.kind(), "could not delete a plugin image file");
        }
    }
}

/// How fetches reach websites.
#[derive(Clone)]
enum Reach {
    /// The system's resolver, public addresses, public certificate
    /// authorities.
    Internet,
    #[cfg(feature = "testing")]
    Test(TestReach),
}

impl Reach {
    fn client(&self, guard: Arc<NetGuard>) -> Result<reqwest::Client, NetError> {
        match self {
            Reach::Internet => net::client(guard, Arc::new(SystemResolver)),
            #[cfg(feature = "testing")]
            Reach::Test(test) => test.client(guard),
        }
    }
}

/// For tests: fetches look names up with `resolver` and trust `ca_der`
/// instead of the public authorities.
#[cfg(feature = "testing")]
#[derive(Clone)]
pub struct TestReach {
    pub resolver: Arc<dyn net::Resolve>,
    /// Whether this machine's addresses may be reached; `false` keeps the
    /// policy the server runs with.
    pub loopback: bool,
    /// A certificate authority, DER.
    pub ca_der: Vec<u8>,
}

#[cfg(feature = "testing")]
impl TestReach {
    fn client(&self, guard: Arc<NetGuard>) -> Result<reqwest::Client, NetError> {
        let tls_error = |error: rustls::Error| NetError::Network(format!("TLS: {error}"));
        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(rustls::pki_types::CertificateDer::from(self.ca_der.clone()))
            .map_err(tls_error)?;
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let tls = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(tls_error)?
            .with_root_certificates(roots)
            .with_no_client_auth();
        let policy = if self.loopback {
            net::AddressPolicy::LoopbackForTests
        } else {
            net::AddressPolicy::Public
        };
        net::build_client_with(guard, self.resolver.clone(), policy, tls)
    }
}

/// Every plugin's fetched images.
///
/// **A plugin's entry** is made by its first image request, and leaves
/// when it's uninstalled ([`Images::forget_plugin`]) or the server
/// restarts; a stop or restart keeps it, since its images don't need its
/// scripts. **Its images** leave once stale and fetched again, when pushed
/// out by newer ones, when it's uninstalled (with their folder), and at the
/// next boot's first fetch.
pub struct Images {
    plugins: Mutex<HashMap<String, Arc<PluginImages>>>,
    reach: RwLock<Reach>,
    clock: Clock,
}

impl Default for Images {
    fn default() -> Self {
        Images {
            plugins: Mutex::default(),
            reach: RwLock::new(Reach::Internet),
            clock: Clock::default(),
        }
    }
}

impl Images {
    /// For tests: images see the time as `by` later from now on.
    pub fn move_clock(&self, by: Duration) {
        let by = i64::try_from(by.as_millis()).unwrap_or(i64::MAX);
        self.clock.0.fetch_add(by, Ordering::Relaxed);
    }

    /// For tests: fetches go through `reach` from now on.
    #[cfg(feature = "testing")]
    pub fn reach_for_tests(&self, reach: TestReach) {
        *self.reach.write().unwrap_or_else(PoisonError::into_inner) = Reach::Test(reach);
        for plugin in self.lock().values() {
            *plugin.client.lock().unwrap_or_else(PoisonError::into_inner) = None;
        }
    }

    /// Forgets the plugin's images, as it's uninstalled; the uninstall
    /// deletes their folder. Fetches still running keep nothing.
    pub fn forget_plugin(&self, plugin_id: &str) {
        let Some(plugin) = self.lock().remove(plugin_id) else {
            return;
        };
        let mut cache = plugin.cache();
        cache.forgotten = true;
        cache.entries.clear();
        cache.bytes = 0;
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, Arc<PluginImages>>> {
        self.plugins.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn plugin(&self, plugins_dir: &Path, plugin_id: &str) -> Arc<PluginImages> {
        self.lock()
            .entry(plugin_id.to_owned())
            .or_insert_with(|| {
                Arc::new(PluginImages {
                    plugin_id: plugin_id.to_owned(),
                    folder: plugins_dir.join(plugin_id).join(CACHE_FOLDER),
                    cache: Mutex::default(),
                    client: Mutex::new(None),
                    slots: Semaphore::new(HTTP_CONCURRENT),
                    emptied: tokio::sync::Mutex::new(false),
                    clock: self.clock.clone(),
                })
            })
            .clone()
    }

    /// The plugin's client for `guard`, built again when the plugin's
    /// domains changed.
    fn client(
        &self,
        plugin: &PluginImages,
        guard: &Arc<NetGuard>,
    ) -> Result<reqwest::Client, NetError> {
        if let Some((built_for, client)) =
            &*plugin.client.lock().unwrap_or_else(PoisonError::into_inner)
            && Arc::ptr_eq(built_for, guard)
        {
            return Ok(client.clone());
        }
        let reach = self
            .reach
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let client = reach.client(guard.clone())?;
        *plugin.client.lock().unwrap_or_else(PoisonError::into_inner) =
            Some((guard.clone(), client.clone()));
        Ok(client)
    }

    /// Serves the image at `url`, from the cache or fetched now.
    async fn serve(
        &self,
        state: &AppState,
        handle: Arc<PluginHandle>,
        user_id: i64,
        url: Url,
    ) -> Response {
        let plugin = self.plugin(&state.config.plugins_dir(), &handle.id);
        let key = hex::encode(Sha256::digest(url.as_str()));
        // A cached file can be pushed out between finding and opening it;
        // then it's fetched again, once.
        for _ in 0..2 {
            let hit = match self
                .find_or_fetch(state, &plugin, &handle, user_id, &url, &key)
                .await
            {
                Ok(hit) => hit,
                Err(refusal) => return refusal.into_response(),
            };
            match tokio::fs::File::open(&hit.path).await {
                Ok(file) => {
                    let left_ms = hit
                        .fetched_at_ms
                        .saturating_add(fresh_ms())
                        .saturating_sub(self.clock.now_ms())
                        .max(0);
                    let cache = format!("private, max-age={}", left_ms / 1000);
                    return serve_open(
                        &handle.id,
                        file,
                        hit.content_type,
                        vec![(CACHE_CONTROL, cache)],
                    )
                    .await;
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => plugin.lost(&hit),
                Err(error) => {
                    return Failed::disk(&handle.id, "opening an image", &error)
                        .refusal
                        .into_response();
                }
            }
        }
        Refusal::internal().into_response()
    }

    /// The fresh cached image for `key`, or the one the running fetch of
    /// it gets, or one fetched now, counted against the member's limit.
    async fn find_or_fetch(
        &self,
        state: &AppState,
        plugin: &Arc<PluginImages>,
        handle: &Arc<PluginHandle>,
        user_id: i64,
        url: &Url,
        key: &str,
    ) -> Result<Hit, Refusal> {
        let client = self.client(plugin, &handle.domains).map_err(|error| {
            tracing::warn!(plugin = %handle.id, %error, "could not set up a plugin's image client");
            Refusal::internal()
        })?;
        let fetching = {
            let mut cache = plugin.cache();
            if let Some(hit) = cache.fresh(&plugin.folder, key, self.clock.now_ms()) {
                return Ok(hit);
            }
            match cache.fetching.get(key) {
                Some(fetching) => fetching.clone(),
                None => {
                    if let Err(wait) = state.rate_limits.check_plugin_image(user_id) {
                        return Err(Refusal::new(
                            StatusCode::TOO_MANY_REQUESTS,
                            ApiError::rate_limited(wait),
                        ));
                    }
                    let job = Job {
                        plugin: plugin.clone(),
                        handle: handle.clone(),
                        client,
                        url: url.clone(),
                        key: key.to_owned(),
                    };
                    // Listed under the lock its end takes to unlist it, so
                    // it can't end before it's listed.
                    let task = tokio::spawn(job.run());
                    let fetching: Fetching = async move {
                        task.await.unwrap_or_else(|error| {
                            tracing::error!(%error, "a plugin image fetch failed");
                            Err(Refusal::internal())
                        })
                    }
                    .boxed()
                    .shared();
                    cache.fetching.insert(key.to_owned(), fetching.clone());
                    fetching
                }
            }
        };
        fetching.await
    }
}

/// Takes a fetch off its plugin's list however it ends.
struct Unlist {
    plugin: Arc<PluginImages>,
    key: String,
}

impl Drop for Unlist {
    fn drop(&mut self) {
        self.plugin.cache().fetching.remove(&self.key);
    }
}

/// One fetch, run as its own task, so it finishes and cleans up whoever
/// still waits for it.
struct Job {
    plugin: Arc<PluginImages>,
    handle: Arc<PluginHandle>,
    client: reqwest::Client,
    url: Url,
    key: String,
}

impl Job {
    async fn run(self) -> Result<Hit, Refusal> {
        let _unlist = Unlist {
            plugin: self.plugin.clone(),
            key: self.key.clone(),
        };
        let result = self.fetch_and_keep().await;
        let outcome = match &result {
            Ok(hit) => hit.content_type.to_owned(),
            Err(failed) => format!("failed: {}", failed.reason),
        };
        self.handle.log().push(
            proto::PluginLogLevel::Action,
            &format!("image {} → {outcome}", net::shown(&self.url)),
        );
        result.map_err(|failed| failed.refusal)
    }

    async fn fetch_and_keep(&self) -> Result<Hit, Failed> {
        let plugin = &self.plugin;
        plugin.prepare().await?;
        let _slot = plugin
            .slots
            .acquire()
            .await
            .map_err(|_| Failed::network("the server is stopping".to_owned()))?;
        let temp = plugin.folder.join(format!(
            "{}.part-{}",
            self.key,
            hex::encode(random::bytes::<8>())
        ));
        let downloaded = tokio::time::timeout(HTTP_TIMEOUT, self.download(&temp))
            .await
            .unwrap_or_else(|_| {
                Err(Failed::network(format!(
                    "no answer within {} s",
                    HTTP_TIMEOUT.as_secs()
                )))
            });
        let checked = match downloaded {
            Ok(format) => check_pixels(&plugin.plugin_id, temp.clone(), format).await,
            Err(failed) => Err(failed),
        };
        let content_type = match checked {
            Ok(content_type) => content_type,
            Err(failed) => {
                let (plugin_id, temp) = (plugin.plugin_id.clone(), temp.clone());
                let removal =
                    tokio::task::spawn_blocking(move || remove_quietly(&plugin_id, &temp));
                if let Err(error) = removal.await {
                    tracing::error!(%error, "deleting a refused plugin image failed");
                }
                return Err(failed);
            }
        };
        let size = tokio::fs::metadata(&temp)
            .await
            .map_err(|error| Failed::disk(&plugin.plugin_id, "measuring an image", &error))?
            .len();
        let entry = Entry {
            size,
            content_type,
            fetched_at_ms: plugin.clock.now_ms(),
        };
        let (keeper, key) = (plugin.clone(), self.key.clone());
        tokio::task::spawn_blocking(move || keeper.keep(&key, &temp, entry))
            .await
            .unwrap_or_else(|error| {
                tracing::error!(%error, "keeping a plugin image failed");
                Err(Failed {
                    refusal: Refusal::internal(),
                    reason: "the server couldn't keep it".to_owned(),
                })
            })
    }

    /// Writes the website's answer to `temp`, refusing it as soon as it's
    /// too large or doesn't start like an image: the image's format.
    async fn download(&self, temp: &Path) -> Result<ImageFormat, Failed> {
        let plugin_id = &self.plugin.plugin_id;
        let mut response = self
            .client
            .get(self.url.clone())
            .send()
            .await
            .map_err(|error| Failed::network(net::failure(error).to_string()))?;
        // Redirects stay within the domains, but may leave https.
        if response.url().scheme() != "https" {
            return Err(Failed::network(
                "it was redirected to an address without https".to_owned(),
            ));
        }
        let status = response.status();
        if !status.is_success() {
            return Err(Failed::network(format!(
                "the website answered {}",
                status.as_u16()
            )));
        }
        if response
            .content_length()
            .is_some_and(|length| length > IMAGE_FETCH_MAX_BYTES)
        {
            return Err(Failed::too_large());
        }
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(temp)
            .await
            .map_err(|error| Failed::disk(plugin_id, "writing an image", &error))?;
        let mut head = Vec::with_capacity(SNIFF_BYTES);
        let mut format = None;
        let mut size: u64 = 0;
        while let Some(piece) = response
            .chunk()
            .await
            .map_err(|error| Failed::network(net::failure(error).to_string()))?
        {
            size = size.saturating_add(piece.len() as u64);
            if size > IMAGE_FETCH_MAX_BYTES {
                return Err(Failed::too_large());
            }
            if format.is_none() {
                let wanted = SNIFF_BYTES - head.len();
                head.extend_from_slice(&piece[..piece.len().min(wanted)]);
                if head.len() == SNIFF_BYTES {
                    format = Some(image_format(&head)?);
                }
            }
            file.write_all(&piece)
                .await
                .map_err(|error| Failed::disk(plugin_id, "writing an image", &error))?;
        }
        let format = match format {
            Some(format) => format,
            None => image_format(&head)?,
        };
        file.flush()
            .await
            .map_err(|error| Failed::disk(plugin_id, "writing an image", &error))?;
        Ok(format)
    }
}

/// The format a file starting with `head` is in: PNG, JPEG, WebP or GIF
/// only. SVG and everything else is refused.
fn image_format(head: &[u8]) -> Result<ImageFormat, Failed> {
    match sniff(head) {
        Sniffed::Png => Ok(ImageFormat::Png),
        Sniffed::Jpeg => Ok(ImageFormat::Jpeg),
        Sniffed::WebP => Ok(ImageFormat::WebP),
        Sniffed::Gif => Ok(ImageFormat::Gif),
        _ => Err(Failed::refused(
            proto::ErrorCode::FileTypeBlocked,
            "only PNG, JPEG, WebP and GIF images can be shown",
        )),
    }
}

/// Reads the image's size from its header, without decoding its pixels,
/// and refuses one over [`IMAGE_FETCH_MAX_PIXELS`]: the type it's served
/// as.
async fn check_pixels(
    plugin_id: &str,
    path: PathBuf,
    format: ImageFormat,
) -> Result<&'static str, Failed> {
    let id = plugin_id.to_owned();
    let read = tokio::task::spawn_blocking(move || -> Result<(u32, u32), Failed> {
        let file = std::fs::File::open(&path)
            .map_err(|error| Failed::disk(&id, "reading an image", &error))?;
        let mut reader = ImageReader::with_format(io::BufReader::new(file), format);
        // Only the header is read; nothing is allocated for the pixels.
        reader.no_limits();
        reader.into_dimensions().map_err(|_| {
            Failed::refused(
                proto::ErrorCode::FileTypeBlocked,
                "the image's header couldn't be read",
            )
        })
    });
    let (width, height) = read.await.unwrap_or_else(|error| {
        tracing::error!(%error, "reading a plugin image's header failed");
        Err(Failed {
            refusal: Refusal::internal(),
            reason: "the server couldn't read it".to_owned(),
        })
    })?;
    if u64::from(width) * u64::from(height) > IMAGE_FETCH_MAX_PIXELS {
        return Err(Failed::refused(
            proto::ErrorCode::FileTooLarge,
            format!(
                "the image is larger than {} megapixels",
                IMAGE_FETCH_MAX_PIXELS / 1_000_000
            ),
        ));
    }
    let content_type = match format {
        ImageFormat::Png => Sniffed::Png,
        ImageFormat::Jpeg => Sniffed::Jpeg,
        ImageFormat::WebP => Sniffed::WebP,
        _ => Sniffed::Gif,
    };
    Ok(content_type.content_type())
}

/// `url`, when a plugin may show it: an https address, at most as long as
/// an image block's `url` may be, on one of `domains`.
fn image_url(raw: Option<&str>, domains: &NetGuard) -> Result<Url, Refusal> {
    let raw = raw
        .filter(|raw| !raw.is_empty())
        .ok_or_else(|| Refusal::invalid("url is needed"))?;
    if raw.chars().count() > VIEW_MAX_PROP_CHARS {
        return Err(Refusal::invalid(format!(
            "url is longer than {VIEW_MAX_PROP_CHARS} characters"
        )));
    }
    let mut url = Url::parse(raw).map_err(|_| Refusal::invalid("url isn't a web address"))?;
    if url.scheme() != "https" {
        return Err(Refusal::invalid("only https images can be shown"));
    }
    domains
        .check(&url)
        .map_err(|error| Refusal::forbidden(error.to_string()))?;
    // Never sent, so it names the same image.
    url.set_fragment(None);
    Ok(url)
}

#[derive(Deserialize)]
pub(super) struct ImageQuery {
    url: Option<String>,
}

/// `GET /plugins/{id}/images?url=<https URL>`: an image from one of the
/// plugin's domains, for a member.
pub(super) async fn get_image(
    State(state): State<Arc<AppState>>,
    UrlPath(id): UrlPath<String>,
    Query(query): Query<ImageQuery>,
    headers: HeaderMap,
) -> Response {
    let user_id = match authorize(&state, &headers) {
        Ok(user_id) => user_id,
        Err(error) => return reply_error(&error),
    };
    if !state.config.plugins.enabled {
        return super::http::plugins_off();
    }
    let Some(handle) = state.plugins.get(&id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !handle.holds(PluginPermission::Internet) {
        return Refusal::forbidden("this plugin has no internet access").into_response();
    }
    let url = match image_url(query.url.as_deref(), &handle.domains) {
        Ok(url) => url,
        Err(refusal) => return refusal.into_response(),
    };
    state
        .plugins
        .images
        .serve(&state, handle, user_id, url)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guard(domains: &[&str]) -> NetGuard {
        let domains: Vec<String> = domains.iter().map(|domain| domain.to_string()).collect();
        NetGuard::new(&domains)
    }

    fn refused(raw: &str, domains: &NetGuard) -> (StatusCode, String) {
        let refusal = image_url(Some(raw), domains).unwrap_err();
        (refusal.status, refusal.error.message)
    }

    // Fails if a URL off https, off the domains, too long or with a
    // fragment that splits the cache gets through as it is.
    #[test]
    fn only_https_urls_on_the_domains_are_taken() {
        let domains = guard(&["img.example.com"]);
        let url = image_url(Some("https://img.example.com/a.png#x"), &domains).unwrap();
        assert_eq!(url.as_str(), "https://img.example.com/a.png");
        assert_eq!(
            refused("http://img.example.com/a.png", &domains),
            (
                StatusCode::BAD_REQUEST,
                "only https images can be shown".to_owned()
            )
        );
        assert_eq!(
            refused("https://evil.example.com/a.png", &domains),
            (
                StatusCode::FORBIDDEN,
                "evil.example.com isn't in the plugin's domains".to_owned()
            )
        );
        assert_eq!(
            refused("https://127.0.0.1/a.png", &domains).0,
            StatusCode::FORBIDDEN
        );
        let long = format!(
            "https://img.example.com/{}",
            "a".repeat(VIEW_MAX_PROP_CHARS)
        );
        assert_eq!(refused(&long, &domains).0, StatusCode::BAD_REQUEST);
        assert!(image_url(None, &domains).is_err());
    }

    // Fails if anything but the four image signatures passes, or one of
    // them is refused.
    #[test]
    fn only_the_four_image_types_pass() {
        let mut webp = b"RIFF\0\0\0\0WEBP".to_vec();
        webp.truncate(SNIFF_BYTES);
        assert_eq!(
            image_format(b"\x89PNG\r\n\x1a\n\0\0\0\0").ok(),
            Some(ImageFormat::Png)
        );
        assert_eq!(
            image_format(b"\xff\xd8\xff\xe0").ok(),
            Some(ImageFormat::Jpeg)
        );
        assert_eq!(image_format(b"GIF89a").ok(), Some(ImageFormat::Gif));
        assert_eq!(image_format(&webp).ok(), Some(ImageFormat::WebP));
        for other in [&b"<svg xmlns"[..], b"RIFF\0\0\0\0WAVE", b"OggS", b""] {
            let failed = image_format(other).unwrap_err();
            assert_eq!(failed.refusal.error.code, proto::ErrorCode::FileTypeBlocked);
        }
    }
}
