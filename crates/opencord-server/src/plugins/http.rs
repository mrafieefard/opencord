//! Plugins' HTTP routes (design 2026-11-07 §9.1, §19).

use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::{Path, Query, State};
use axum::http::header::CONTENT_LENGTH;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::{get, post, put};
use opencord_proto::v1 as proto;
use proto::response::Result as Reply;
use serde::Deserialize;

use super::dev::{self, Pushed};
use super::install;
use crate::error::ApiError;
use crate::media::{authorize, reply, reply_error};
use crate::state::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/plugins/packages", post(upload))
        .route("/plugins/dev/{id}", put(dev_push))
        .route("/plugins/dev/{id}/log", get(dev_log))
}

fn plugins_off() -> Response {
    reply_error(&ApiError::new(
        proto::ErrorCode::PluginUnavailable,
        "plugins are turned off on this server",
    ))
}

fn too_large(state: &AppState) -> Response {
    let error = ApiError::new(
        proto::ErrorCode::PluginPackageInvalid,
        format!(
            "the zip is larger than {} MB",
            state.config.plugins.max_package_mb
        ),
    );
    reply(
        StatusCode::PAYLOAD_TOO_LARGE,
        Reply::Error(error.to_proto()),
    )
}

/// The zip in `body`, read no further than the server's package limit;
/// `None` when it is larger.
async fn read_zip(state: &AppState, headers: &HeaderMap, body: Body) -> Option<Vec<u8>> {
    let max_mb = state.config.plugins.max_package_mb;
    let max = usize::try_from(u64::from(max_mb) << 20).unwrap_or(usize::MAX);
    let declared = headers
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok());
    if declared.is_some_and(|length| length > max) {
        return None;
    }
    match to_bytes(body, max.saturating_add(1)).await {
        Ok(bytes) if bytes.len() <= max => Some(bytes.to_vec()),
        Ok(_) => None,
        Err(error) => {
            // Mostly the limit; a dropped connection has nobody to answer.
            tracing::info!(%error, "a plugin upload wasn't read");
            None
        }
    }
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
        return plugins_off();
    }
    let Some(zip) = read_zip(&state, &headers, body).await else {
        return too_large(&state);
    };
    match install::upload(&state, user_id, zip).await {
        Ok(review) => reply(StatusCode::OK, Reply::PluginReview(Box::new(review))),
        Err(error) => reply_error(&error),
    }
}

/// `PUT /plugins/dev/{id}` with a dev token for `{id}` and a zip: `200`
/// with the plugin once installed, or `202` with the review it waits in.
async fn dev_push(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let token = match dev::authorize(&state, &headers, &id).await {
        Ok(token) => token,
        Err(error) => return reply_error(&error),
    };
    if !state.config.plugins.enabled {
        return plugins_off();
    }
    if let Err(error) = dev::count_push(&state, &token).await {
        return reply_error(&error);
    }
    let Some(zip) = read_zip(&state, &headers, body).await else {
        return too_large(&state);
    };
    match dev::push(&state, &token, zip).await {
        Ok(Pushed::Review(review)) => reply(StatusCode::ACCEPTED, Reply::PluginReview(review)),
        Ok(Pushed::Installed(plugin)) => reply(StatusCode::OK, Reply::Plugin(*plugin)),
        Err(error) => reply_error(&error),
    }
}

#[derive(Deserialize)]
struct LogQuery {
    #[serde(default)]
    after: u64,
}

/// `GET /plugins/dev/{id}/log?after=<seq>` with a dev token for `{id}`:
/// the lines after `seq`, once there are some or after 25 s.
async fn dev_log(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<LogQuery>,
    headers: HeaderMap,
) -> Response {
    if let Err(error) = dev::authorize(&state, &headers, &id).await {
        return reply_error(&error);
    }
    let lines = dev::wait_for_log(&state, &id, query.after).await;
    reply(StatusCode::OK, Reply::PluginLog(proto::PluginLog { lines }))
}
