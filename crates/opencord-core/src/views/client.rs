//! Plugin views over the network (plan M2 Task 15, spec §11): opening a
//! view on the server, loading the plugin's client files by hash, starting
//! its VM, and carrying events both ways. One service per client, shared by
//! every server's connection.
//!
//! - **Opening:** `OpenView`; a manifest `api` above this app's sends
//!   `too_old`; otherwise every `client/` file of `ViewOpened.files` comes
//!   from the cache (`<data>/cache/plugins/<sha256>`) or the assets route
//!   with `?v=`, each checked against its SHA-256, and the VM starts.
//! - **Routing:** `ViewMessage` → the VM's `app.on`; `ViewClosed` → a
//!   `closed` op, then the VM ends; `ViewScriptsUpdate` → the new files,
//!   then a reload keeping the state; `app.send` → `SendViewEvent`; errors
//!   → `ReportViewError`, one per view every `VIEW_ERROR_REPORT_EVERY`.
//! - **Places:** a plugin channel or an activity session in a call
//!   ([`At`]), sent as the `OpenView`'s place and shown to the VM.
//! - **Connection:** losing it tells each view (`connection_lost`); a
//!   resumed session tells them it's back; a new session opens every view
//!   again on the server under the same core id, keeping its VM and state.
//!   An activity's view whose session is gone then ends `activity_ended`;
//!   one refused for not being in the call waits while the core rejoins it
//!   ([`REJOIN_GRACE`]), else ends `left_call`.
//!
//! Maps and when their entries leave:
//! - `views` (core id → view) and `by_server` ((server, server view id) →
//!   core id): on `close`, on `ViewClosed`, when reopening fails, and when
//!   the app removes the server (`close_server`). A new session drops the
//!   `by_server` entries of its server and adds the new ids.
//! - `expected` (views the server opened that `open` hasn't claimed yet,
//!   with what arrived for them): when claimed, when the open fails after
//!   the server's answer, and on every new session or lost connection of
//!   their server; at most [`MAX_EXPECTED`] at once. While an `OpenView` is
//!   on its way, what comes for a view not known yet starts an entry: the
//!   server queues `view_open` for the plugin before it answers, so the
//!   plugin's first message can arrive before the answer.
//! - `opening` (each server's `OpenView`s waiting for their answer): until
//!   the answer, or the request failing.
//! - `viewers` (who this device is on each server): replaced by each
//!   `Ready`; on `close_server`.
//! - `assets` and the image files: see [`files`].

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::{Duration, Instant};

use futures_util::future::BoxFuture;
use opencord_plugins::API_VERSION;
use opencord_plugins::limits::{
    PACKAGE_MAX_FILE_BYTES, VIEW_ERROR_REPORT_EVERY, VIEW_ERROR_REPORT_MAX_CHARS,
};
use opencord_plugins::view::{Batch, CloseReason, Patch};
use opencord_proto::v1 as proto;
use proto::request::Kind as Request;
use proto::response::Result as Response;
use sha2::{Digest, Sha256};
use tokio::runtime::Handle;
use tokio::sync::mpsc;

use super::{Place, ViewContext, ViewHandle, ViewSink, ViewSpec, Viewer, start};
use crate::api::types::{CoreError, ErrorCode};
use crate::http::HttpResponse;

#[path = "client_files.rs"]
mod files;
use files::Known;

/// A plugin's client files together, in bytes.
const CLIENT_FILES_MAX_BYTES: u64 = 16 * 1024 * 1024;
/// Events and error reports waiting to leave one view, in order.
const OUTGOING_QUEUE: usize = 256;
/// Views the server opened that no `open` has claimed yet.
const MAX_EXPECTED: usize = 16;
/// What one unclaimed view keeps of what arrived for it.
const MAX_EARLY: usize = 64;
/// The platform every view opened here says it's on (§11.4). The phone
/// opens no views before milestone 4.
const PLATFORM: &str = "desktop";
/// The words of an `error` op the core sends itself, as the VM's.
const ERROR_MESSAGE: &str = "This plugin hit an error";
/// How long an activity's view opening again on a new session waits for
/// this device to get back into its call (the core rejoins it then, at most
/// five tries a second or more apart), and how often it asks meanwhile.
const REJOIN_GRACE: Duration = Duration::from_secs(15);
const REOPEN_RETRY: Duration = Duration::from_millis(250);

pub use crate::api::types::ViewOutput;

/// How the service reaches a server: its connection's requests and its
/// HTTP routes with this device's media token.
pub(crate) trait ViewTransport: Send + Sync {
    fn request(&self, key: &str, kind: Request) -> BoxFuture<'static, Result<Response, CoreError>>;
    /// `GET path`, reading at most `limit` bytes of the answer.
    fn get(
        &self,
        key: &str,
        path: String,
        limit: usize,
    ) -> BoxFuture<'static, Result<HttpResponse, CoreError>>;
    /// This device is in, or getting back into, the call `channel_id` on
    /// `key`.
    fn in_call(&self, _key: &str, _channel_id: i64) -> bool {
        false
    }
}

#[derive(Clone)]
pub(crate) struct Views {
    shared: Arc<Shared>,
}

struct Shared {
    runtime: Handle,
    transport: Arc<dyn ViewTransport>,
    /// `<data>/cache/plugins`: client files by their SHA-256.
    cache_dir: PathBuf,
    /// `<data>/plugins`: each plugin's device storage under it.
    storage_root: PathBuf,
    outputs: Mutex<Option<mpsc::UnboundedSender<ViewOutput>>>,
    state: Mutex<State>,
    next_id: AtomicU64,
    /// Cached files' hashes checked this run.
    verified: Mutex<HashSet<String>>,
    /// This run's image files, under `<data>/cache/plugin-images`.
    images_dir: PathBuf,
    /// Their names, oldest first.
    images: Mutex<VecDeque<String>>,
}

#[derive(Default)]
struct State {
    views: HashMap<String, Open>,
    by_server: HashMap<(String, i64), String>,
    expected: HashMap<(String, i64), Vec<Early>>,
    viewers: HashMap<String, Viewer>,
    /// (server, plugin) → asset path → its hash and size.
    assets: HashMap<(String, String), HashMap<String, Known>>,
    /// Each server's `OpenView`s waiting for their answer.
    opening: HashMap<String, usize>,
}

/// Where a view is open on its server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum At {
    /// A plugin channel.
    Channel(i64),
    /// An activity session in the call `channel_id`.
    Activity {
        session_id: i64,
        channel_id: i64,
        activity_key: String,
    },
}

impl At {
    fn wire(&self) -> proto::view_place::Kind {
        match self {
            Self::Channel(channel_id) => proto::view_place::Kind::ChannelId(*channel_id),
            Self::Activity { session_id, .. } => {
                proto::view_place::Kind::ActivitySessionId(*session_id)
            }
        }
    }

    fn place(&self) -> Place {
        match self {
            Self::Channel(channel_id) => Place::Channel {
                channel_id: channel_id.to_string(),
            },
            Self::Activity {
                session_id,
                channel_id,
                activity_key,
            } => Place::Activity {
                session_id: session_id.to_string(),
                channel_id: channel_id.to_string(),
                activity: activity_key.clone(),
            },
        }
    }
}

struct Open {
    key: String,
    at: At,
    plugin_id: String,
    /// `None` while it's being opened again on a new session.
    server_id: Option<i64>,
    /// The new sessions it was opened again on, counted.
    attempt: u64,
    /// The client files it runs, by path, with their hashes.
    files: BTreeMap<String, String>,
    /// The view was told the connection dropped and not yet that it's back.
    lost: bool,
    handle: Arc<ViewHandle>,
}

/// What arrived for a view before `open` claimed it.
enum Early {
    Message { name: String, data_json: String },
    Closed(CloseReason),
}

enum Outgoing {
    Event { name: String, data_json: String },
    Error(String),
}

impl Views {
    pub(crate) fn new(runtime: Handle, data_dir: &Path, transport: Arc<dyn ViewTransport>) -> Self {
        let images_dir = files::images_dir(&runtime, data_dir);
        Self {
            shared: Arc::new(Shared {
                runtime,
                transport,
                cache_dir: data_dir.join("cache").join("plugins"),
                storage_root: data_dir.join("plugins"),
                outputs: Mutex::new(None),
                state: Mutex::new(State::default()),
                next_id: AtomicU64::new(1),
                verified: Mutex::default(),
                images_dir,
                images: Mutex::default(),
            }),
        }
    }

    /// Every view's batches from now on; an earlier receiver hears nothing
    /// more. A view's first batches can arrive before `open` returns its
    /// id, so the receiver keeps what it gets for ids it doesn't know yet.
    pub(crate) fn outputs(&self) -> mpsc::UnboundedReceiver<ViewOutput> {
        let (sender, receiver) = mpsc::unbounded_channel();
        *lock(&self.shared.outputs) = Some(sender);
        receiver
    }

    /// Who this device is on `key`, from its `Ready` and member updates.
    pub(crate) fn set_viewer(&self, key: &str, viewer: Viewer) {
        self.shared.lock().viewers.insert(key.to_owned(), viewer);
    }

    /// Opens a view of a plugin channel or an activity session; the core's
    /// own id for it, kept across sessions. Its batches arrive on
    /// [`Self::outputs`].
    pub(crate) async fn open(
        &self,
        key: &str,
        at: At,
        context_json: &str,
    ) -> Result<String, CoreError> {
        let context: ViewContext =
            serde_json::from_str(context_json).map_err(|error| CoreError::InvalidInput {
                message: format!("the view's context isn't valid: {error}"),
            })?;
        let viewer = self
            .shared
            .lock()
            .viewers
            .get(key)
            .cloned()
            .ok_or(CoreError::NotConnected)?;
        let began = Instant::now();
        let opened = self.shared.open_on_server(key, &at).await?;
        let id = self
            .shared
            .next_id
            .fetch_add(1, Ordering::Relaxed)
            .to_string();
        let server = crate::log::alias::server(key);
        if opened.api > API_VERSION {
            tracing::info!(%server, view = %id, api = opened.api, "a view needs a newer app");
            self.shared.abandon(key, opened.view_id);
            self.shared.emit_alone(&id, Patch::TooOld);
            return Ok(id);
        }
        self.shared
            .remember_assets(key, &opened.plugin_id, &opened.files);
        let scripts = scripts_of(&opened.files);
        let sources = match self
            .shared
            .load(key, &opened.plugin_id, &opened.files)
            .await
        {
            Ok(sources) => sources,
            Err(problem) => {
                tracing::warn!(%server, view = %id, after_ms = began.elapsed().as_millis() as u64, "a view's client files couldn't be loaded");
                self.shared.abandon(key, opened.view_id);
                let details = opened.show_errors.then_some(problem);
                self.shared.emit_alone(
                    &id,
                    Patch::Error {
                        message: ERROR_MESSAGE.to_owned(),
                        details,
                    },
                );
                return Ok(id);
            }
        };
        let (outgoing, queue) = mpsc::channel(OUTGOING_QUEUE);
        let sink = Arc::new(Sink {
            view_id: id.clone(),
            shared: Arc::downgrade(&self.shared),
            outgoing,
            last_report: Mutex::new(None),
        });
        let spec = ViewSpec {
            view_id: id.clone(),
            plugin_id: opened.plugin_id.clone(),
            entry: opened.entry.clone(),
            sources,
            context,
            viewer,
            place: at.place(),
            show_errors: opened.show_errors,
            storage_dir: self.shared.storage_dir(key, &opened.plugin_id),
        };
        let handle = Arc::new(start(spec, sink));
        self.shared.runtime.spawn(Shared::send_out(
            Arc::downgrade(&self.shared),
            id.clone(),
            queue,
        ));
        let view = Open {
            key: key.to_owned(),
            at,
            plugin_id: opened.plugin_id,
            server_id: None,
            attempt: 0,
            files: scripts,
            lost: false,
            handle,
        };
        self.shared.lock().views.insert(id.clone(), view);
        self.shared.claim(&id, key, opened.view_id);
        tracing::info!(%server, view = %id, after_ms = began.elapsed().as_millis() as u64, "view opened");
        Ok(id)
    }

    /// An input from the renderer, as JSON; one for a closed view is
    /// dropped.
    pub(crate) fn input(&self, view_id: &str, json: &str) {
        let handle = self
            .shared
            .lock()
            .views
            .get(view_id)
            .map(|view| view.handle.clone());
        match handle {
            Some(handle) => handle.input(json),
            None => tracing::debug!(view = %view_id, "an input for a closed view"),
        }
    }

    /// The server a view is open on and the server's id for it, `None`
    /// while it's being opened again; `None` for a view that isn't open.
    pub(crate) fn place_of(&self, view_id: &str) -> Option<(String, Option<i64>)> {
        let state = self.shared.lock();
        let view = state.views.get(view_id)?;
        Some((view.key.clone(), view.server_id))
    }

    /// Closes a view: the server hears it, and the VM runs its `close`
    /// listeners and ends, off the caller's thread.
    pub(crate) fn close(&self, view_id: &str) {
        let removed = self.shared.remove(view_id);
        let Some(view) = removed else { return };
        if let Some(server_id) = view.server_id {
            self.shared.close_on_server(&view.key, server_id);
        }
        self.shared
            .runtime
            .spawn_blocking(move || view.handle.close());
    }

    /// The app removed the server: its views end without a word to it.
    pub(crate) fn close_server(&self, key: &str) {
        let mut state = self.shared.lock();
        state.viewers.remove(key);
        state.assets.retain(|(server, _), _| server != key);
        state.expected.retain(|(server, _), _| server != key);
        state.by_server.retain(|(server, _), _| server != key);
        let ids: Vec<String> = state
            .views
            .iter()
            .filter(|(_, view)| view.key == key)
            .map(|(id, _)| id.clone())
            .collect();
        let handles: Vec<Arc<ViewHandle>> = ids
            .iter()
            .filter_map(|id| state.views.remove(id))
            .map(|view| view.handle)
            .collect();
        drop(state);
        // Dropping a handle closes its view without waiting.
        drop(handles);
    }

    /// The server answered `OpenView`: what arrives for that view before
    /// `open` claims it is kept. Called by the connection, in order with
    /// its events.
    pub(crate) fn expect(&self, key: &str, server_id: i64) {
        let mut state = self.shared.lock();
        let at = (key.to_owned(), server_id);
        if state.expected.contains_key(&at) {
            // Something came for it before the answer: keep it.
            return;
        }
        if state.expected.len() >= MAX_EXPECTED {
            tracing::warn!(
                view = server_id,
                "too many views opening at once; one won't hear what came early"
            );
            return;
        }
        state.expected.insert(at, Vec::new());
    }

    pub(crate) fn on_message(&self, key: &str, message: proto::ViewMessage) {
        let mut state = self.shared.lock();
        let at = (key.to_owned(), message.view_id);
        if let Some(early) = state.early_for(&at) {
            if early.len() < MAX_EARLY {
                early.push(Early::Message {
                    name: message.name,
                    data_json: message.data_json,
                });
            }
            return;
        }
        let handle = state
            .by_server
            .get(&at)
            .and_then(|id| state.views.get(id))
            .map(|view| view.handle.clone());
        drop(state);
        match handle {
            Some(handle) => handle.server_message(message.name, message.data_json),
            None => tracing::debug!(
                view = message.view_id,
                "a message for a view that isn't open"
            ),
        }
    }

    pub(crate) fn on_closed(&self, key: &str, closed: proto::ViewClosed) {
        let reason = close_reason(closed.reason);
        let mut state = self.shared.lock();
        let at = (key.to_owned(), closed.view_id);
        if let Some(early) = state.early_for(&at) {
            early.push(Early::Closed(reason));
            return;
        }
        let id = state.by_server.get(&at).cloned();
        drop(state);
        let Some(view) = id.and_then(|id| self.shared.remove(&id)) else {
            return;
        };
        tracing::info!(view = closed.view_id, ?reason, "the server closed a view");
        view.handle.finish(Patch::Closed { reason });
    }

    /// New client files for a plugin's open views on `key`: loaded once,
    /// then each view reloads keeping its state.
    pub(crate) fn on_scripts(&self, key: &str, update: proto::ViewScriptsUpdate) {
        self.shared
            .remember_assets(key, &update.plugin_id, &update.files);
        let shared = self.shared.clone();
        let key = key.to_owned();
        self.shared.runtime.spawn(async move {
            let scripts = scripts_of(&update.files);
            let ids: Vec<String> = shared
                .lock()
                .views
                .iter()
                .filter(|(_, view)| view.key == key && view.plugin_id == update.plugin_id)
                .map(|(id, _)| id.clone())
                .collect();
            if ids.is_empty() {
                return;
            }
            match shared.load(&key, &update.plugin_id, &update.files).await {
                Ok(sources) => {
                    let mut state = shared.lock();
                    for id in &ids {
                        if let Some(view) = state.views.get_mut(id) {
                            view.files = scripts.clone();
                            view.handle.reload(update.entry.clone(), sources.clone());
                        }
                    }
                }
                Err(problem) => {
                    tracing::warn!(plugin = %update.plugin_id, "new client files couldn't be loaded; the views keep the old ones");
                    for id in &ids {
                        shared.queue(id, Outgoing::Error(format!("reload: {problem}")));
                    }
                }
            }
        });
    }

    /// The connection to `key` dropped: each view hears it once.
    pub(crate) fn on_lost(&self, key: &str) {
        let mut state = self.shared.lock();
        state.expected.retain(|(server, _), _| server != key);
        for view in state.views.values_mut().filter(|view| view.key == key) {
            if !view.lost {
                view.lost = true;
                view.handle.input(LOST);
            }
        }
    }

    /// The connection to `key` is up: after a resume each view hears it's
    /// back; after a new session (`fresh`) each opens again on the server
    /// first, keeping its VM.
    pub(crate) fn on_session(&self, key: &str, fresh: bool) {
        let mut state = self.shared.lock();
        if !fresh {
            for view in state.views.values_mut().filter(|view| view.key == key) {
                if view.lost {
                    view.lost = false;
                    view.handle.input(BACK);
                }
            }
            return;
        }
        state.expected.retain(|(server, _), _| server != key);
        state.by_server.retain(|(server, _), _| server != key);
        state.assets.retain(|(server, _), _| server != key);
        let mut ids = Vec::new();
        for (id, view) in state.views.iter_mut().filter(|(_, view)| view.key == key) {
            view.server_id = None;
            view.attempt += 1;
            ids.push((id.clone(), view.attempt));
        }
        drop(state);
        for (id, attempt) in ids {
            let shared = self.shared.clone();
            self.shared
                .runtime
                .spawn(async move { shared.reopen(id, attempt).await });
        }
    }
}

impl State {
    /// Where what arrives for the server's view `at` waits, if it's a view
    /// being opened: one the server answered for, or, while an `OpenView`
    /// of that server is on its way, one not known yet.
    fn early_for(&mut self, at: &(String, i64)) -> Option<&mut Vec<Early>> {
        let unknown = !self.expected.contains_key(at) && !self.by_server.contains_key(at);
        let opening = self.opening.get(&at.0).is_some_and(|n| *n > 0);
        if unknown && opening && self.expected.len() < MAX_EXPECTED {
            self.expected.insert(at.clone(), Vec::new());
        }
        self.expected.get_mut(at)
    }
}

/// An `OpenView` on its way to `key`, counted in `State::opening` until
/// dropped.
struct Opening<'a> {
    shared: &'a Shared,
    key: String,
}

impl<'a> Opening<'a> {
    fn new(shared: &'a Shared, key: &str) -> Self {
        *shared.lock().opening.entry(key.to_owned()).or_default() += 1;
        Self {
            shared,
            key: key.to_owned(),
        }
    }
}

impl Drop for Opening<'_> {
    fn drop(&mut self) {
        let mut state = self.shared.lock();
        if let Some(count) = state.opening.get_mut(&self.key) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                state.opening.remove(&self.key);
            }
        }
    }
}

const LOST: &str = r#"{"kind":"lifecycle","event":"connection_lost"}"#;
const BACK: &str = r#"{"kind":"lifecycle","event":"connection_back"}"#;

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        lock(&self.state)
    }

    async fn open_on_server(&self, key: &str, at: &At) -> Result<proto::ViewOpened, CoreError> {
        let request = Request::OpenView(proto::OpenView {
            place: Some(proto::ViewPlace {
                kind: Some(at.wire()),
            }),
            platform: PLATFORM.to_owned(),
        });
        let _opening = Opening::new(self, key);
        match self.transport.request(key, request).await? {
            Response::ViewOpened(opened) => Ok(opened),
            _ => Err(CoreError::Connection {
                message: "the server sent an unexpected response".to_owned(),
            }),
        }
    }

    /// A view opened on the server that won't run here.
    fn abandon(&self, key: &str, server_id: i64) {
        self.lock().expected.remove(&(key.to_owned(), server_id));
        self.close_on_server(key, server_id);
    }

    fn close_on_server(&self, key: &str, server_id: i64) {
        let transport = self.transport.clone();
        let request = Request::CloseView(proto::CloseView { view_id: server_id });
        let key = key.to_owned();
        self.runtime.spawn(async move {
            if let Err(error) = transport.request(&key, request).await {
                // The server closes it with the session anyway.
                tracing::debug!(
                    view = server_id,
                    ?error,
                    "a view couldn't be closed on the server"
                );
            }
        });
    }

    /// Ties the server's view to the core's, and hands the VM what came
    /// before. False when the view closed meanwhile.
    fn claim(&self, id: &str, key: &str, server_id: i64) -> bool {
        let mut state = self.lock();
        let early = state
            .expected
            .remove(&(key.to_owned(), server_id))
            .unwrap_or_default();
        let Some(view) = state.views.get_mut(id) else {
            drop(state);
            self.close_on_server(key, server_id);
            return false;
        };
        view.server_id = Some(server_id);
        let handle = view.handle.clone();
        state
            .by_server
            .insert((key.to_owned(), server_id), id.to_owned());
        let mut closed = None;
        for item in early {
            match item {
                Early::Message { name, data_json } => handle.server_message(name, data_json),
                Early::Closed(reason) => closed = Some(reason),
            }
        }
        drop(state);
        if let Some(reason) = closed
            && let Some(view) = self.remove(id)
        {
            view.handle.finish(Patch::Closed { reason });
            return false;
        }
        true
    }

    fn remove(&self, id: &str) -> Option<Open> {
        let mut state = self.lock();
        let view = state.views.remove(id)?;
        if let Some(server_id) = view.server_id {
            state.by_server.remove(&(view.key.clone(), server_id));
        }
        Some(view)
    }

    /// Opens `id` again on a new session (its `attempt`). Refused, the view
    /// ends with the server's error, or for an activity with `closed`: its
    /// session gone, or this device out of its call. While this device is
    /// getting back into the call, a refusal for not being in it is tried
    /// again. Cut off again, it waits for the next session.
    async fn reopen(self: Arc<Self>, id: String, attempt: u64) {
        let Some((key, at, files)) = self
            .lock()
            .views
            .get(&id)
            .map(|view| (view.key.clone(), view.at.clone(), view.files.clone()))
        else {
            return;
        };
        let began = Instant::now();
        let opened = loop {
            let error = match self.open_on_server(&key, &at).await {
                Ok(opened) => break opened,
                Err(CoreError::NotConnected | CoreError::Timeout) => return,
                Err(error) => error,
            };
            let ending = match (&at, &error) {
                (At::Activity { .. }, CoreError::Server { code, .. })
                    if *code == ErrorCode::NotFound =>
                {
                    Patch::Closed {
                        reason: CloseReason::ActivityEnded,
                    }
                }
                (At::Activity { channel_id, .. }, CoreError::Server { code, .. })
                    if *code == ErrorCode::Forbidden =>
                {
                    let rejoining =
                        self.transport.in_call(&key, *channel_id) && began.elapsed() < REJOIN_GRACE;
                    if rejoining {
                        tokio::time::sleep(REOPEN_RETRY).await;
                        let current = self.lock().views.get(&id).map(|view| view.attempt);
                        if current != Some(attempt) {
                            // Closed, or a newer session opens it.
                            return;
                        }
                        continue;
                    }
                    Patch::Closed {
                        reason: CloseReason::LeftCall,
                    }
                }
                _ => Patch::Error {
                    message: error.to_string(),
                    details: None,
                },
            };
            tracing::info!(view = %id, ?error, "a view couldn't be opened again");
            if let Some(view) = self.remove(&id) {
                view.handle.finish(ending);
            }
            return;
        };
        if opened.api > API_VERSION {
            self.abandon(&key, opened.view_id);
            if let Some(view) = self.remove(&id) {
                view.handle.finish(Patch::TooOld);
            }
            return;
        }
        self.remember_assets(&key, &opened.plugin_id, &opened.files);
        let scripts = scripts_of(&opened.files);
        if scripts != files {
            match self.load(&key, &opened.plugin_id, &opened.files).await {
                Ok(sources) => {
                    if let Some(view) = self.lock().views.get_mut(&id) {
                        view.files = scripts;
                        view.handle.reload(opened.entry.clone(), sources);
                    }
                }
                Err(problem) => {
                    self.queue(&id, Outgoing::Error(format!("reload: {problem}")));
                }
            }
        }
        if self.claim(&id, &key, opened.view_id) {
            let mut state = self.lock();
            if let Some(view) = state.views.get_mut(&id) {
                view.lost = false;
                view.handle.input(BACK);
            }
            drop(state);
            tracing::info!(view = %id, "view opened again on a new session");
        }
    }

    /// Every `client/` file of `files`, as text, from the cache or the
    /// server, each checked against its hash.
    async fn load(
        &self,
        key: &str,
        plugin_id: &str,
        files: &[proto::PluginFileRef],
    ) -> Result<BTreeMap<String, String>, String> {
        check_id(plugin_id)?;
        let mut sources = BTreeMap::new();
        let mut total = 0;
        for file in files.iter().filter(|file| file.path.starts_with("client/")) {
            total += file.size_bytes;
            if file.size_bytes > PACKAGE_MAX_FILE_BYTES || total > CLIENT_FILES_MAX_BYTES {
                return Err(format!(
                    "the client files are over {CLIENT_FILES_MAX_BYTES} bytes"
                ));
            }
            let bytes = self.file(key, plugin_id, file).await?;
            let text =
                String::from_utf8(bytes).map_err(|_| format!("{} isn't UTF-8 text", file.path))?;
            sources.insert(file.path.clone(), text);
        }
        Ok(sources)
    }

    /// One file of the plugin's package by its hash: the cached copy, or
    /// the server's through the assets route, then cached.
    async fn file(
        &self,
        key: &str,
        plugin_id: &str,
        file: &proto::PluginFileRef,
    ) -> Result<Vec<u8>, String> {
        check_path(&file.path)?;
        let sha256 = &file.sha256;
        if sha256.len() != 64
            || !sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err(format!("{} has no valid hash", file.path));
        }
        let cached = self.cache_dir.join(sha256);
        match tokio::fs::read(&cached).await {
            Ok(bytes) if hash_of(bytes.clone()).await == *sha256 => return Ok(bytes),
            Ok(_) => {
                tracing::warn!("a cached plugin file didn't match its hash; fetching it again")
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                tracing::warn!(kind = ?error.kind(), "a cached plugin file couldn't be read; fetching it again")
            }
        }
        let path = format!("/plugins/{plugin_id}/assets/{}?v={sha256}", file.path);
        let limit = usize::try_from(file.size_bytes)
            .unwrap_or(usize::MAX)
            .saturating_add(1);
        let response = self
            .transport
            .get(key, path, limit)
            .await
            .map_err(|error| format!("{} couldn't be fetched: {error}", file.path))?;
        if response.status != 200 {
            return Err(format!(
                "{} couldn't be fetched (HTTP {})",
                file.path, response.status
            ));
        }
        if hash_of(response.body.clone()).await != *sha256 {
            return Err(format!("{} doesn't match its hash", file.path));
        }
        if let Err(error) = write_aside(&cached, &response.body).await {
            // Only a cache: the next open fetches it again.
            tracing::warn!(kind = ?error.kind(), "a plugin file couldn't be cached");
        }
        Ok(response.body)
    }

    /// `<data>/plugins/<first 16 hex of the server key's SHA-256>/<plugin>`.
    fn storage_dir(&self, key: &str, plugin_id: &str) -> PathBuf {
        let hash = hex::encode(Sha256::digest(key.as_bytes()));
        self.storage_root.join(&hash[..16]).join(plugin_id)
    }

    fn emit(&self, output: ViewOutput) {
        let outputs = lock(&self.outputs);
        if let Some(sender) = outputs.as_ref()
            && sender.send(output).is_err()
        {
            tracing::debug!("view output with nobody listening");
        }
    }

    /// The only batch of a view that never ran.
    fn emit_alone(&self, id: &str, op: Patch) {
        let batch = Batch {
            view: id.to_owned(),
            seq: 1,
            ops: vec![op],
        };
        match serde_json::to_string(&batch) {
            Ok(json) => self.emit(ViewOutput {
                view_id: id.to_owned(),
                json,
            }),
            Err(error) => tracing::error!(view = %id, %error, "a view's batch couldn't be written"),
        }
    }

    fn queue(&self, id: &str, item: Outgoing) {
        // The view's sink holds the sending half; this path is only for
        // reload problems, sent as reports through the same order.
        let Some(server) = self.server_view(id) else {
            return;
        };
        let transport = self.transport.clone();
        let Outgoing::Error(text) = item else { return };
        self.runtime.spawn(async move {
            report(&*transport, &server.0, server.1, text).await;
        });
    }

    fn server_view(&self, id: &str) -> Option<(String, i64)> {
        let state = self.lock();
        let view = state.views.get(id)?;
        Some((view.key.clone(), view.server_id?))
    }

    /// Sends a view's events and reports in order, while it lives.
    async fn send_out(shared: Weak<Shared>, id: String, mut queue: mpsc::Receiver<Outgoing>) {
        while let Some(item) = queue.recv().await {
            let Some(shared) = shared.upgrade() else {
                return;
            };
            let Some((key, server_id)) = shared.server_view(&id) else {
                tracing::debug!(view = %id, "a view's event dropped while it isn't open on the server");
                continue;
            };
            match item {
                Outgoing::Event { name, data_json } => {
                    let request = Request::SendViewEvent(proto::SendViewEvent {
                        view_id: server_id,
                        name,
                        data_json,
                    });
                    if let Err(error) = shared.transport.request(&key, request).await {
                        tracing::info!(view = %id, ?error, "a view's event was refused");
                    }
                }
                Outgoing::Error(text) => report(&*shared.transport, &key, server_id, text).await,
            }
        }
    }
}

async fn report(transport: &dyn ViewTransport, key: &str, server_id: i64, text: String) {
    let request = Request::ReportViewError(proto::ReportViewError {
        view_id: server_id,
        text: super::cut(&text, VIEW_ERROR_REPORT_MAX_CHARS),
    });
    if let Err(error) = transport.request(key, request).await {
        tracing::info!(
            view = server_id,
            ?error,
            "a view's error report was refused"
        );
    }
}

/// One view's way out, called on its thread.
struct Sink {
    view_id: String,
    shared: Weak<Shared>,
    outgoing: mpsc::Sender<Outgoing>,
    last_report: Mutex<Option<Instant>>,
}

impl Sink {
    fn push(&self, item: Outgoing) {
        if self.outgoing.try_send(item).is_err() {
            tracing::warn!(view = %self.view_id, "a view's outgoing queue is full; an event was dropped");
        }
    }
}

impl ViewSink for Sink {
    fn batch(&self, json: String) {
        if let Some(shared) = self.shared.upgrade() {
            shared.emit(ViewOutput {
                view_id: self.view_id.clone(),
                json,
            });
        }
    }

    fn send(&self, name: String, data_json: String) {
        self.push(Outgoing::Event { name, data_json });
    }

    fn report_error(&self, text: String) {
        let now = Instant::now();
        {
            let mut last = lock(&self.last_report);
            if last.is_some_and(|at| now.duration_since(at) < VIEW_ERROR_REPORT_EVERY) {
                tracing::debug!(view = %self.view_id, "a view error report dropped: one went out recently");
                return;
            }
            *last = Some(now);
        }
        self.push(Outgoing::Error(text));
    }
}

/// The `client/` files of `files`, by path, with their hashes.
fn scripts_of(files: &[proto::PluginFileRef]) -> BTreeMap<String, String> {
    files
        .iter()
        .filter(|file| file.path.starts_with("client/"))
        .map(|file| (file.path.clone(), file.sha256.clone()))
        .collect()
}

fn close_reason(reason: i32) -> CloseReason {
    match proto::ViewCloseReason::try_from(reason) {
        Ok(proto::ViewCloseReason::PluginStopped) => CloseReason::PluginStopped,
        Ok(proto::ViewCloseReason::LostAccess) => CloseReason::LostAccess,
        Ok(proto::ViewCloseReason::ChannelDeleted) => CloseReason::ChannelDeleted,
        Ok(proto::ViewCloseReason::ActivityEnded) => CloseReason::ActivityEnded,
        Ok(proto::ViewCloseReason::LeftCall) => CloseReason::LeftCall,
        _ => CloseReason::Unavailable,
    }
}

/// A plugin id goes into a URL and a folder name.
fn check_id(id: &str) -> Result<(), String> {
    let fine = !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-');
    fine.then_some(())
        .ok_or_else(|| "the plugin's id isn't valid".to_owned())
}

/// A package path goes into a URL unencoded, so only plain names pass.
fn check_path(path: &str) -> Result<(), String> {
    let plain = path
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/'));
    let segments_fine = path
        .split('/')
        .all(|part| !part.is_empty() && part != "." && part != "..");
    (plain && segments_fine && path.len() <= 200)
        .then_some(())
        .ok_or_else(|| format!("{path:?} isn't a path this app loads"))
}

/// Lowercase hex SHA-256, worked out off the runtime's threads.
async fn hash_of(bytes: Vec<u8>) -> String {
    let hashed = tokio::task::spawn_blocking(move || hex::encode(Sha256::digest(&bytes))).await;
    hashed.unwrap_or_default()
}

/// Writes `bytes` next to `path`, then renames it there.
async fn write_aside(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let aside = path.with_extension(format!("part{}-{n}", std::process::id()));
    tokio::fs::write(&aside, bytes).await?;
    tokio::fs::rename(&aside, path).await
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
