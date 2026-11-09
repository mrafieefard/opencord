// SPDX-License-Identifier: MPL-2.0

//! A plugin's client files, assets and files over HTTP (design 2026-11-07
//! §13.1, plan M2 Task 9).
//!
//! Every route needs a member's media token. Nothing is read from disk
//! until the path is known to be one the route may serve: a package path
//! listed in the installed version's [`ClientFiles`], or a file the
//! plugin's `plugin_files` rows hold. A request's path is never joined
//! onto a folder before that.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Path as UrlPath, Query, State};
use axum::http::header::{
    CACHE_CONTROL, CONTENT_LENGTH, CONTENT_TYPE, ETAG, IF_NONE_MATCH, X_CONTENT_TYPE_OPTIONS,
};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{AppendHeaders, IntoResponse, Response};
use bytes::Bytes;
use data_encoding::BASE64URL_NOPAD;
use futures_util::Stream;
use opencord_common::sniff::Sniffed;
use opencord_plugins::limits::{FILE_TOKEN_LIFETIME, FILE_TOKENS_PER_PLUGIN};
use opencord_plugins::package::Package;
use opencord_proto::v1 as proto;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt as _;

use super::PluginHandle;
use super::api::files::check_file_path;
use crate::db;
use crate::error::ApiError;
use crate::media::{authorize, reply_error};
use crate::random;
use crate::state::{AppState, now_ms};

/// The folder in `data/plugins/<id>/` that `files.put` writes to.
pub(crate) const FILES_FOLDER: &str = "files";
/// What a versioned asset may be cached as.
const IMMUTABLE: &str = "public, max-age=31536000, immutable";
/// How often expired file tokens are swept.
const TOKEN_SWEEP_EVERY: Duration = Duration::from_secs(60);
/// What a file is streamed in.
const READ_CHUNK: usize = 64 * 1024;

/// One file of a package a view may load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientFile {
    /// Lowercase hex SHA-256.
    pub sha256: String,
    pub size: u64,
}

/// Every file under `client/` and `assets/` of one installed version,
/// worked out once per version: a plugin started again on the same folder
/// keeps its running copy's.
#[derive(Debug)]
pub struct ClientFiles {
    /// The version's folder, `data/plugins/<id>/v<ms>`.
    folder: PathBuf,
    files: BTreeMap<String, ClientFile>,
}

impl ClientFiles {
    /// Hashes every `client/` and `assets/` file of `package`, which is
    /// written out in `folder`. Reads every byte: run it off the async
    /// threads.
    pub fn of(package: &Package, folder: PathBuf) -> ClientFiles {
        let files = package
            .paths()
            .filter(|path| path.starts_with("client/") || path.starts_with("assets/"))
            .filter_map(|path| {
                let bytes = package.get(path)?;
                let file = ClientFile {
                    sha256: hex::encode(Sha256::digest(bytes)),
                    size: bytes.len() as u64,
                };
                Some((path.to_owned(), file))
            })
            .collect();
        ClientFiles { folder, files }
    }

    pub fn folder(&self) -> &Path {
        &self.folder
    }
}

/// The installed version's client files and assets, in path order, for
/// `ViewOpened`.
pub fn client_files(handle: &PluginHandle) -> Vec<proto::PluginFileRef> {
    handle
        .client
        .files
        .iter()
        .map(|(path, file)| proto::PluginFileRef {
            path: path.clone(),
            sha256: file.sha256.clone(),
            size_bytes: file.size,
        })
        .collect()
}

/// What one file token opens.
struct Grant {
    plugin_id: String,
    path: String,
    user_id: i64,
    expires_at_ms: i64,
}

#[derive(Default)]
struct Entries {
    by_token: HashMap<String, Grant>,
    /// Each plugin's tokens, by the file's path and the member.
    by_plugin: HashMap<String, HashMap<(String, i64), String>>,
}

/// Tokens that let one member load one of a plugin's private files
/// (§13.1). In memory only.
///
/// **An entry leaves** once it expires ([`FileTokens::sweep`], every
/// minute; an expired token is refused before that), when its plugin is
/// uninstalled ([`FileTokens::forget_plugin`]), and when the server
/// restarts. A plugin's restart keeps them, as it keeps its views; a stop
/// closes its views, whose tokens then run out within the hour.
#[derive(Default)]
pub struct FileTokens {
    entries: Mutex<Entries>,
    /// Added to the clock; moved only by tests.
    offset_ms: AtomicI64,
}

impl FileTokens {
    /// The time tokens expire by.
    pub fn now_ms(&self) -> i64 {
        now_ms() + self.offset_ms.load(Ordering::Relaxed)
    }

    /// For tests: tokens see the time as `by` later from now on.
    pub fn move_clock(&self, by: Duration) {
        let by = i64::try_from(by.as_millis()).unwrap_or(i64::MAX);
        self.offset_ms.fetch_add(by, Ordering::Relaxed);
    }

    /// A token for `user_id` to load the plugin's file at `path`, good for
    /// [`FILE_TOKEN_LIFETIME`]: 32 random bytes, URL-safe base64. Asking
    /// again for the same file and member renews that token and returns it,
    /// so a plugin making one per redraw doesn't pile them up.
    /// `PLUGIN_LIMIT` past [`FILE_TOKENS_PER_PLUGIN`] live tokens.
    pub fn issue(&self, plugin_id: &str, path: &str, user_id: i64) -> Result<String, ApiError> {
        let now = self.now_ms();
        let lifetime = i64::try_from(FILE_TOKEN_LIFETIME.as_millis()).unwrap_or(i64::MAX);
        let expires_at_ms = now.saturating_add(lifetime);
        let mut entries = self.lock();
        let Entries {
            by_token,
            by_plugin,
        } = &mut *entries;
        let tokens = by_plugin.entry(plugin_id.to_owned()).or_default();
        let pair = (path.to_owned(), user_id);
        if let Some(token) = tokens.get(&pair)
            && let Some(grant) = by_token.get_mut(token)
        {
            grant.expires_at_ms = expires_at_ms;
            return Ok(token.clone());
        }
        if tokens.len() >= FILE_TOKENS_PER_PLUGIN {
            // Expired ones wait for the sweep: make room from them first.
            tokens.retain(|_, token| {
                let live = by_token
                    .get(token)
                    .is_some_and(|grant| grant.expires_at_ms > now);
                if !live {
                    by_token.remove(token);
                }
                live
            });
            if tokens.len() >= FILE_TOKENS_PER_PLUGIN {
                return Err(ApiError::new(
                    proto::ErrorCode::PluginLimit,
                    format!("at most {FILE_TOKENS_PER_PLUGIN} file tokens at a time"),
                ));
            }
        }
        let token = BASE64URL_NOPAD.encode(&random::bytes::<32>());
        tokens.insert(pair, token.clone());
        by_token.insert(
            token.clone(),
            Grant {
                plugin_id: plugin_id.to_owned(),
                path: path.to_owned(),
                user_id,
                expires_at_ms,
            },
        );
        Ok(token)
    }

    /// The path `token` opens, when it is live and was issued in
    /// `plugin_id` for `user_id`.
    pub fn path(&self, token: &str, plugin_id: &str, user_id: i64) -> Option<String> {
        let now = self.now_ms();
        self.lock()
            .by_token
            .get(token)
            .filter(|grant| {
                grant.plugin_id == plugin_id
                    && grant.user_id == user_id
                    && grant.expires_at_ms > now
            })
            .map(|grant| grant.path.clone())
    }

    /// Removes the expired tokens; how many there were.
    pub fn sweep(&self) -> usize {
        let now = self.now_ms();
        let mut entries = self.lock();
        let Entries {
            by_token,
            by_plugin,
        } = &mut *entries;
        let before = by_token.len();
        by_token.retain(|_, grant| grant.expires_at_ms > now);
        by_plugin.retain(|_, tokens| {
            tokens.retain(|_, token| by_token.contains_key(token));
            !tokens.is_empty()
        });
        before - by_token.len()
    }

    /// Removes every token of the plugin, as it's uninstalled.
    pub fn forget_plugin(&self, plugin_id: &str) {
        let mut entries = self.lock();
        let Entries {
            by_token,
            by_plugin,
        } = &mut *entries;
        if let Some(tokens) = by_plugin.remove(plugin_id) {
            for token in tokens.values() {
                by_token.remove(token);
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, Entries> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Sweeps expired file tokens every minute until shutdown.
pub async fn sweep_file_tokens(state: Arc<AppState>) {
    let mut every = tokio::time::interval(TOKEN_SWEEP_EVERY);
    loop {
        tokio::select! {
            _ = every.tick() => {}
            () = state.shutdown.cancelled() => return,
        }
        state.plugins.file_tokens.sweep();
    }
}

#[derive(Deserialize)]
pub(super) struct AssetQuery {
    v: Option<String>,
}

/// `GET /plugins/{id}/assets/{*path}`: a file under `client/` or `assets/`
/// of the installed version, tagged with its SHA-256; immutable when `v`
/// is that SHA-256.
pub(super) async fn get_asset(
    State(state): State<Arc<AppState>>,
    UrlPath((id, path)): UrlPath<(String, String)>,
    Query(query): Query<AssetQuery>,
    headers: HeaderMap,
) -> Response {
    if let Err(error) = authorize(&state, &headers) {
        return reply_error(&error);
    }
    if !state.config.plugins.enabled {
        return super::http::plugins_off();
    }
    let Some(handle) = state.plugins.get(&id) else {
        return not_found();
    };
    let client = handle.client.clone();
    // Only listed paths: `..`, empty segments and other folders never are.
    let Some(file) = client.files.get(&path) else {
        return not_found();
    };
    let etag = format!("\"{}\"", file.sha256);
    let cache = if query.v.as_deref() == Some(file.sha256.as_str()) {
        IMMUTABLE
    } else {
        "no-cache"
    };
    let tags = vec![(ETAG, etag.clone()), (CACHE_CONTROL, cache.to_owned())];
    if headers
        .get(IF_NONE_MATCH)
        .is_some_and(|value| etag_matches(value, &etag))
    {
        return (StatusCode::NOT_MODIFIED, AppendHeaders(tags)).into_response();
    }
    serve(&id, &client.folder.join(&path), &path, tags).await
}

/// `GET /plugins/{id}/files/{*path}`: one of the plugin's public files.
/// A private file is not found, as a missing one is.
pub(super) async fn get_file(
    State(state): State<Arc<AppState>>,
    UrlPath((id, path)): UrlPath<(String, String)>,
    headers: HeaderMap,
) -> Response {
    if let Err(error) = authorize(&state, &headers) {
        return reply_error(&error);
    }
    if !state.config.plugins.enabled {
        return super::http::plugins_off();
    }
    serve_kept(&state, &id, &path, Kept::Public).await
}

/// `GET /plugins/{id}/files/t/{token}`: the file a token was issued for,
/// to the member it was issued for. `t/<name>` is also a public file's
/// path, so a token that opens nothing is tried as one.
pub(super) async fn get_token_file(
    State(state): State<Arc<AppState>>,
    UrlPath((id, token)): UrlPath<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let user_id = match authorize(&state, &headers) {
        Ok(user_id) => user_id,
        Err(error) => return reply_error(&error),
    };
    if !state.config.plugins.enabled {
        return super::http::plugins_off();
    }
    match state.plugins.file_tokens.path(&token, &id, user_id) {
        Some(path) => serve_kept(&state, &id, &path, Kept::Token).await,
        None => serve_kept(&state, &id, &format!("t/{token}"), Kept::Public).await,
    }
}

/// How a plugin file was asked for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kept {
    /// By its path: only a public file.
    Public,
    /// Through a token: any file the plugin keeps.
    Token,
}

/// Serves the plugin's file at `path` if the plugin keeps one there and
/// `kept` may have it.
async fn serve_kept(state: &AppState, id: &str, path: &str, kept: Kept) -> Response {
    if check_file_path(path).is_err() {
        return not_found();
    }
    let public = match state.db.acquire().await {
        Ok(mut conn) => db::plugins::file_is_public(&mut conn, id, path).await,
        Err(error) => Err(error),
    };
    let public = match public {
        Ok(public) => public,
        Err(error) => {
            tracing::error!(plugin = %id, %error, "could not look up a plugin file");
            return reply_error(&ApiError::internal());
        }
    };
    let cache = match (public, kept) {
        (Some(true), Kept::Public) => "no-cache",
        (Some(_), Kept::Token) => "private, no-cache",
        _ => return not_found(),
    };
    // `id` and `path` name a row the plugin wrote, so both are safe here.
    let disk = state
        .config
        .plugins_dir()
        .join(id)
        .join(FILES_FOLDER)
        .join(path);
    serve(id, &disk, path, vec![(CACHE_CONTROL, cache.to_owned())]).await
}

/// Streams the file at `disk`, typed by `path`'s extension.
async fn serve(
    plugin_id: &str,
    disk: &Path,
    path: &str,
    headers: Vec<(HeaderName, String)>,
) -> Response {
    let file = match tokio::fs::File::open(disk).await {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return not_found(),
        Err(error) => {
            tracing::warn!(plugin = %plugin_id, kind = ?error.kind(), "could not open a plugin file");
            return reply_error(&ApiError::internal());
        }
    };
    // The open file's length: replacing the file (files.put renames over
    // it) doesn't change what this handle reads.
    let size = match file.metadata().await {
        Ok(metadata) if metadata.is_file() => metadata.len(),
        Ok(_) => return not_found(),
        Err(error) => {
            tracing::warn!(plugin = %plugin_id, kind = ?error.kind(), "could not read a plugin file's size");
            return reply_error(&ApiError::internal());
        }
    };
    (
        StatusCode::OK,
        [
            (CONTENT_TYPE, content_type(path).to_owned()),
            (CONTENT_LENGTH, size.to_string()),
            (X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
        ],
        AppendHeaders(headers),
        Body::from_stream(chunks(file, size)),
    )
        .into_response()
}

/// `size` bytes of `file`, a piece at a time. An error ends the reply
/// short, and the client sees a failed download.
fn chunks(file: tokio::fs::File, size: u64) -> impl Stream<Item = std::io::Result<Bytes>> + Send {
    futures_util::stream::try_unfold(file.take(size), |mut reader| async move {
        let mut buffer = vec![0; READ_CHUNK];
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            return Ok(None);
        }
        buffer.truncate(read);
        Ok(Some((Bytes::from(buffer), reader)))
    })
}

/// The type a plugin file is served as, from its extension: images, fonts,
/// audio and Luau source; anything else is `application/octet-stream`, so
/// nothing a plugin stores runs as a page on the server's origin.
fn content_type(path: &str) -> &'static str {
    let name = path.rsplit('/').next().unwrap_or(path);
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    let sniffed = match extension.as_str() {
        "png" => Sniffed::Png,
        "jpg" | "jpeg" => Sniffed::Jpeg,
        "webp" => Sniffed::WebP,
        "gif" => Sniffed::Gif,
        "ogg" => Sniffed::Ogg,
        "mp3" => Sniffed::Mp3,
        "wav" => Sniffed::Wav,
        "ttf" => return "font/ttf",
        "otf" => return "font/otf",
        "lua" | "luau" => return "text/plain; charset=utf-8",
        _ => Sniffed::Unknown,
    };
    sniffed.content_type()
}

/// Whether an `If-None-Match` header names `etag` (or is `*`).
fn etag_matches(header: &HeaderValue, etag: &str) -> bool {
    let Ok(header) = header.to_str() else {
        return false;
    };
    header.split(',').map(str::trim).any(|candidate| {
        candidate == "*" || candidate.strip_prefix("W/").unwrap_or(candidate) == etag
    })
}

fn not_found() -> Response {
    StatusCode::NOT_FOUND.into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: Duration = Duration::from_secs(60 * 60);

    fn live(tokens: &FileTokens) -> (usize, usize) {
        let entries = tokens.lock();
        (entries.by_token.len(), entries.by_plugin.len())
    }

    // Fails if asking again for the same file and member makes a second
    // token: a plugin calling `files.token` per redraw would fill the map.
    #[test]
    fn the_same_file_and_member_renew_one_token() {
        let tokens = FileTokens::default();
        let first = tokens.issue("p", "a.png", 1).unwrap();
        tokens.move_clock(HOUR / 2);
        assert_eq!(tokens.issue("p", "a.png", 1).unwrap(), first);
        tokens.move_clock(HOUR * 3 / 4);
        assert_eq!(tokens.path(&first, "p", 1).as_deref(), Some("a.png"));
        assert_ne!(tokens.issue("p", "a.png", 2).unwrap(), first);
        assert_eq!(live(&tokens), (2, 1));
    }

    // Fails if the sweep leaves expired tokens or an emptied plugin entry.
    #[test]
    fn the_sweep_removes_expired_tokens_and_empty_plugins() {
        let tokens = FileTokens::default();
        let old = tokens.issue("p", "a.png", 1).unwrap();
        tokens.move_clock(HOUR / 2);
        let newer = tokens.issue("q", "b.png", 1).unwrap();
        tokens.move_clock(HOUR / 2 + Duration::from_secs(1));
        assert_eq!(tokens.sweep(), 1);
        assert_eq!(live(&tokens), (1, 1));
        assert_eq!(tokens.path(&old, "p", 1), None);
        assert_eq!(tokens.path(&newer, "q", 1).as_deref(), Some("b.png"));
    }

    // Fails if forgetting a plugin keeps its tokens, or drops another's.
    #[test]
    fn forgetting_a_plugin_drops_only_its_tokens() {
        let tokens = FileTokens::default();
        let mine = tokens.issue("p", "a.png", 1).unwrap();
        let theirs = tokens.issue("q", "a.png", 1).unwrap();
        tokens.forget_plugin("p");
        assert_eq!(tokens.path(&mine, "p", 1), None);
        assert_eq!(tokens.path(&theirs, "q", 1).as_deref(), Some("a.png"));
        assert_eq!(live(&tokens), (1, 1));
    }

    // Fails if a plugin can hold more live tokens than the cap, or if
    // expired ones count against it.
    #[test]
    fn a_plugin_holds_at_most_the_cap() {
        let tokens = FileTokens::default();
        for user in 0..FILE_TOKENS_PER_PLUGIN {
            tokens.issue("p", "a.png", user as i64).unwrap();
        }
        let error = tokens.issue("p", "a.png", -1).unwrap_err();
        assert_eq!(error.code, proto::ErrorCode::PluginLimit);
        assert_eq!(
            error.message,
            format!("at most {FILE_TOKENS_PER_PLUGIN} file tokens at a time")
        );
        assert!(tokens.issue("q", "a.png", -1).is_ok());
        tokens.move_clock(HOUR + Duration::from_secs(1));
        assert!(tokens.issue("p", "a.png", -1).is_ok());
        assert_eq!(live(&tokens), (2, 2));
    }

    // Fails if a type comes from anything but the allowed extensions.
    #[test]
    fn content_types_come_from_the_allowed_extensions() {
        assert_eq!(content_type("assets/a.PNG"), "image/png");
        assert_eq!(content_type("assets/a.jpeg"), "image/jpeg");
        assert_eq!(content_type("assets/a.mp3"), "audio/mpeg");
        assert_eq!(content_type("assets/a.otf"), "font/otf");
        assert_eq!(content_type("client/main.lua"), "text/plain; charset=utf-8");
        assert_eq!(content_type("page.html"), "application/octet-stream");
        assert_eq!(content_type("x.svg"), "application/octet-stream");
        assert_eq!(content_type("png"), "application/octet-stream");
        assert_eq!(content_type("a.png/b"), "application/octet-stream");
    }
}
