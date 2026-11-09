//! Plugins (design 2026-11-07): server Lua scripts admins install, each
//! acting through a plugin user and a managed role.

pub mod api;
pub mod channel_types;
pub mod command_calls;
pub mod cpu_share;
pub mod database;
pub mod dev;
pub mod event_tables;
pub mod events;
pub mod github;
pub mod http;
pub mod identity;
pub mod install;
pub mod logbook;
pub mod net;
pub mod permissions;
pub mod queue;
pub mod review;
pub mod runtime;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::Duration;

use opencord_common::permissions::Permissions;
use opencord_plugins::manifest::{ConfigKind, Manifest};
use opencord_plugins::package::Package;
use opencord_plugins::signing::PublicKey;
use opencord_proto::v1 as proto;
use tokio::sync::{oneshot, watch};
use tokio::task::JoinHandle;

use crate::db;
use crate::error::ApiError;
use crate::state::{AppState, Audience};
use logbook::PluginLog;
use permissions::{PluginPermission, may_grant};
use queue::EventQueue;
use runtime::{Clock, Spec, SystemClock};

/// How long `Plugins::start` waits for the scripts and `start`, in real time.
const START_WAIT: Duration = Duration::from_secs(30);

/// What starting a plugin needs.
pub struct InstalledPlugin {
    pub id: String,
    pub user_id: i64,
    pub folder: PathBuf,
    pub manifest: Manifest,
    pub package: Package,
    pub granted: HashSet<PluginPermission>,
    /// When it was first installed, in ms: lists show plugins in this order.
    pub installed_at: i64,
}

/// An event for a plugin's handlers. Ids in the payload are strings.
pub struct PluginEvent {
    pub name: String,
    pub payload: serde_json::Value,
    /// The first `reply(...)` a handler makes, as a JSON array of its
    /// arguments. Nobody replying drops it, or leaves it to time out.
    pub reply: Option<oneshot::Sender<serde_json::Value>>,
    /// Where a command's answer goes, for the `command` event.
    pub command: Option<oneshot::Sender<command_calls::CommandAnswer>>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PluginStats {
    /// CPU time over the last 5 s, in thousandths of one core.
    pub cpu_permille: u32,
    pub memory_bytes: u64,
}

/// Why a plugin didn't start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartError(pub String);

impl fmt::Display for StartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for StartError {}

#[derive(Debug, Clone)]
pub enum ConfigValue {
    Text(String),
    Secret(String),
    Number(f64),
    Boolean(bool),
    Role(String),
    Channel(String),
    User(String),
}

#[derive(Default)]
pub struct PluginBus {
    listeners: Mutex<HashMap<(String, String), HashSet<String>>>,
}

impl PluginBus {
    pub fn listen(&self, receiver_id: &str, sender_id: &str, name: &str) {
        let mut listeners = self
            .listeners
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        listeners
            .entry((sender_id.to_owned(), name.to_owned()))
            .or_default()
            .insert(receiver_id.to_owned());
    }

    pub fn unlisten_all(&self, receiver_id: &str) {
        let mut listeners = self
            .listeners
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        listeners.retain(|_, set| {
            set.remove(receiver_id);
            !set.is_empty()
        });
    }

    pub fn emit(&self, plugins: &Plugins, sender_id: &str, name: &str, data: serde_json::Value) {
        let receivers: Vec<String> = {
            let listeners = self
                .listeners
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            listeners
                .get(&(sender_id.to_owned(), name.to_owned()))
                .map(|set| set.iter().cloned().collect())
                .unwrap_or_default()
        };

        for receiver_id in receivers {
            if let Some(handle) = plugins.get(&receiver_id)
                && handle.takes_events()
                && handle.holds(PluginPermission::TalkToPlugins)
            {
                handle.deliver(PluginEvent {
                    name: "plugin_bus".into(),
                    payload: serde_json::json!({
                        "sender": sender_id,
                        "name": name,
                        "data": data,
                    }),
                    reply: None,
                    command: None,
                });
            }
        }
    }
}

/// One plugin as the runtime keeps it, across its VM's restarts.
pub struct PluginHandle {
    pub id: String,
    pub user_id: i64,
    pub installed_at: i64,
    pub(crate) queue: Arc<EventQueue>,
    log: Arc<PluginLog>,
    granted: Arc<RwLock<HashSet<PluginPermission>>>,
    status: Mutex<Status>,
    cpu_permille: AtomicU32,
    memory_bytes: AtomicU64,
    stopping: watch::Sender<bool>,
    supervisor: tokio::sync::Mutex<Option<JoinHandle<()>>>,
    pub(crate) hooks: Arc<RwLock<HashSet<String>>>,
    pub(crate) timers: Arc<Mutex<HashMap<u64, tokio::task::JoinHandle<()>>>>,
    pub(crate) next_timer_id: Arc<AtomicU64>,
    pub(crate) config_values: Arc<RwLock<HashMap<String, ConfigValue>>>,
    pub(crate) files_lock: Arc<tokio::sync::Mutex<()>>,
    pub(crate) files_reads: Arc<tokio::sync::Semaphore>,
}

struct Status {
    state: proto::PluginState,
    reason: Option<String>,
}

impl PluginHandle {
    pub fn state(&self) -> proto::PluginState {
        self.status().state
    }

    pub fn status_reason(&self) -> Option<String> {
        self.status().reason.clone()
    }

    pub fn granted(&self) -> HashSet<PluginPermission> {
        self.granted
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Whether the plugin holds `permission` now.
    pub fn holds(&self, permission: PluginPermission) -> bool {
        self.granted
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(&permission)
    }

    /// Changes what the plugin may do, and tells it with
    /// `permission_change { granted, added, removed }`.
    pub fn set_granted(&self, granted: HashSet<PluginPermission>) {
        let old = std::mem::replace(
            &mut *self.granted.write().unwrap_or_else(PoisonError::into_inner),
            granted.clone(),
        );
        let names = |set: &mut dyn Iterator<Item = &PluginPermission>| {
            let mut names: Vec<&str> = set.map(|permission| permission.name()).collect();
            names.sort_unstable();
            names
        };
        let payload = serde_json::json!({
            "granted": names(&mut granted.iter()),
            "added": names(&mut granted.difference(&old)),
            "removed": names(&mut old.difference(&granted)),
        });
        self.deliver(PluginEvent {
            name: "permission_change".to_owned(),
            payload,
            reply: None,
            command: None,
        });
    }

    pub fn stats(&self) -> PluginStats {
        PluginStats {
            cpu_permille: self.cpu_permille.load(Ordering::Relaxed),
            memory_bytes: self.memory_bytes.load(Ordering::Relaxed),
        }
    }

    pub fn log(&self) -> &PluginLog {
        &self.log
    }

    /// Queues the event while the plugin is starting or running; drops it
    /// otherwise.
    pub fn deliver(&self, event: PluginEvent) {
        if self.takes_events() {
            self.queue.push(event);
        }
    }

    fn takes_events(&self) -> bool {
        matches!(
            self.state(),
            proto::PluginState::Starting | proto::PluginState::Running
        )
    }

    fn status(&self) -> std::sync::MutexGuard<'_, Status> {
        self.status.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn set_status(&self, state: proto::PluginState, reason: Option<String>) {
        *self.status() = Status { state, reason };
    }

    pub fn has_hook(&self, path: &str) -> bool {
        self.hooks
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(path)
    }

    pub fn clear_hooks(&self) {
        self.hooks
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    pub fn clear_timers(&self) {
        let mut timers = self.timers.lock().unwrap_or_else(PoisonError::into_inner);
        for handle in timers.values() {
            handle.abort();
        }
        timers.clear();
    }

    pub fn update_config(&self, values: HashMap<String, ConfigValue>) {
        let mut conf = self
            .config_values
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        *conf = values;
    }

    /// Delivers `stop`, waits the grace, ends the VM. A crashed plugin
    /// stays crashed.
    async fn stop(&self) {
        self.stopping.send_replace(true);
        let supervisor = self.supervisor.lock().await.take();
        if let Some(supervisor) = supervisor
            && let Err(error) = supervisor.await
        {
            tracing::error!(plugin = %self.id, %error, "a plugin's supervisor failed");
        }
        {
            let mut status = self.status();
            if matches!(
                status.state,
                proto::PluginState::Starting | proto::PluginState::Running
            ) {
                *status = Status {
                    state: proto::PluginState::Stopped,
                    reason: None,
                };
            }
        }
        self.clear_timers();
        self.clear_hooks();
        self.queue.clear();
    }
}

/// The plugins the runtime knows. An entry is added by [`Plugins::start`]
/// and leaves with [`Plugins::remove`] (uninstalling); a stopped or crashed
/// plugin keeps its entry, so its state, reason and log stay readable.
pub struct Plugins {
    handles: RwLock<HashMap<String, Arc<PluginHandle>>>,
    clock: Arc<dyn Clock>,
    /// Uploads waiting for an admin.
    pub reviews: review::Reviews,
    pub dev_tokens: dev::DevTokens,
    pub bus: Arc<PluginBus>,
    /// The channel types running plugins registered.
    pub channel_types: channel_types::ChannelTypes,
    log_watchers: RwLock<HashMap<String, HashSet<String>>>,
    /// Held through each install, update, uninstall and permission change,
    /// so two never interleave. Taken before the guild's write lock.
    pub(crate) changes: tokio::sync::Mutex<()>,
}

impl Default for Plugins {
    fn default() -> Self {
        Self::with_clock(Arc::new(SystemClock))
    }
}

impl fmt::Debug for Plugins {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Plugins").field("ids", &self.ids()).finish()
    }
}

impl Plugins {
    pub(crate) fn with_clock(clock: Arc<dyn Clock>) -> Self {
        Self {
            handles: RwLock::default(),
            clock,
            reviews: review::Reviews::default(),
            dev_tokens: dev::DevTokens::default(),
            bus: Arc::new(PluginBus::default()),
            channel_types: channel_types::ChannelTypes::default(),
            log_watchers: RwLock::new(HashMap::new()),
            changes: tokio::sync::Mutex::new(()),
        }
    }

    pub fn get(&self, id: &str) -> Option<Arc<PluginHandle>> {
        self.handles
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .cloned()
    }

    pub fn ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .handles
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .keys()
            .cloned()
            .collect();
        ids.sort_unstable();
        ids
    }

    fn all(&self) -> Vec<Arc<PluginHandle>> {
        self.handles
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
            .cloned()
            .collect()
    }

    /// Starts the plugin, stopping a running copy first. Resolves once its
    /// scripts ran and `start` was handled, or with what made start fail;
    /// either way the plugin's handle stays, with its state and reason.
    pub async fn start(
        &self,
        state: &Arc<AppState>,
        installed: InstalledPlugin,
    ) -> Result<(), StartError> {
        if !state.config.plugins.enabled {
            return Err(StartError(
                "plugins are turned off on this server".to_owned(),
            ));
        }
        let data_dir = state.config.plugins_dir().join(&installed.id);
        let spec = Arc::new(Spec::new(&installed, &state.config.plugins, data_dir)?);
        let id = installed.id.clone();
        let started = self.replace(state, installed, spec).await;
        if started.is_err() {
            // The old copy stopped and the new one didn't start.
            channel_types::stopped(state, &id).await;
        }
        started
    }

    /// Stops a running copy, then starts `installed` as [`Plugins::start`]
    /// says.
    async fn replace(
        &self,
        state: &Arc<AppState>,
        installed: InstalledPlugin,
        spec: Arc<Spec>,
    ) -> Result<(), StartError> {
        let old = self.get(&installed.id);
        if let Some(old) = &old {
            old.stop().await;
        }
        // Starting it again clears why it was turned off.
        save_crashed_reason(state, &installed.id, None)
            .await
            .map_err(|error| {
                tracing::warn!(plugin = %installed.id, %error, "could not clear a plugin's crash");
                StartError("the server couldn't save the plugin's state".to_owned())
            })?;
        let (log, is_new_log) = match &old {
            Some(old) => (old.log.clone(), false),
            None => (
                PluginLog::new(
                    &installed.id,
                    Some(state.config.plugins_dir().join(&installed.id)),
                ),
                true,
            ),
        };
        let (files_lock, files_reads) = match &old {
            Some(old) => (old.files_lock.clone(), old.files_reads.clone()),
            None => (
                Arc::new(tokio::sync::Mutex::new(())),
                Arc::new(tokio::sync::Semaphore::new(10)),
            ),
        };
        let mut conn = state
            .db
            .acquire()
            .await
            .map_err(|e| StartError(e.to_string()))?;
        let config_rows = db::plugins::config_values(&mut conn, &installed.id)
            .await
            .map_err(|e| StartError(e.to_string()))?;
        drop(conn);

        let config_map = install::build_config_values(
            state,
            &installed.id,
            &installed.manifest.config,
            &config_rows,
        );

        let (stopping, stopping_receiver) = watch::channel(false);
        let handle = Arc::new(PluginHandle {
            id: installed.id.clone(),
            user_id: installed.user_id,
            installed_at: installed.installed_at,
            queue: Arc::new(EventQueue::default()),
            log: log.clone(),
            granted: Arc::new(RwLock::new(installed.granted)),
            status: Mutex::new(Status {
                state: proto::PluginState::Starting,
                reason: None,
            }),
            cpu_permille: AtomicU32::new(0),
            memory_bytes: AtomicU64::new(0),
            stopping,
            supervisor: tokio::sync::Mutex::new(None),
            hooks: Arc::new(RwLock::new(HashSet::new())),
            timers: Arc::new(Mutex::new(HashMap::new())),
            next_timer_id: Arc::new(AtomicU64::new(1)),
            config_values: Arc::new(RwLock::new(config_map)),
            files_lock,
            files_reads,
        });

        if is_new_log {
            let (plugin_id_c, state_weak) = (installed.id.clone(), Arc::downgrade(state));
            tokio::spawn(logbook::forward(log.subscribe(), move |line| {
                let Some(state) = state_weak.upgrade() else {
                    return false;
                };
                state
                    .plugins
                    .dispatch_log_append(&state, &plugin_id_c, line);
                true
            }));
        }
        self.handles
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(installed.id, handle.clone());
        let (started, started_receiver) = oneshot::channel();
        let supervisor = tokio::spawn(runtime::supervise(
            Arc::downgrade(state),
            handle.clone(),
            spec,
            self.clock.clone(),
            stopping_receiver,
            started,
        ));
        *handle.supervisor.lock().await = Some(supervisor);
        match tokio::time::timeout(START_WAIT, started_receiver).await {
            Ok(Ok(Ok(()))) => Ok(()),
            Ok(Ok(Err(error))) => Err(StartError(error)),
            Ok(Err(_)) => Err(StartError(
                "the plugin was stopped while starting".to_owned(),
            )),
            Err(_) => {
                handle.stop().await;
                let error = format!("starting took longer than {} s", START_WAIT.as_secs());
                handle.log.push(proto::PluginLogLevel::Error, &error);
                handle.set_status(proto::PluginState::Stopped, Some(error.clone()));
                Err(StartError(error))
            }
        }
    }

    /// Delivers `stop`, waits the grace, drops the VM; waiting calls are
    /// cancelled. Its channels can't be opened after.
    pub async fn stop(&self, state: &AppState, id: &str) {
        if let Some(handle) = self.get(id) {
            handle.stop().await;
        }
        channel_types::stopped(state, id).await;
    }

    /// Stops the plugin and forgets it, writing out its log first.
    pub async fn remove(&self, state: &AppState, id: &str) {
        let Some(handle) = self.get(id) else { return };
        handle.stop().await;
        channel_types::stopped(state, id).await;
        self.clear_log_watchers(id);
        self.bus.unlisten_all(id);
        self.handles
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(id);
        handle.log.close().await;
    }

    pub fn watch_log(&self, plugin_id: &str, session_id: &str, watch: bool) {
        let mut watchers = self
            .log_watchers
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        if watch {
            watchers
                .entry(plugin_id.to_owned())
                .or_default()
                .insert(session_id.to_owned());
        } else if let Some(set) = watchers.get_mut(plugin_id) {
            set.remove(session_id);
        }
    }

    pub fn clear_log_watchers(&self, plugin_id: &str) {
        let mut watchers = self
            .log_watchers
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        watchers.remove(plugin_id);
    }

    pub fn dispatch_log_append(
        &self,
        state: &AppState,
        plugin_id: &str,
        line: proto::PluginLogLine,
    ) {
        let watchers: Vec<String> = {
            let watchers = self
                .log_watchers
                .read()
                .unwrap_or_else(PoisonError::into_inner);
            watchers
                .get(plugin_id)
                .map(|s| s.iter().cloned().collect())
                .unwrap_or_default()
        };
        if watchers.is_empty() {
            return;
        }

        let event = proto::Event {
            kind: Some(proto::event::Kind::PluginLogAppend(
                proto::PluginLogAppend {
                    plugin_id: plugin_id.to_owned(),
                    lines: vec![line],
                },
            )),
        };
        let now = std::time::Instant::now();
        let mut gone = Vec::new();
        let guild = state.guild();
        for id in watchers {
            match state.sessions.get(&id) {
                Some(session) if !session.is_ended() => {
                    let has_perm = guild
                        .base_permissions(session.user_id)
                        .contains(opencord_common::permissions::Permissions::MANAGE_PLUGINS);
                    if has_perm {
                        session.push_event(&event, now);
                    } else {
                        gone.push(id);
                    }
                }
                _ => gone.push(id),
            }
        }
        if !gone.is_empty() {
            let mut watchers = self
                .log_watchers
                .write()
                .unwrap_or_else(PoisonError::into_inner);
            if let Some(set) = watchers.get_mut(plugin_id) {
                for id in gone {
                    set.remove(&id);
                }
            }
        }
    }

    pub async fn stop_all(&self) {
        let handles = self.all();
        futures_util::future::join_all(handles.iter().map(|handle| handle.stop())).await;
    }

    /// The users of the plugins taking events: whose visible channels
    /// `Visibility` follows alongside members'.
    pub fn listening_user_ids(&self) -> Vec<i64> {
        self.all()
            .iter()
            .filter(|handle| handle.takes_events())
            .map(|handle| handle.user_id)
            .collect()
    }

    pub fn deliver(&self, id: &str, event: PluginEvent) {
        if let Some(handle) = self.get(id) {
            handle.deliver(event);
        }
    }

    /// Offers each starting or running plugin an event built by `f`.
    /// Never waits.
    pub fn deliver_all(&self, f: impl Fn(&PluginHandle) -> Option<PluginEvent>) {
        for handle in self.all() {
            if handle.takes_events()
                && let Some(event) = f(&handle)
            {
                handle.queue.push(event);
            }
        }
    }
}

/// Starts every plugin an admin left running and that wasn't turned off
/// after failing, each in the background.
pub async fn start_all(state: &Arc<AppState>) {
    if !state.config.plugins.enabled {
        return;
    }
    let rows = match state.db.acquire().await {
        Ok(mut conn) => db::plugins::plugins(&mut conn).await,
        Err(error) => Err(error),
    };
    let rows = match rows {
        Ok(rows) => rows,
        Err(error) => {
            return tracing::warn!(%error, "could not read the installed plugins");
        }
    };
    for row in rows {
        if !row.running || row.crashed_reason.is_some() {
            continue;
        }
        let state = state.clone();
        tokio::spawn(async move {
            let id = row.id.clone();
            match installed(&state, row).await {
                Ok(installed) => {
                    if let Err(error) = state.plugins.start(&state, installed).await {
                        tracing::warn!(plugin = %id, %error, "a plugin failed to start");
                    }
                }
                Err(error) => tracing::warn!(plugin = %id, %error, "could not load a plugin"),
            }
        });
    }
}

/// An installed plugin as its row and folder hold it.
async fn installed(
    state: &AppState,
    row: db::plugins::PluginRow,
) -> Result<InstalledPlugin, String> {
    let manifest: Manifest = serde_json::from_str(&row.manifest)
        .map_err(|error| format!("its saved manifest is unreadable: {error}"))?;
    let granted = match state.db.acquire().await {
        Ok(mut conn) => db::plugins::granted(&mut conn, &row.id).await,
        Err(error) => Err(error),
    }
    .map_err(|error| error.to_string())?
    .iter()
    .filter_map(|name| PluginPermission::from_name(name))
    .collect();
    let folder = state.config.plugins_dir().join(&row.id).join(&row.folder);
    let reading = folder.clone();
    let package = tokio::task::spawn_blocking(move || Package::from_dir(&reading))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
    Ok(InstalledPlugin {
        id: row.id,
        user_id: row.user_id,
        folder,
        manifest,
        package,
        granted,
        installed_at: row.installed_at,
    })
}

async fn save_crashed_reason(
    state: &AppState,
    id: &str,
    reason: Option<&str>,
) -> Result<(), sqlx::Error> {
    let mut conn = state.db.acquire().await?;
    db::plugins::set_crashed_reason(&mut conn, id, reason).await
}

/// Sends the admins (`MANAGE_PLUGINS`) the plugin as it is now.
pub async fn announce(state: &AppState, id: &str) {
    match describe(state, id, None).await {
        Ok(Some(plugin)) => state.broadcast(
            proto::event::Kind::PluginUpdate(proto::PluginUpdate {
                plugin: Some(plugin),
            }),
            Audience::Holding(Permissions::MANAGE_PLUGINS),
        ),
        Ok(None) => {}
        Err(error) => {
            tracing::warn!(plugin = %id, error = %error.message, "could not describe a plugin");
        }
    }
}

/// What admins see of an installed plugin; `None` when it isn't installed.
/// Each permission's `grantable` is worked out for `viewer`, and is false
/// without one.
pub async fn describe(
    state: &AppState,
    id: &str,
    viewer: Option<i64>,
) -> Result<Option<proto::Plugin>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let Some(row) = db::plugins::plugin(&mut conn, id).await? else {
        return Ok(None);
    };
    let granted: HashSet<String> = db::plugins::granted(&mut conn, id)
        .await?
        .into_iter()
        .collect();
    let config = db::plugins::config_values(&mut conn, id).await?;
    let files_bytes = db::plugins::files_bytes(&mut conn, id).await?;
    drop(conn);
    let manifest: Manifest = serde_json::from_str(&row.manifest).map_err(|error| {
        tracing::error!(plugin = %id, %error, "a plugin's saved manifest is unreadable");
        ApiError::internal()
    })?;
    let database = state.config.plugins_dir().join(id).join("data.sqlite");
    let database_bytes = tokio::fs::metadata(database)
        .await
        .map_or(0, |metadata| metadata.len());
    let (plugin_state, status_reason, stats) = match state.plugins.get(id) {
        Some(handle) => (handle.state(), handle.status_reason(), handle.stats()),
        None => match row.crashed_reason {
            Some(reason) => (
                proto::PluginState::Crashed,
                Some(reason),
                PluginStats::default(),
            ),
            None => (proto::PluginState::Stopped, None, PluginStats::default()),
        },
    };
    let permissions = {
        let guild = state.guild();
        manifest
            .permissions
            .iter()
            .map(|name| proto::PluginPermissionGrant {
                name: name.clone(),
                granted: granted.contains(name),
                grantable: viewer
                    .zip(PluginPermission::from_name(name))
                    .is_some_and(|(viewer, permission)| may_grant(&guild, viewer, permission)),
            })
            .collect()
    };
    let signature = match row.signature.as_str() {
        "signed" => proto::PluginSignature::Signed,
        "dev" => proto::PluginSignature::Dev,
        _ => proto::PluginSignature::Unsigned,
    };
    let github_check = match row.github_check.as_str() {
        "verified" => proto::GithubCheck::Verified,
        "mismatch" => proto::GithubCheck::Mismatch,
        "failed" => proto::GithubCheck::Failed,
        _ => proto::GithubCheck::Unspecified,
    };
    let key_fingerprint = row
        .public_key
        .and_then(|key| <[u8; 32]>::try_from(key).ok())
        .map(|key| PublicKey(key).fingerprint())
        .unwrap_or_default();
    Ok(Some(proto::Plugin {
        id: row.id,
        signature: signature as i32,
        key_fingerprint,
        github_check: github_check as i32,
        state: plugin_state as i32,
        status_reason: status_reason.unwrap_or_default(),
        permissions,
        config: config
            .into_iter()
            .map(|value| proto::PluginConfigValue {
                key: value.key,
                // Secrets never leave the server.
                value: if value.sealed {
                    String::new()
                } else {
                    String::from_utf8_lossy(&value.value).into_owned()
                },
                has_value: true,
            })
            .collect(),
        user_id: row.user_id,
        role_id: row.role_id,
        installed_at_ms: row.installed_at,
        updated_at_ms: row.updated_at,
        cpu_permille: stats.cpu_permille,
        memory_bytes: stats.memory_bytes,
        database_bytes,
        files_bytes: u64::try_from(files_bytes).unwrap_or(0),
        ..manifest_plugin(&manifest)
    }))
}

/// What the manifest says of a plugin, as admins see it.
fn manifest_plugin(manifest: &Manifest) -> proto::Plugin {
    proto::Plugin {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        version: manifest.version.to_string(),
        author: manifest.author.clone(),
        description: manifest.description.clone(),
        github: manifest
            .github
            .as_ref()
            .map(|github| format!("https://github.com/{}/{}", github.owner, github.repo))
            .unwrap_or_default(),
        license: manifest.license.clone(),
        domains: manifest.domains.clone(),
        config_fields: manifest
            .config
            .iter()
            .map(|field| proto::PluginConfigField {
                key: field.key.clone(),
                label: field.label.clone(),
                kind: config_kind(field.kind) as i32,
                choices: field.choices.clone(),
                required: field.required,
            })
            .collect(),
        ..Default::default()
    }
}

fn config_kind(kind: ConfigKind) -> proto::PluginConfigKind {
    match kind {
        ConfigKind::Text => proto::PluginConfigKind::Text,
        ConfigKind::Secret => proto::PluginConfigKind::Secret,
        ConfigKind::Number => proto::PluginConfigKind::Number,
        ConfigKind::Boolean => proto::PluginConfigKind::Boolean,
        ConfigKind::Choice => proto::PluginConfigKind::Choice,
        ConfigKind::Role => proto::PluginConfigKind::Role,
        ConfigKind::Channel => proto::PluginConfigKind::Channel,
        ConfigKind::User => proto::PluginConfigKind::User,
    }
}
