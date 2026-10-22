//! A channel's Lavalink player (Phase 3 plan §12.4): one per voice channel
//! with music, on one node. Its voice state points at the audio bridge with
//! a token of its own; a new voice state revokes the old token, so late
//! audio from a node the channel left is refused (§12.7).

use std::sync::Arc;

use opencord_voice::bridge::ExpectedPlayer;
use opencord_voice::node::NodeCommand;
use serde_json::{Value, json};

use super::queue::Item;
use crate::lavalink::bridge;
use crate::lavalink::client::{Failure, Rest};
use crate::lavalink::{Node, health::Health};
use crate::state::AppState;
use crate::voice::nodes::{MusicTarget, NodeId};

/// Lavalink's player for a channel, on one node.
#[derive(Debug, Clone)]
pub struct Player {
    pub node_id: i64,
    rest: Rest,
    /// The node's Lavalink session the player lives in.
    session: String,
    /// The bridge has not been told about it yet: the next update carries
    /// a voice state.
    needs_voice: bool,
    /// The voice node whose bridge the voice state points at.
    pub bridge_at: Option<NodeId>,
}

/// What Lavalink reports a channel's player plays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playing {
    pub item_id: String,
    pub position_ms: u64,
    pub paused: bool,
}

impl Player {
    /// A player on `node`, while it is connected.
    pub fn open(state: &AppState, node: &Node) -> Option<Self> {
        let (rest, session) = node.rest(state)?;
        Some(Self {
            node_id: node.id,
            rest,
            session,
            needs_voice: true,
            bridge_at: None,
        })
    }

    /// Whether its node can still play: connected, and up.
    pub fn usable(&self, state: &AppState) -> bool {
        state.lavalink.node(self.node_id).is_some_and(|node| {
            node.health().is_up()
                && node
                    .rest(state)
                    .is_some_and(|(_, session)| session == self.session)
        })
    }

    /// Whether the channel should move at its next track change (§12.6):
    /// its node struggles or is drained.
    pub fn should_move(&self, state: &AppState) -> bool {
        state
            .lavalink
            .node(self.node_id)
            .is_none_or(|node| matches!(node.health(), Health::Degraded | Health::Draining))
    }

    /// The voice state Lavalink connects to the bridge with, at the voice
    /// node serving the channel (plan §13.1); that bridge expects it from
    /// now on, and no longer the previous one.
    fn voice(&mut self, state: &AppState, channel_id: i64) -> Option<Value> {
        let node = state.lavalink.node(self.node_id)?;
        let player = ExpectedPlayer {
            channel_id,
            user_id: state.lavalink.user_id(),
            token: crate::random::code(32),
            session_id: crate::random::code(32),
            allowed: node.allowed_addresses(),
        };
        let (endpoint, at) = match state.voice_nodes.music_target(channel_id)? {
            MusicTarget::Embedded => {
                state.music_bridge.as_ref()?.expect(player.clone());
                (bridge::address(state).endpoint(), NodeId::Embedded)
            }
            MusicTarget::External {
                id,
                bridge: Some(bridge),
            } => {
                state
                    .voice_nodes
                    .send(channel_id, NodeCommand::ExpectMusic(player.clone()));
                (bridge, NodeId::External(id))
            }
            MusicTarget::External { id, bridge: None } => {
                tracing::warn!(
                    channel_id,
                    node = id,
                    "the channel's voice node runs no audio bridge: turn on [music] there"
                );
                return None;
            }
        };
        self.bridge_at = Some(at);
        Some(json!({
            "token": player.token,
            "endpoint": endpoint,
            "sessionId": player.session_id,
            "channelId": channel_id.to_string(),
        }))
    }

    /// Whether the channel's voice node changed since the voice state was
    /// given: everyone left and came back on another node.
    pub fn bridge_moved(&self, state: &AppState, channel_id: i64) -> bool {
        self.bridge_at.is_some() && state.voice_nodes.node_of(channel_id) != self.bridge_at
    }

    /// Sends `body`, with a voice state when the bridge has none for it.
    async fn update(
        &mut self,
        state: &AppState,
        channel_id: i64,
        mut body: Value,
    ) -> Result<(), Failure> {
        if self.needs_voice
            && let Some(voice) = self.voice(state, channel_id)
        {
            body["voice"] = voice;
        }
        self.rest
            .update_player(&self.session, channel_id, &body)
            .await?;
        self.needs_voice = false;
        Ok(())
    }

    /// Plays `item` from `position_ms`.
    pub async fn play(
        &mut self,
        state: &AppState,
        channel_id: i64,
        item: &Item,
        position_ms: u64,
        paused: bool,
    ) -> Result<(), Failure> {
        let body = json!({
            "track": {
                "encoded": item.track.encoded,
                "userData": { "itemId": item.id, "channelId": channel_id.to_string() },
            },
            "position": position_ms,
            "paused": paused,
        });
        self.update(state, channel_id, body).await
    }

    pub async fn set_paused(
        &mut self,
        state: &AppState,
        channel_id: i64,
        paused: bool,
    ) -> Result<(), Failure> {
        self.update(state, channel_id, json!({ "paused": paused }))
            .await
    }

    pub async fn seek(
        &mut self,
        state: &AppState,
        channel_id: i64,
        position_ms: u64,
    ) -> Result<(), Failure> {
        self.update(state, channel_id, json!({ "position": position_ms }))
            .await
    }

    /// Plays nothing; the player stays.
    pub async fn stop(&mut self, state: &AppState, channel_id: i64) -> Result<(), Failure> {
        self.update(state, channel_id, json!({ "track": { "encoded": null } }))
            .await
    }

    /// The next update carries a new voice state.
    pub fn renew_voice(&mut self) {
        self.needs_voice = true;
    }

    /// Gives the bridge a new voice state: after a restart, or when
    /// Lavalink lost its voice connection.
    pub async fn reconnect_voice(
        &mut self,
        state: &AppState,
        channel_id: i64,
    ) -> Result<(), Failure> {
        self.needs_voice = true;
        self.update(state, channel_id, json!({})).await
    }

    /// What Lavalink's player for the channel plays, if anything we added.
    pub async fn playing(&self, channel_id: i64) -> Option<Playing> {
        let player = self
            .rest
            .player(&self.session, channel_id)
            .await
            .ok()
            .flatten()?;
        let item_id = player["track"]["userData"]["itemId"].as_str()?.to_owned();
        Some(Playing {
            item_id,
            position_ms: player["state"]["position"]
                .as_u64()
                .or_else(|| player["track"]["info"]["position"].as_u64())
                .unwrap_or(0),
            paused: player["paused"].as_bool().unwrap_or(false),
        })
    }

    /// Destroys the player; the bridge lets nothing more in for the
    /// channel. A node that cannot be reached is cleaned up when it is back
    /// (plan §12.7).
    pub fn destroy(self, state: &Arc<AppState>, channel_id: i64) {
        stop_stream(state, channel_id);
        tokio::spawn(async move {
            if let Err(failure) = self.rest.destroy_player(&self.session, channel_id).await {
                tracing::debug!(
                    channel_id,
                    node_id = self.node_id,
                    reason = failure.text(),
                    "a Lavalink player could not be destroyed now"
                );
            }
        });
    }

    /// Forgets the player without asking its node: it is on another node
    /// now, which has its own voice state at the bridge.
    pub fn abandon(self, channel_id: i64) {
        tokio::spawn(async move {
            let _ = self.rest.destroy_player(&self.session, channel_id).await;
        });
    }
}

/// Ends the channel's music stream and bridge connection, where the
/// channel's node is and on this server (plan §18.4).
pub fn stop_stream(state: &AppState, channel_id: i64) {
    if let Some(music_bridge) = &state.music_bridge {
        music_bridge.cancel(channel_id);
    }
    let command = NodeCommand::StopMusic { channel_id };
    match state.voice_nodes.node_of(channel_id) {
        Some(NodeId::External(_)) => state.voice_nodes.send(channel_id, command),
        _ => {
            if let Some(node) = state.voice_nodes.embedded() {
                node.send(command);
            }
        }
    }
}
