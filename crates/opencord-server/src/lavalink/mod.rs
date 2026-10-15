//! Lavalink nodes (Phase 3 plan §12): any number, the admin's own or public
//! ones. Each enabled node keeps its own WebSocket and session, so a spare
//! is ready the moment it is needed; its health is judged from what it
//! reports, and the best one gets each channel.
//!
//! Passwords are sealed at rest, never sent to clients and never logged.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use opencord_common::permissions::Permissions;
use opencord_proto::v1 as proto;
use opencord_voice::bridge::BridgeEvent;
use proto::event::Kind as Event;
use tokio::sync::{Notify, mpsc};
use tokio_util::sync::CancellationToken;

pub mod bridge;
pub mod client;
pub mod health;
mod node;
mod test;

use client::Info;
use health::{Candidate, Health, Observed, Priority};

use crate::db;
use crate::error::ApiError;
use crate::handshakes::Refusals;
use crate::state::{AppState, Audience, now_ms};

pub use test::start as start_test;

/// At most this many nodes (plan §12.1).
pub const MAX_NODES: usize = 20;
/// Node events kept for the list (plan §12.8).
const EVENTS_KEPT: usize = 50;
const USER_ID_KEY: &str = "lavalink_user_id";
/// How often health is judged again.
const TICK: Duration = Duration::from_secs(1);
const NAME_MAX: usize = 64;
const PASSWORD_MAX: usize = 512;
const MAX_PLAYERS_MAX: u32 = 10_000;

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn password_purpose(id: i64) -> String {
    format!("lavalink.{id}.password")
}

/// What the admin sets for a node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeSettings {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub secure: bool,
    pub priority: Priority,
    pub max_players: u32,
    pub strict_address: bool,
    pub enabled: bool,
}

impl NodeSettings {
    /// Checks and tidies what a client sent.
    fn from_proto(settings: &proto::LavalinkNodeSettings) -> Result<Self, ApiError> {
        let invalid = |message: &str| Err(ApiError::invalid_argument(message.to_owned()));
        let name = settings.name.trim();
        if name.is_empty() || name.chars().count() > NAME_MAX {
            return invalid("a node needs a name of up to 64 characters");
        }
        let host = settings
            .host
            .trim()
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_ascii_lowercase();
        let host_ok = host.parse::<IpAddr>().is_ok()
            || (!host.is_empty()
                && host.len() <= 253
                && host
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
                && !host.starts_with(['.', '-']));
        if !host_ok {
            return invalid(
                "the host is a name like lavalink.example.com or an IP address, without http:// or a path",
            );
        }
        let Ok(port) = u16::try_from(settings.port) else {
            return invalid("the port is a number from 1 to 65535");
        };
        if port == 0 {
            return invalid("the port is a number from 1 to 65535");
        }
        if settings.max_players > MAX_PLAYERS_MAX {
            return invalid("max players is at most 10000");
        }
        Ok(Self {
            name: name.to_owned(),
            host,
            port,
            secure: settings.secure,
            priority: Priority::from_proto(settings.priority),
            max_players: settings.max_players,
            strict_address: settings.strict_address,
            enabled: settings.enabled,
        })
    }

    fn to_proto(&self) -> proto::LavalinkNodeSettings {
        proto::LavalinkNodeSettings {
            name: self.name.clone(),
            host: self.host.clone(),
            port: u32::from(self.port),
            secure: self.secure,
            priority: self.priority.to_proto() as i32,
            max_players: self.max_players,
            strict_address: self.strict_address,
            enabled: self.enabled,
        }
    }

    /// A change that needs a new connection.
    fn reaches_differently(&self, other: &Self) -> bool {
        (&self.host, self.port, self.secure) != (&other.host, other.port, other.secure)
    }
}

fn check_password(password: &str) -> Result<(), ApiError> {
    if password.is_empty()
        || password.len() > PASSWORD_MAX
        || password.chars().any(char::is_control)
    {
        return Err(ApiError::invalid_argument(
            "the password is Lavalink's server.password, up to 512 characters",
        ));
    }
    Ok(())
}

/// What a node is doing now.
#[derive(Debug, Default)]
struct Status {
    observed: Observed,
    health: Option<Health>,
    reason: String,
    /// Back from unavailable: new channels wait until then.
    recovering_until: Option<Instant>,
    info: Option<Info>,
    session_id: Option<String>,
    connected_since_ms: Option<i64>,
    /// The node's addresses, for the bridge's strict check.
    addresses: Vec<IpAddr>,
}

/// One node: its settings, its connection's status, and the Opencord
/// channels it plays for.
pub struct Node {
    pub id: i64,
    created_at: i64,
    settings: Mutex<NodeSettings>,
    password: Mutex<String>,
    draining: AtomicBool,
    status: Mutex<Status>,
    channels: Mutex<HashSet<i64>>,
    /// Ends its connection for good: disabled, deleted, or the server stops.
    stop: CancellationToken,
    /// Connect again now: where it is reached changed.
    reconnect: Notify,
}

impl std::fmt::Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Node")
            .field("id", &self.id)
            .field("settings", &*lock(&self.settings))
            .finish_non_exhaustive()
    }
}

impl Node {
    pub fn settings(&self) -> NodeSettings {
        lock(&self.settings).clone()
    }

    pub fn draining(&self) -> bool {
        self.draining.load(Ordering::Relaxed)
    }

    /// Where it is and how to sign in.
    fn endpoint(&self, user_id: i64) -> client::Endpoint {
        let settings = self.settings();
        client::Endpoint {
            host: settings.host,
            port: settings.port,
            secure: settings.secure,
            password: lock(&self.password).clone(),
            user_id,
        }
    }

    /// Its REST API and session, while connected.
    pub fn rest(&self, state: &AppState) -> Option<(client::Rest, String)> {
        let session = {
            let status = lock(&self.status);
            if !status.observed.connected {
                return None;
            }
            status.session_id.clone()?
        };
        let rest = client::Rest::new(
            state.network.http().clone(),
            self.endpoint(state.lavalink.user_id()),
        );
        Some((rest, session))
    }

    pub fn health(&self) -> Health {
        lock(&self.status).health.unwrap_or(Health::Connecting)
    }

    /// Whether the node offers a source manager (from `/v4/info`).
    pub fn has_source(&self, source: &str) -> bool {
        lock(&self.status)
            .info
            .as_ref()
            .is_some_and(|info| info.sources.iter().any(|known| known == source))
    }

    /// The channels it plays for.
    pub fn channels(&self) -> Vec<i64> {
        lock(&self.channels).iter().copied().collect()
    }

    /// A track failed to load on it (counts toward its health, §12.5).
    pub fn load_failed(&self) {
        let now = Instant::now();
        let mut status = lock(&self.status);
        let failures = &mut status.observed.load_failures;
        failures.retain(|at| now.duration_since(*at) < health::FAILURE_WINDOW);
        failures.push(now);
    }

    /// The addresses the bridge accepts it from, with the strict check.
    pub fn allowed_addresses(&self) -> Option<Vec<IpAddr>> {
        self.settings()
            .strict_address
            .then(|| lock(&self.status).addresses.clone())
    }

    fn to_row(&self, sealed: Vec<u8>) -> db::lavalink::NodeRow {
        let settings = self.settings();
        db::lavalink::NodeRow {
            id: self.id,
            name: settings.name,
            host: settings.host,
            port: i64::from(settings.port),
            secure: settings.secure,
            password_enc: sealed,
            priority: settings.priority.as_str().to_owned(),
            max_players: i64::from(settings.max_players),
            strict_address: settings.strict_address,
            enabled: settings.enabled,
            draining: self.draining(),
            resume_session: lock(&self.status).session_id.clone(),
            created_at: self.created_at,
        }
    }

    fn to_proto(&self, now: Instant) -> proto::LavalinkNode {
        let settings = self.settings();
        let status = lock(&self.status);
        let load = status.observed.load.unwrap_or_default();
        let info = status.info.clone().unwrap_or_default();
        proto::LavalinkNode {
            id: self.id,
            settings: Some(settings.to_proto()),
            draining: self.draining(),
            health: status.health.unwrap_or(Health::Connecting).to_proto() as i32,
            reason: status.reason.clone(),
            version: info.version,
            sources: info.sources,
            plugins: info.plugins,
            channels: u32::try_from(lock(&self.channels).len()).unwrap_or(u32::MAX),
            players: load.players,
            playing_players: load.playing_players,
            cpu: load.system_load as f32,
            ping_ms: status.observed.ping_ms.unwrap_or(0),
            last_error: status.observed.last_error.clone().unwrap_or_default(),
            connected_since_ms: status.connected_since_ms.unwrap_or(0),
            recovering: status.recovering_until.is_some_and(|until| until > now),
        }
    }

    fn candidate(&self, now: Instant) -> Candidate {
        let settings = self.settings();
        let status = lock(&self.status);
        Candidate {
            id: self.id,
            health: status.health.unwrap_or(Health::Connecting),
            priority: settings.priority,
            recovering: status.recovering_until.is_some_and(|until| until > now),
            channels: u32::try_from(lock(&self.channels).len()).unwrap_or(u32::MAX),
            max_players: settings.max_players,
            sources: status
                .info
                .as_ref()
                .map(|info| info.sources.clone())
                .unwrap_or_default(),
            load: status.observed.load.unwrap_or_default(),
            ping_ms: status.observed.ping_ms.unwrap_or(0),
            recent_failures: health::recent_failures(&status.observed.load_failures, now),
        }
    }
}

/// Every node, and who watches them.
#[derive(Debug, Default)]
pub struct Lavalink {
    /// The internal Lavalink user id (plan §12.3), set at start.
    user_id: AtomicI64,
    nodes: Mutex<BTreeMap<i64, Arc<Node>>>,
    /// Sessions with the node list open.
    watchers: Mutex<HashSet<String>>,
    events: Mutex<VecDeque<proto::LavalinkNodeEvent>>,
    /// Connection tests waiting on the bridge, by their channel.
    bridge_waiters: Mutex<HashMap<i64, mpsc::UnboundedSender<BridgeEvent>>>,
    refusals: Arc<Refusals>,
    /// Every node is down and the admins were told.
    all_down: AtomicBool,
    /// Where health changes go: to music, for failover.
    health_changes: std::sync::OnceLock<mpsc::UnboundedSender<HealthChange>>,
}

/// A node's health changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HealthChange {
    pub node_id: i64,
    pub before: Option<Health>,
    pub after: Health,
}

impl Lavalink {
    pub fn user_id(&self) -> i64 {
        self.user_id.load(Ordering::Relaxed)
    }

    /// TLS handshakes refused over the certificate; the main listener
    /// records them.
    pub fn refusals(&self) -> Arc<Refusals> {
        Arc::clone(&self.refusals)
    }

    pub fn node(&self, id: i64) -> Option<Arc<Node>> {
        lock(&self.nodes).get(&id).cloned()
    }

    fn all(&self) -> Vec<Arc<Node>> {
        lock(&self.nodes).values().cloned().collect()
    }

    pub fn watch(&self, session_id: &str, watching: bool) {
        let mut watchers = lock(&self.watchers);
        if watching {
            watchers.insert(session_id.to_owned());
        } else {
            watchers.remove(session_id);
        }
    }

    pub fn info(&self, state: &AppState) -> proto::LavalinkNodes {
        let now = Instant::now();
        proto::LavalinkNodes {
            nodes: self.all().iter().map(|node| node.to_proto(now)).collect(),
            bridge: Some(bridge::checklist(state)),
            events: lock(&self.events).iter().cloned().collect(),
            channels_playing: self.channels_playing(),
        }
    }

    fn channels_playing(&self) -> u32 {
        let total: usize = self
            .all()
            .iter()
            .map(|node| lock(&node.channels).len())
            .sum();
        u32::try_from(total).unwrap_or(u32::MAX)
    }

    /// Tells sessions with the list open.
    fn push(&self, state: &AppState, event: Event) {
        let watchers: Vec<String> = lock(&self.watchers).iter().cloned().collect();
        if watchers.is_empty() {
            return;
        }
        let event = proto::Event { kind: Some(event) };
        let now = Instant::now();
        let mut gone = Vec::new();
        for id in watchers {
            match state.sessions.get(&id) {
                Some(session) if !session.is_ended() => session.push_event(&event, now),
                _ => gone.push(id),
            }
        }
        let mut watchers = lock(&self.watchers);
        for id in gone {
            watchers.remove(&id);
        }
    }

    fn node_changed(&self, state: &AppState, node: &Node) {
        self.push(
            state,
            Event::LavalinkNodeStatus(Box::new(proto::LavalinkNodeStatus {
                node: Some(node.to_proto(Instant::now())),
                removed: false,
                channels_playing: self.channels_playing(),
            })),
        );
    }

    /// A line in the node events, kept and sent.
    fn record(&self, state: &AppState, node: &Node, kind: proto::LavalinkEventKind, text: String) {
        tracing::info!(node_id = node.id, "{text}");
        let event = proto::LavalinkNodeEvent {
            node_id: node.id,
            kind: kind as i32,
            text,
            at_ms: now_ms(),
        };
        {
            let mut events = lock(&self.events);
            events.push_front(event.clone());
            events.truncate(EVENTS_KEPT);
        }
        self.push(state, Event::LavalinkNodeEvent(event));
    }

    /// Judges a node again; tells watchers and records what changed.
    fn judge(&self, state: &AppState, node: &Node, now: Instant) {
        let settings = node.settings();
        let (before, after, reason, version, recovering_changed) = {
            let mut status = lock(&node.status);
            status.observed.enabled = settings.enabled;
            status.observed.draining = node.draining();
            let before = status.health;
            let (after, reason) =
                health::judge(&status.observed, before.unwrap_or(Health::Connecting), now);
            let was_recovering = status.recovering_until.is_some_and(|until| until > now);
            if before == Some(Health::Unavailable) && after.is_up() {
                status.recovering_until = Some(now + health::RECOVERY_HOLD);
            }
            let recovering = status.recovering_until.is_some_and(|until| until > now);
            let changed = before != Some(after) || status.reason != reason;
            status.health = Some(after);
            status.reason.clone_from(&reason);
            let version = status.info.as_ref().map(|info| info.version.clone());
            (
                before,
                changed.then_some(after),
                reason,
                version,
                was_recovering != recovering,
            )
        };
        let Some(after) = after else {
            if recovering_changed {
                self.node_changed(state, node);
            }
            return;
        };
        let name = &settings.name;
        use proto::LavalinkEventKind as Kind;
        let line = match (before, after) {
            (_, Health::Disabled | Health::Draining | Health::Connecting) => None,
            (Some(Health::Unavailable), Health::Healthy | Health::Degraded) => {
                Some((Kind::Recovered, format!("{name} is back")))
            }
            (None | Some(Health::Connecting), Health::Healthy) => Some((
                Kind::Connected,
                match version {
                    Some(version) => format!("{name} connected (Lavalink {version})"),
                    None => format!("{name} connected"),
                },
            )),
            (Some(Health::Degraded), Health::Healthy) => {
                Some((Kind::Recovered, format!("{name} recovered")))
            }
            (_, Health::Degraded) if before != Some(Health::Degraded) => {
                Some((Kind::Degraded, format!("{name} is struggling: {reason}")))
            }
            (_, Health::Unavailable) if before != Some(Health::Unavailable) => Some((
                Kind::Disconnected,
                format!("{name} is unavailable: {reason}"),
            )),
            _ => None,
        };
        if let Some((kind, text)) = line {
            self.record(state, node, kind, text);
        }
        self.node_changed(state, node);
        self.notice_if_all_down(state);
        if before != Some(after)
            && let Some(changes) = self.health_changes.get()
        {
            let _ = changes.send(HealthChange {
                node_id: node.id,
                before,
                after,
            });
        }
    }

    /// Health changes from now on; music takes them, once.
    pub fn health_changes(&self) -> Option<mpsc::UnboundedReceiver<HealthChange>> {
        let (sender, receiver) = mpsc::unbounded_channel();
        self.health_changes.set(sender).ok().map(|()| receiver)
    }

    /// The audio watchdog found a node silent and not answering (plan
    /// §12.5): it is unavailable now, as two failed REST calls make it.
    pub fn unresponsive(&self, state: &AppState, node_id: i64, reason: &str) {
        let Some(node) = self.node(node_id) else {
            return;
        };
        {
            let mut status = lock(&node.status);
            status.observed.rest_failures = status.observed.rest_failures.max(2);
            status.observed.last_error = Some(reason.to_owned());
        }
        self.judge(state, &node, Instant::now());
    }

    /// Records channels moved between nodes (plan §12.8).
    pub fn moved(&self, state: &AppState, from: i64, to: i64, count: usize) {
        let (Some(from), Some(to)) = (self.node(from), self.node(to)) else {
            return;
        };
        let channels = if count == 1 {
            "1 channel".to_owned()
        } else {
            format!("{count} channels")
        };
        let text = format!(
            "Moved {channels} from {} to {}",
            from.settings().name,
            to.settings().name
        );
        self.record(state, &to, proto::LavalinkEventKind::Moved, text);
    }

    /// The nodes in order of id.
    pub fn nodes(&self) -> Vec<Arc<Node>> {
        self.all()
    }

    /// Tells members who manage the server once, when every enabled node is
    /// unavailable (plan §12.8).
    fn notice_if_all_down(&self, state: &AppState) {
        let healths: Vec<Health> = self
            .all()
            .iter()
            .filter(|node| node.settings().enabled)
            .map(|node| lock(&node.status).health.unwrap_or(Health::Connecting))
            .collect();
        let down =
            !healths.is_empty() && healths.iter().all(|health| *health == Health::Unavailable);
        if down == self.all_down.swap(down, Ordering::Relaxed) || !down {
            return;
        }
        state.broadcast(
            Event::ServerNotice(proto::ServerNotice {
                key: "music.nodes".to_owned(),
                severity: proto::NoticeSeverity::Warning as i32,
                title: "Music server unavailable".to_owned(),
                body: "Every Lavalink node is unavailable, so music is paused. Opencord keeps \
                       retrying; see Server settings → Music."
                    .to_owned(),
            }),
            Audience::Holding(Permissions::MANAGE_SERVER),
        );
    }

    /// The node a new channel goes to (plan §12.6), playing `source` when
    /// given; never one in `except`.
    pub fn best_node(&self, source: Option<&str>, except: &[i64]) -> Option<Arc<Node>> {
        let now = Instant::now();
        let candidates: Vec<Candidate> = self
            .all()
            .iter()
            .filter(|node| !node.draining())
            .map(|node| node.candidate(now))
            .collect();
        health::best(&candidates, source, except).and_then(|id| self.node(id))
    }

    /// A channel now plays on `node_id` (and on no other node).
    pub fn assign(&self, state: &AppState, channel_id: i64, node_id: i64) {
        for node in self.all() {
            let changed = if node.id == node_id {
                lock(&node.channels).insert(channel_id)
            } else {
                lock(&node.channels).remove(&channel_id)
            };
            if changed {
                self.node_changed(state, &node);
            }
        }
    }

    /// A channel plays on no node now.
    pub fn release(&self, state: &AppState, channel_id: i64) {
        for node in self.all() {
            if lock(&node.channels).remove(&channel_id) {
                self.node_changed(state, &node);
            }
        }
    }

    /// The node a channel plays on.
    pub fn node_of(&self, channel_id: i64) -> Option<Arc<Node>> {
        self.all()
            .into_iter()
            .find(|node| lock(&node.channels).contains(&channel_id))
    }

    /// The bridge reported something: to the test waiting on that channel.
    pub fn bridge_event(&self, event: BridgeEvent) {
        let channel_id = event.channel_id();
        let waiter = lock(&self.bridge_waiters).get(&channel_id).cloned();
        if let Some(waiter) = waiter {
            let _ = waiter.send(event);
        }
    }

    fn wait_for_bridge(&self, channel_id: i64) -> mpsc::UnboundedReceiver<BridgeEvent> {
        let (sender, receiver) = mpsc::unbounded_channel();
        lock(&self.bridge_waiters).insert(channel_id, sender);
        receiver
    }

    fn stop_waiting_for_bridge(&self, channel_id: i64) {
        lock(&self.bridge_waiters).remove(&channel_id);
    }

    pub async fn create(
        &self,
        state: &Arc<AppState>,
        settings: &proto::LavalinkNodeSettings,
        password: &str,
    ) -> Result<(), ApiError> {
        let settings = NodeSettings::from_proto(settings)?;
        check_password(password)?;
        if lock(&self.nodes).len() >= MAX_NODES {
            return Err(ApiError::invalid_argument(
                "a server has at most 20 Lavalink nodes",
            ));
        }
        let id = state.ids.next_id();
        let node = Arc::new(Node {
            id,
            created_at: now_ms(),
            settings: Mutex::new(settings),
            password: Mutex::new(password.to_owned()),
            draining: AtomicBool::new(false),
            status: Mutex::new(Status {
                observed: Observed {
                    first_attempt: true,
                    ..Observed::default()
                },
                ..Status::default()
            }),
            channels: Mutex::new(HashSet::new()),
            stop: state.shutdown.child_token(),
            reconnect: Notify::new(),
        });
        let sealed = state
            .certificates
            .secrets()
            .seal(&password_purpose(id), password.as_bytes());
        db::lavalink::put(&mut *state.db.acquire().await?, &node.to_row(sealed)).await?;
        lock(&self.nodes).insert(id, Arc::clone(&node));
        let settings = node.settings();
        tracing::info!(
            node_id = id,
            name = settings.name,
            host = settings.host,
            port = settings.port,
            "a Lavalink node was added"
        );
        if settings.enabled {
            node::spawn(state, &node);
        }
        self.judge(state, &node, Instant::now());
        Ok(())
    }

    pub async fn update(
        &self,
        state: &Arc<AppState>,
        request: &proto::UpdateLavalinkNode,
    ) -> Result<(), ApiError> {
        let node = self
            .node(request.node_id)
            .ok_or_else(|| ApiError::not_found("Lavalink node"))?;
        let settings = request
            .settings
            .as_ref()
            .map(NodeSettings::from_proto)
            .transpose()?;
        if let Some(password) = &request.password {
            check_password(password)?;
        }
        let before = node.settings();
        let after = settings.unwrap_or_else(|| before.clone());
        let password_changed = request
            .password
            .as_ref()
            .is_some_and(|password| *password != *lock(&node.password));
        let reconnect = after.reaches_differently(&before) || password_changed;
        *lock(&node.settings) = after.clone();
        if let Some(password) = &request.password {
            lock(&node.password).clone_from(password);
        }
        if let Some(draining) = request.draining {
            node.draining.store(draining, Ordering::Relaxed);
        }
        if reconnect {
            // Another node, or other credentials: its session is not ours.
            lock(&node.status).session_id = None;
        }
        let sealed = state
            .certificates
            .secrets()
            .seal(&password_purpose(node.id), lock(&node.password).as_bytes());
        db::lavalink::put(&mut *state.db.acquire().await?, &node.to_row(sealed)).await?;
        tracing::info!(
            node_id = node.id,
            name = after.name,
            enabled = after.enabled,
            draining = node.draining(),
            reconnect,
            "a Lavalink node was changed"
        );
        let node = match (before.enabled, after.enabled) {
            (true, false) => {
                node.stop.cancel();
                self.replace_stopped(state, &node)
            }
            (false, true) => {
                let fresh = self.replace_stopped(state, &node);
                lock(&fresh.status).observed.first_attempt = true;
                node::spawn(state, &fresh);
                fresh
            }
            _ => {
                if reconnect {
                    node.reconnect.notify_one();
                }
                node
            }
        };
        self.judge(state, &node, Instant::now());
        Ok(())
    }

    /// A node whose connection ended, as a fresh one with the same
    /// settings, so it can connect again later.
    fn replace_stopped(&self, state: &AppState, node: &Arc<Node>) -> Arc<Node> {
        node.stop.cancel();
        let fresh = Arc::new(Node {
            id: node.id,
            created_at: node.created_at,
            settings: Mutex::new(node.settings()),
            password: Mutex::new(lock(&node.password).clone()),
            draining: AtomicBool::new(node.draining()),
            status: Mutex::new(Status {
                health: lock(&node.status).health,
                ..Status::default()
            }),
            channels: Mutex::new(lock(&node.channels).clone()),
            stop: state.shutdown.child_token(),
            reconnect: Notify::new(),
        });
        lock(&self.nodes).insert(node.id, Arc::clone(&fresh));
        fresh
    }

    pub async fn delete(&self, state: &Arc<AppState>, id: i64) -> Result<(), ApiError> {
        let node = lock(&self.nodes)
            .remove(&id)
            .ok_or_else(|| ApiError::not_found("Lavalink node"))?;
        node.stop.cancel();
        db::lavalink::delete(&mut *state.db.acquire().await?, id).await?;
        tracing::info!(
            node_id = id,
            name = node.settings().name,
            "a Lavalink node was deleted"
        );
        self.push(
            state,
            Event::LavalinkNodeStatus(Box::new(proto::LavalinkNodeStatus {
                node: Some(proto::LavalinkNode {
                    id,
                    ..Default::default()
                }),
                removed: true,
                channels_playing: self.channels_playing(),
            })),
        );
        self.notice_if_all_down(state);
        Ok(())
    }
}

/// Loads the nodes and the internal user id, connects every enabled node,
/// and judges their health every second until the server stops.
pub async fn start(state: &Arc<AppState>) -> anyhow::Result<()> {
    let mut conn = state.db.acquire().await?;
    let saved = db::meta::load_prefixed(&mut conn, USER_ID_KEY).await?;
    let user_id = match saved.get(USER_ID_KEY).and_then(|id| id.parse::<i64>().ok()) {
        Some(id) => id,
        None => {
            let id = state.ids.next_id();
            db::meta::put(&mut conn, USER_ID_KEY, Some(&id.to_string())).await?;
            id
        }
    };
    state.lavalink.user_id.store(user_id, Ordering::Relaxed);
    let rows = db::lavalink::list(&mut conn).await?;
    drop(conn);
    for row in rows {
        let password = match state
            .certificates
            .secrets()
            .open_string(&password_purpose(row.id), &row.password_enc)
        {
            Ok(password) => password,
            Err(error) => {
                tracing::warn!(node_id = row.id, %error, "a Lavalink node's password could not be opened");
                String::new()
            }
        };
        let settings = NodeSettings {
            name: row.name,
            host: row.host,
            port: u16::try_from(row.port).unwrap_or(2333),
            secure: row.secure,
            priority: Priority::parse(&row.priority),
            max_players: u32::try_from(row.max_players).unwrap_or(0),
            strict_address: row.strict_address,
            enabled: row.enabled,
        };
        let node = Arc::new(Node {
            id: row.id,
            created_at: row.created_at,
            settings: Mutex::new(settings),
            password: Mutex::new(password),
            draining: AtomicBool::new(row.draining),
            status: Mutex::new(Status {
                session_id: row.resume_session,
                observed: Observed {
                    first_attempt: true,
                    ..Observed::default()
                },
                ..Status::default()
            }),
            channels: Mutex::new(HashSet::new()),
            stop: state.shutdown.child_token(),
            reconnect: Notify::new(),
        });
        lock(&state.lavalink.nodes).insert(row.id, Arc::clone(&node));
        if row.enabled {
            node::spawn(state, &node);
        }
    }
    let state = Arc::clone(state);
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(TICK);
        loop {
            tokio::select! {
                _ = tick.tick() => {}
                () = state.shutdown.cancelled() => return,
            }
            let now = Instant::now();
            for node in state.lavalink.all() {
                state.lavalink.judge(&state, &node, now);
            }
        }
    });
    Ok(())
}
