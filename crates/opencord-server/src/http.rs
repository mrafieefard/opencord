//! HTTP routes: the gateway plus a couple of plain endpoints.

use std::sync::Arc;

use axum::extract::State;
use axum::extract::ws::WebSocketUpgrade;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use opencord_voice::node::VoiceNode;
use serde::Serialize;

use crate::state::AppState;
use crate::{gateway, media, voice};

/// Voice gateway messages are small.
const VOICE_FRAME_BYTES: usize = 64 * 1024;

pub fn router(state: Arc<AppState>) -> Router {
    let router = Router::new()
        .route("/health", get(health))
        .route("/info", get(info))
        .route("/gateway", get(gateway::upgrade))
        .route("/voice", get(voice_gateway))
        .merge(media::routes());
    // Only servers with external voice nodes listen for them.
    let voice = &state.config.voice;
    let router = if voice.enabled && !voice.external_nodes.is_empty() {
        router.route(
            opencord_voice::control::CONTROL_PATH,
            get(voice::control::upgrade),
        )
    } else {
        router
    };
    router.with_state(state)
}

#[derive(Debug, Serialize)]
struct Health {
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct Info {
    name: String,
    version: &'static str,
    protocol_version: u32,
    member_count: usize,
    voice_enabled: bool,
    /// Where media goes, when voice is on.
    voice_udp_port: Option<u16>,
}

/// The embedded voice node's gateway.
async fn voice_gateway(State(state): State<Arc<AppState>>, upgrade: WebSocketUpgrade) -> Response {
    match state.voice_nodes.embedded().cloned() {
        Some(node) => voice_gateway_upgrade(node, upgrade),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Hands a voice gateway WebSocket to `node`.
pub fn voice_gateway_upgrade(node: VoiceNode, upgrade: WebSocketUpgrade) -> Response {
    upgrade
        .max_message_size(VOICE_FRAME_BYTES)
        .max_frame_size(VOICE_FRAME_BYTES)
        .on_upgrade(move |socket| async move { node.serve(socket).await })
}

async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

async fn info(State(state): State<Arc<AppState>>) -> Json<Info> {
    let guild = state.guild();
    Json(Info {
        name: guild.meta.name.clone(),
        version: env!("CARGO_PKG_VERSION"),
        protocol_version: opencord_common::PROTOCOL_VERSION,
        member_count: guild.members.len(),
        voice_enabled: state.config.voice.enabled,
        voice_udp_port: state.voice_nodes.embedded().map(VoiceNode::udp_port),
    })
}
