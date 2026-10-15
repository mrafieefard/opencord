//! The WebSocket gateway at `/gateway`.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::extract::ws::WebSocketUpgrade;
use axum::response::Response;

use crate::network::ClientAddr;
use crate::state::AppState;

pub mod connection;
pub mod frames;
pub mod identify;
pub mod session;

/// Larger frames are rejected.
pub const MAX_FRAME_BYTES: usize = 1 << 20;
/// Time a client has to answer `Hello`.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

pub async fn upgrade(
    State(state): State<Arc<AppState>>,
    ClientAddr(peer): ClientAddr,
    upgrade: WebSocketUpgrade,
) -> Response {
    upgrade
        .max_message_size(MAX_FRAME_BYTES)
        .max_frame_size(MAX_FRAME_BYTES)
        .on_upgrade(move |socket| connection::run(state, socket, peer.ip()))
}
