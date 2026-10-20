//! HTTP routes: the gateway plus a couple of plain endpoints.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{ConnectInfo, State};
use axum::http::header::HOST;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use opencord_voice::node::VoiceNode;
use serde::Serialize;

use crate::domains::http_check::VERIFY_PATH;
use crate::domains::web;
use crate::network::proxy::ThroughLocalListener;
use crate::state::AppState;
use crate::{gateway, media, music, voice};

/// Voice gateway messages are small.
const VOICE_FRAME_BYTES: usize = 64 * 1024;

pub fn router(state: Arc<AppState>) -> Router {
    let router = Router::new()
        .route("/", get(music::upgrade))
        .route(VERIFY_PATH, get(web::verify))
        .route("/invite/{code}", get(web::invite_page))
        .route("/health", get(health))
        .route("/info", get(info))
        .route("/gateway", get(gateway::upgrade))
        .route("/voice", get(voice_gateway))
        .merge(media::routes())
        .merge(crate::avatars::routes())
        .merge(crate::attachments::http::routes());
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

/// The embedded voice node's gateway. Reached through a tunnel or a
/// reverse proxy, which carry no UDP, its media goes straight to the
/// server's public address (Phase 3 plan §4, §9.2–9.3).
async fn voice_gateway(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    local: Option<axum::Extension<ThroughLocalListener>>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    let Some(node) = state.voice_nodes.embedded().cloned() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let host = headers.get(HOST).and_then(|host| host.to_str().ok());
    // A trusted peer alone is not enough: loopback is trusted by default,
    // and an app on the server's own machine connects from there.
    let forwarded = headers.contains_key("x-forwarded-for");
    let proxied = local.is_some()
        || (forwarded && state.network.settings().trusts(peer.ip()))
        || host.is_some_and(|host| state.domains.behind_proxy(host));
    if !proxied {
        return voice_gateway_upgrade(node, upgrade);
    }
    let public = state.network.current_public_ips();
    let media_host = public
        .v4
        .map(|ip| ip.to_string())
        .or_else(|| public.v6.map(|ip| ip.to_string()));
    upgrade
        .max_message_size(VOICE_FRAME_BYTES)
        .max_frame_size(VOICE_FRAME_BYTES)
        .on_upgrade(move |socket| async move { node.serve_proxied(socket, media_host).await })
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
