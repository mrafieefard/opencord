//! Music in voice channels (Phase 3 plan Part B). Lavalink plays into the
//! audio bridge, which this server runs next to its embedded voice node:
//! Lavalink's WebSocket comes in on the main TLS port, its audio on the
//! music UDP port, and the frames go to the node as the channel's music.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::IpAddr;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use anyhow::Context as _;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::ws::rejection::WebSocketUpgradeRejection;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use opencord_proto::v1 as proto;
use opencord_voice::bridge::{self, BridgeConfig, BridgeEvent, MusicBridge};
use opencord_voice::node::VoiceNode;
use proto::event::Kind as Event;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

use crate::config::Config;
use crate::db;
use crate::error::ApiError;
use crate::lavalink::health::Health;
use crate::lavalink::{HealthChange, lock};
use crate::network::ClientAddr;
use crate::state::AppState;

pub mod artwork;
pub mod player;
pub mod queue;
pub mod search;
pub mod session;
pub mod settings;
pub mod tracks;

use session::Command;
use settings::MusicSettings;

/// Channels moved at once in a failover, before penalties are weighed
/// again (plan §12.7).
const FAILOVER_BATCH: usize = 5;

/// Lavalink's voice messages are small.
pub(crate) const BRIDGE_FRAME_BYTES: usize = 16 * 1024;

/// Starts the audio bridge when music is on and this server carries voice
/// itself. (External voice nodes run their own bridge: plan §13.1.) Its
/// events go to [`watch_bridge`] once the server's state exists.
pub async fn start_bridge(
    config: &Config,
    node: Option<&VoiceNode>,
) -> anyhow::Result<Option<(MusicBridge, mpsc::UnboundedReceiver<BridgeEvent>)>> {
    let Some(node) = node.filter(|_| config.music.enabled) else {
        return Ok(None);
    };
    let (bridge, mut frames, events) = MusicBridge::start(BridgeConfig {
        udp_port: config.music.audio_udp_port,
        public_ip: config.voice.public_address.parse::<IpAddr>().ok(),
        heartbeat: bridge::HEARTBEAT,
    })
    .await
    .with_context(|| {
        format!(
            "could not start music on UDP port {}",
            config.music.audio_udp_port
        )
    })?;
    let node = node.clone();
    tokio::spawn(async move {
        while let Some(frame) = frames.recv().await {
            node.music_frame(frame.channel_id, frame.packet);
        }
    });
    Ok(Some((bridge, events)))
}

/// Logs what the bridge reports and hands it to whoever waits on it.
pub fn watch_bridge(state: &Arc<AppState>, mut events: mpsc::UnboundedReceiver<BridgeEvent>) {
    let state = Arc::clone(state);
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            bridge_event(&state, event);
        }
    });
}

/// What a bridge reported, this server's or an external node's: logged,
/// and given to the connection test or the channel's music.
pub fn bridge_event(state: &AppState, event: BridgeEvent) {
    match &event {
        BridgeEvent::Connected {
            channel_id,
            connection,
        } => tracing::info!(channel_id, connection, "Lavalink connected for music"),
        BridgeEvent::Closed { channel_id, code } => {
            tracing::info!(channel_id, code, "Lavalink's music connection ended");
        }
        BridgeEvent::Discovered { channel_id, from } => {
            tracing::debug!(channel_id, %from, "Lavalink's audio address is known");
        }
        BridgeEvent::Refused { channel_id, peer } => tracing::warn!(
            channel_id,
            %peer,
            "Lavalink signed in from an address its node does not have"
        ),
        BridgeEvent::Audio { channel_id } => {
            tracing::debug!(channel_id, "music audio flows");
        }
        BridgeEvent::Silent { channel_id } => {
            tracing::debug!(channel_id, "music audio stopped");
        }
    }
    state
        .music
        .send_existing(event.channel_id(), Command::Bridge(event.clone()));
    state.lavalink.bridge_event(event);
}

#[derive(Deserialize)]
pub struct BridgeQuery {
    /// The voice gateway version Lavalink asks for.
    v: Option<u8>,
}

/// `wss://<domain>:<port>/?v=8`: Lavalink's voice connection, to the bridge
/// (plan §13.2). Everything else on `/` is not found.
pub async fn upgrade(
    State(state): State<Arc<AppState>>,
    ClientAddr(peer): ClientAddr,
    Query(query): Query<BridgeQuery>,
    upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Response {
    let (Some(bridge), Some(4 | 8), Ok(upgrade)) = (state.music_bridge.clone(), query.v, upgrade)
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    upgrade
        .max_message_size(BRIDGE_FRAME_BYTES)
        .max_frame_size(BRIDGE_FRAME_BYTES)
        .on_upgrade(move |socket| async move { bridge.serve(socket, peer).await })
}

/// Every channel's music, the music settings and who is blocked from
/// music.
#[derive(Debug, Default)]
pub struct Music {
    settings: RwLock<MusicSettings>,
    blocks: Mutex<BTreeMap<i64, db::music::BlockRow>>,
    sessions: Mutex<HashMap<i64, mpsc::UnboundedSender<Command>>>,
    /// Sessions watching a channel's player without being in it.
    subscribers: Mutex<HashMap<i64, HashSet<String>>>,
    /// After a restart: the node each channel's music was on, kept until
    /// the channel is back on a node, so that node's players stay.
    preferred: Mutex<HashMap<i64, i64>>,
    /// Search results handed out.
    pub results: search::Results,
    /// Artwork ids handed out, and the pictures kept.
    pub artwork: artwork::Artwork,
}

impl Music {
    pub fn settings(&self) -> MusicSettings {
        self.settings
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub fn set_settings(&self, settings: MusicSettings) {
        *self
            .settings
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = settings;
    }

    pub fn is_blocked(&self, user_id: i64) -> bool {
        lock(&self.blocks).contains_key(&user_id)
    }

    pub fn blocks(&self) -> proto::MusicBlocks {
        proto::MusicBlocks {
            blocks: lock(&self.blocks)
                .values()
                .map(|row| proto::MusicBlock {
                    user_id: row.user_id,
                    reason: row.reason.clone(),
                    blocked_by: row.blocked_by,
                    created_at_ms: row.created_at,
                })
                .collect(),
        }
    }

    pub fn set_block(&self, user_id: i64, row: Option<db::music::BlockRow>) {
        let mut blocks = lock(&self.blocks);
        match row {
            Some(row) => blocks.insert(user_id, row),
            None => blocks.remove(&user_id),
        };
    }

    /// Sends to a channel's session, starting one if it has none.
    pub fn send(&self, state: &Arc<AppState>, channel_id: i64, command: Command) {
        let mut sessions = lock(&self.sessions);
        let sender = sessions
            .entry(channel_id)
            .or_insert_with(|| session::spawn(state, channel_id, None));
        if let Err(mpsc::error::SendError(command)) = sender.send(command) {
            // Its actor ended between two commands: a new one takes it.
            let sender = session::spawn(state, channel_id, None);
            let _ = sender.send(command);
            sessions.insert(channel_id, sender);
        }
    }

    /// Sends to a channel's session, if it has one.
    fn send_existing(&self, channel_id: i64, command: Command) {
        if let Some(sender) = lock(&self.sessions).get(&channel_id) {
            let _ = sender.send(command);
        }
    }

    /// Asks a channel's session and waits for its answer.
    pub async fn ask<T>(
        &self,
        state: &Arc<AppState>,
        channel_id: i64,
        command: impl FnOnce(oneshot::Sender<T>) -> Command,
    ) -> Result<T, ApiError> {
        let (reply, answer) = oneshot::channel();
        self.send(state, channel_id, command(reply));
        answer.await.map_err(|_| ApiError::internal())
    }

    /// A session ends when nothing else was sent to it meanwhile; it is
    /// sent nothing more after this says yes.
    pub(crate) fn end(&self, channel_id: i64, commands: &mpsc::UnboundedReceiver<Command>) -> bool {
        let mut sessions = lock(&self.sessions);
        if !commands.is_empty() {
            return false;
        }
        sessions.remove(&channel_id);
        true
    }

    /// Every session saves now; before the server stops.
    pub async fn save_all(&self) {
        let answers: Vec<_> = lock(&self.sessions)
            .values()
            .map(|sender| {
                let (done, answer) = oneshot::channel();
                let _ = sender.send(Command::Save { done });
                answer
            })
            .collect();
        for answer in answers {
            let _ = answer.await;
        }
    }

    fn channels(&self) -> Vec<i64> {
        lock(&self.sessions).keys().copied().collect()
    }

    /// Starts or stops snapshots of a channel's player for a session.
    pub fn subscribe(&self, channel_id: i64, session_id: &str, subscribe: bool) {
        let mut subscribers = lock(&self.subscribers);
        let watching = subscribers.entry(channel_id).or_default();
        if subscribe {
            watching.insert(session_id.to_owned());
        } else {
            watching.remove(session_id);
        }
        if watching.is_empty() {
            subscribers.remove(&channel_id);
        }
    }

    /// To a channel's subscribers: members connected to it who can see it,
    /// and sessions that subscribed.
    pub fn publish(&self, state: &AppState, channel_id: i64, kind: Event) {
        let event = proto::Event { kind: Some(kind) };
        let watching = lock(&self.subscribers)
            .get(&channel_id)
            .cloned()
            .unwrap_or_default();
        let now = Instant::now();
        let guild = state.guild();
        let in_voice: HashSet<i64> = state
            .voice()
            .in_channel(channel_id)
            .map(|voice| voice.user_id)
            .collect();
        let mut gone = Vec::new();
        for session in state.sessions.all() {
            let subscribed = watching.contains(&session.id);
            if subscribed && session.is_ended() {
                gone.push(session.id.clone());
                continue;
            }
            let receives = (subscribed || in_voice.contains(&session.user_id))
                && guild.can_view(session.user_id, channel_id);
            if receives {
                session.push_event(&event, now);
            }
        }
        drop(guild);
        for id in gone {
            self.subscribe(channel_id, &id, false);
        }
    }

    fn forget_preferred(&self, channel_id: i64) {
        lock(&self.preferred).remove(&channel_id);
    }

    /// Whether a node's player for `channel_id` is wanted: the channel is
    /// on that node, or comes back to it after a restart.
    fn wanted_on(&self, state: &AppState, node_id: i64, channel_id: i64) -> bool {
        lock(&self.preferred).get(&channel_id) == Some(&node_id)
            || state
                .lavalink
                .node_of(channel_id)
                .is_some_and(|node| node.id == node_id)
    }
}

/// Who a member is in notices.
pub fn display_name(state: &AppState, user_id: i64) -> String {
    state
        .guild()
        .members
        .get(&user_id)
        .map(|member| {
            member
                .nickname
                .clone()
                .unwrap_or_else(|| member.user.display_name.clone())
        })
        .unwrap_or_else(|| "Someone".to_owned())
}

/// Whether this server runs music: on in `opencord.toml`, with a bridge
/// of its own or voice on external nodes (whose bridges take the music,
/// plan §13.1).
pub fn supported(state: &AppState) -> bool {
    state.config.music.enabled
        && (state.music_bridge.is_some()
            || state.config.voice.mode == crate::config::VoiceMode::External)
}

/// Whether music can play here: supported, on in the settings, with a
/// node added.
pub fn configured(state: &AppState) -> bool {
    supported(state) && state.music.settings().enabled && !state.lavalink.nodes().is_empty()
}

/// The settings with what is available now: search sources at least one
/// connected node offers, whether this server runs music at all, and
/// whether it is set up.
pub fn settings_proto(state: &AppState) -> proto::MusicSettings {
    let nodes = state.lavalink.nodes();
    let available = tracks::SEARCH_SOURCES
        .iter()
        .filter(|(_, _, manager)| {
            nodes
                .iter()
                .any(|node| node.health().is_up() && node.has_source(manager))
        })
        .map(|(key, _, _)| (*key).to_owned())
        .collect();
    state
        .music
        .settings()
        .to_proto(available, supported(state), !nodes.is_empty())
}

/// Tells everyone the settings as they are now: they changed, or a node
/// was added or removed. Returns them.
pub fn announce_settings(state: &AppState) -> proto::MusicSettings {
    let settings = settings_proto(state);
    state.broadcast(
        Event::MusicSettingsUpdate(Box::new(proto::MusicSettingsUpdate {
            settings: Some(settings.clone()),
        })),
        crate::state::Audience::Everyone,
    );
    // Music commands come and go with music being set up.
    crate::commands::announce(state);
    settings
}

/// Loads the music settings, the blocked members and the saved sessions,
/// and follows the nodes' health for failover. Runs before the nodes
/// connect, so channels find their old nodes again.
pub async fn start(state: &Arc<AppState>) -> anyhow::Result<()> {
    let mut conn = state.db.acquire().await?;
    let settings = MusicSettings::load(&mut conn, &state.config.music.bridge_domain).await?;
    state.music.set_settings(settings);
    for row in db::music::blocks(&mut conn).await? {
        state.music.set_block(row.user_id, Some(row));
    }
    let saved = db::music::sessions(&mut conn).await?;
    drop(conn);
    if supported(state) {
        let mut sessions = lock(&state.music.sessions);
        for row in saved {
            let Ok(saved) = serde_json::from_str::<session::Saved>(&row.state_json) else {
                tracing::warn!(
                    channel_id = row.channel_id,
                    "a saved music session could not be read"
                );
                continue;
            };
            if let Some(node_id) = saved.node_id {
                lock(&state.music.preferred).insert(row.channel_id, node_id);
            }
            sessions.insert(
                row.channel_id,
                session::spawn(state, row.channel_id, Some(saved)),
            );
        }
    }
    if let Some(mut changes) = state.lavalink.health_changes() {
        let state = Arc::clone(state);
        tokio::spawn(async move {
            while let Some(change) = changes.recv().await {
                health_changed(&state, change);
            }
        });
    }
    Ok(())
}

fn health_changed(state: &Arc<AppState>, change: HealthChange) {
    let was_up = change.before.is_some_and(Health::is_up);
    match change.after {
        Health::Unavailable | Health::Disabled => {
            let state = Arc::clone(state);
            tokio::spawn(async move {
                move_players(&state, change.node_id, None).await;
            });
        }
        Health::Healthy | Health::Degraded if !was_up => {
            let state = Arc::clone(state);
            tokio::spawn(async move { node_up(&state, change.node_id).await });
        }
        _ => {}
    }
}

/// Moves every channel off a node (plan §12.7): to `to`, or each to the
/// best other node, 5 at a time so the penalties are weighed again
/// between batches. Returns how many moved.
pub async fn move_players(state: &Arc<AppState>, from: i64, to: Option<i64>) -> usize {
    let channels = state
        .lavalink
        .node(from)
        .map(|node| node.channels())
        .unwrap_or_default();
    let mut moved: BTreeMap<i64, usize> = BTreeMap::new();
    for batch in channels.chunks(FAILOVER_BATCH) {
        let mut answers = Vec::new();
        for channel_id in batch {
            let (done, answer) = oneshot::channel();
            state
                .music
                .send_existing(*channel_id, Command::Relocate { from, to, done });
            answers.push(answer);
        }
        for answer in answers {
            if let Ok(Some(node_id)) = answer.await {
                *moved.entry(node_id).or_default() += 1;
            }
        }
    }
    for (node_id, count) in &moved {
        state.lavalink.moved(state, from, *node_id, *count);
    }
    moved.values().sum()
}

/// A node is up: players on it that no channel wants are destroyed, so
/// nothing plays twice (§12.7), and channels waiting for a node try again.
async fn node_up(state: &Arc<AppState>, node_id: i64) {
    if let Some((rest, session)) = state
        .lavalink
        .node(node_id)
        .and_then(|node| node.rest(state))
        && let Ok(guilds) = rest.player_guilds(&session).await
    {
        for channel_id in guilds {
            if !state.music.wanted_on(state, node_id, channel_id) {
                tracing::info!(node_id, channel_id, "an old Lavalink player was destroyed");
                let _ = rest.destroy_player(&session, channel_id).await;
            }
        }
    }
    for channel_id in state.music.channels() {
        state.music.send_existing(channel_id, Command::NodeUp);
    }
}

/// A player's event or position, from Lavalink's WebSocket.
pub fn lavalink_event(state: &AppState, node_id: i64, guild_id: i64, value: Value) {
    state
        .music
        .send_existing(guild_id, Command::Lavalink { node_id, value });
}

/// Someone joined, left or moved between voice channels.
pub fn voice_changed(state: &AppState, channels: &[i64]) {
    for channel_id in channels {
        state.music.send_existing(*channel_id, Command::Listeners);
    }
}

/// Music was turned off, or a channel deleted: its music ends.
pub fn end(state: &AppState, channel_id: Option<i64>) {
    let channels = channel_id.map_or_else(|| state.music.channels(), |id| vec![id]);
    for channel_id in channels {
        state.music.send_existing(channel_id, Command::End);
    }
}
