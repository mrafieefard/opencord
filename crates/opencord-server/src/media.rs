//! The media HTTP endpoints (Phase 2 plan §4.4), authenticated with media
//! tokens. Sound files are content-addressed and never change.

use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::header::{AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};

use crate::state::{AppState, now_ms};
use crate::voice::media_token;

/// Where the server's soundboard sounds are kept, inside the data directory.
pub const SOUNDS_DIR: &str = "sounds";

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/media/sounds/{sha256}", get(get_sound))
        .route("/media/sounds", post(not_yet))
        .route("/media/external-sounds", post(not_yet))
}

async fn get_sound(
    State(state): State<Arc<AppState>>,
    Path(sha256): Path<String>,
    headers: HeaderMap,
) -> Response {
    if let Err(status) = authorize(&state, &headers) {
        return status.into_response();
    }
    if !is_sha256_hex(&sha256) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let path = state
        .config
        .server
        .data_dir
        .join(SOUNDS_DIR)
        .join(format!("{sha256}.opus"));
    match tokio::fs::read(&path).await {
        Ok(bytes) => (
            [
                (CONTENT_TYPE, "audio/ogg"),
                (CACHE_CONTROL, "private, max-age=31536000, immutable"),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Uploads arrive with the soundboard (V8).
async fn not_yet(State(state): State<Arc<AppState>>, headers: HeaderMap) -> StatusCode {
    match authorize(&state, &headers) {
        Ok(_) => StatusCode::NOT_IMPLEMENTED,
        Err(status) => status,
    }
}

/// The member a request's bearer token belongs to.
fn authorize(state: &AppState, headers: &HeaderMap) -> Result<i64, StatusCode> {
    let token = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let server_id = state.guild().meta.server_id;
    let user_id = media_token::verify(
        &state.voice_key.verifying_key(),
        token.trim(),
        &server_id,
        now_ms(),
    )
    .map_err(|_| StatusCode::UNAUTHORIZED)?;
    if state.guild().members.contains_key(&user_id) {
        Ok(user_id)
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
