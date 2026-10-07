//! HTTP routes: the gateway plus a couple of plain endpoints.

use std::sync::Arc;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::state::AppState;
use crate::{gateway, media};

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/info", get(info))
        .route("/gateway", get(gateway::upgrade))
        .merge(media::routes())
        .with_state(state)
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
        voice_udp_port: state
            .config
            .voice
            .enabled
            .then_some(state.config.voice.udp_port),
    })
}
