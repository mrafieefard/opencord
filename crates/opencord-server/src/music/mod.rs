//! Music in voice channels (Phase 3 plan Part B). Lavalink plays into the
//! audio bridge, which this server runs next to its embedded voice node:
//! Lavalink's WebSocket comes in on the main TLS port, its audio on the
//! music UDP port, and the frames go to the node as the channel's music.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use anyhow::Context as _;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::ws::rejection::WebSocketUpgradeRejection;
use axum::extract::{ConnectInfo, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use opencord_voice::bridge::{self, BridgeConfig, BridgeEvent, MusicBridge};
use opencord_voice::node::VoiceNode;
use serde::Deserialize;

use crate::config::Config;
use crate::state::AppState;

/// Lavalink's voice messages are small.
const BRIDGE_FRAME_BYTES: usize = 16 * 1024;

/// Starts the audio bridge when music is on and this server carries voice
/// itself. (External voice nodes run their own bridge: plan §13.1.)
pub async fn start_bridge(
    config: &Config,
    node: Option<&VoiceNode>,
) -> anyhow::Result<Option<MusicBridge>> {
    let Some(node) = node.filter(|_| config.music.enabled) else {
        return Ok(None);
    };
    let (bridge, mut frames, mut events) = MusicBridge::start(BridgeConfig {
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
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            match event {
                BridgeEvent::Connected {
                    channel_id,
                    connection,
                } => tracing::info!(channel_id, connection, "Lavalink connected for music"),
                BridgeEvent::Closed { channel_id, code } => {
                    tracing::info!(channel_id, code, "Lavalink's music connection ended");
                }
            }
        }
    });
    Ok(Some(bridge))
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
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
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
