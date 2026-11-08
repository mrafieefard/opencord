//! Plugins' HTTP routes (design 2026-11-07 §9.1).

use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::State;
use axum::http::header::CONTENT_LENGTH;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::post;
use opencord_proto::v1 as proto;
use proto::response::Result as Reply;

use super::install;
use crate::error::ApiError;
use crate::media::{authorize, reply, reply_error};
use crate::state::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/plugins/packages", post(upload))
}

/// `POST /plugins/packages` with a plugin's zip as the body, by a member
/// with `MANAGE_PLUGINS`: the review it waits in.
async fn upload(State(state): State<Arc<AppState>>, headers: HeaderMap, body: Body) -> Response {
    let user_id = match authorize(&state, &headers)
        .and_then(|user_id| install::require_manager(&state, user_id).map(|()| user_id))
    {
        Ok(user_id) => user_id,
        Err(error) => return reply_error(&error),
    };
    if !state.config.plugins.enabled {
        return reply_error(&ApiError::new(
            proto::ErrorCode::PluginUnavailable,
            "plugins are turned off on this server",
        ));
    }
    let max_mb = state.config.plugins.max_package_mb;
    let max = usize::try_from(u64::from(max_mb) << 20).unwrap_or(usize::MAX);
    let too_large = || {
        let error = ApiError::new(
            proto::ErrorCode::PluginPackageInvalid,
            format!("the zip is larger than {max_mb} MB"),
        );
        reply(
            StatusCode::PAYLOAD_TOO_LARGE,
            Reply::Error(error.to_proto()),
        )
    };
    let declared = headers
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok());
    if declared.is_some_and(|length| length > max) {
        return too_large();
    }
    let zip = match to_bytes(body, max.saturating_add(1)).await {
        Ok(bytes) if bytes.len() <= max => bytes.to_vec(),
        Ok(_) => return too_large(),
        Err(error) => {
            // Mostly the limit; a dropped connection has nobody to answer.
            tracing::info!(%error, "a plugin upload wasn't read");
            return too_large();
        }
    };
    match install::upload(&state, user_id, zip).await {
        Ok(review) => reply(StatusCode::OK, Reply::PluginReview(Box::new(review))),
        Err(error) => reply_error(&error),
    }
}
