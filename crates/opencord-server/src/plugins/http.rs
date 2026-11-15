//! Plugins' HTTP routes (design 2026-11-07 §9.1, §19).

use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::{Path, Query, State};
use axum::http::header::CONTENT_LENGTH;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post, put};
use opencord_proto::v1 as proto;
use proto::response::Result as Reply;
use serde::Deserialize;

use super::assets;
use super::dev::{self, Pushed};
use super::images;
use super::install;
use super::permissions::PluginPermission;
use crate::db;
use crate::error::ApiError;
use crate::media::{authorize, reply, reply_error};
use crate::state::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/plugins/packages", post(upload))
        .route("/plugins/dev/{id}", put(dev_push))
        .route("/plugins/dev/{id}/log", get(dev_log))
        .route("/plugins/{id}/hooks", any(hook_root))
        .route("/plugins/{id}/hooks/{*path}", any(hook_path))
        .route("/plugins/{id}/assets/{*path}", get(assets::get_asset))
        .route("/plugins/{id}/files/t/{token}", get(assets::get_token_file))
        .route("/plugins/{id}/files/{*path}", get(assets::get_file))
        .route("/plugins/{id}/images", get(images::get_image))
}

pub(super) fn plugins_off() -> Response {
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

async fn hook_root(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<std::collections::HashMap<String, String>>,
    method: axum::http::Method,
    headers: HeaderMap,
    body: Body,
) -> Response {
    handle_hook(state, id, String::new(), query, method, headers, body).await
}

async fn hook_path(
    State(state): State<Arc<AppState>>,
    Path((id, path)): Path<(String, String)>,
    Query(query): Query<std::collections::HashMap<String, String>>,
    method: axum::http::Method,
    headers: HeaderMap,
    body: Body,
) -> Response {
    handle_hook(state, id, path, query, method, headers, body).await
}

async fn handle_hook(
    state: Arc<AppState>,
    id: String,
    path: String,
    query: std::collections::HashMap<String, String>,
    method: axum::http::Method,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let path = path.trim_start_matches('/').to_owned();

    // 1. Check plugin existence
    let handle = state.plugins.get(&id);
    let handle = match handle {
        Some(h) => h,
        None => {
            let in_db = match state.db.acquire().await {
                Ok(mut conn) => db::plugins::plugin(&mut conn, &id)
                    .await
                    .unwrap_or(None)
                    .is_some(),
                Err(_) => false,
            };
            if in_db {
                return (StatusCode::SERVICE_UNAVAILABLE, "plugin stopped").into_response();
            } else {
                return (StatusCode::NOT_FOUND, "plugin not found").into_response();
            }
        }
    };

    // 2. 503 when it's stopped
    if handle.state() != proto::PluginState::Running {
        return (StatusCode::SERVICE_UNAVAILABLE, "plugin stopped").into_response();
    }

    // 3. 403 without receive_web_requests
    if !handle.holds(PluginPermission::ReceiveWebRequests) {
        return (
            StatusCode::FORBIDDEN,
            "missing receive_web_requests permission",
        )
            .into_response();
    }

    // 4. 404 when no handler for that exact path (cheapest check before body/rate limit)
    if !handle.has_hook(&path) {
        return (StatusCode::NOT_FOUND, "no handler for path").into_response();
    }

    // 5. 429 past 60 a minute for the plugin (checked before reading body)
    if state.rate_limits.check_web_hook(&id).is_err() {
        return (StatusCode::TOO_MANY_REQUESTS, "rate limited").into_response();
    }

    // 6. 413 for a body over 1 MiB
    let max_bytes = 1024 * 1024;
    let declared = headers
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok());
    if declared.is_some_and(|len| len > max_bytes) {
        return (StatusCode::PAYLOAD_TOO_LARGE, "body exceeds 1 MiB").into_response();
    }
    let body_bytes = match to_bytes(body, max_bytes + 1).await {
        Ok(bytes) if bytes.len() <= max_bytes => bytes.to_vec(),
        _ => return (StatusCode::PAYLOAD_TOO_LARGE, "body exceeds 1 MiB").into_response(),
    };

    // 7. Prepare request: method, path, query, headers (lowercase, no cookie), body
    let mut query_map = serde_json::Map::new();
    for (k, v) in query {
        query_map.insert(k, serde_json::Value::String(v));
    }

    let mut headers_map = serde_json::Map::new();
    for (name, value) in &headers {
        let name_lower = name.as_str().to_ascii_lowercase();
        if name_lower != "cookie"
            && let Ok(v_str) = value.to_str()
        {
            headers_map.insert(name_lower, serde_json::Value::String(v_str.to_owned()));
        }
    }

    let body_str = String::from_utf8_lossy(&body_bytes).into_owned();

    let (tx, rx) = tokio::sync::oneshot::channel();
    let event = crate::plugins::PluginEvent {
        name: "hook".to_owned(),
        payload: serde_json::json!({
            "method": method.as_str(),
            "path": path,
            "query": serde_json::Value::Object(query_map),
            "headers": serde_json::Value::Object(headers_map),
            "body": body_str,
        }),
        reply: Some(tx),
        command: None,
    };
    handle.deliver(event);

    let answer = match tokio::time::timeout(std::time::Duration::from_secs(10), rx).await {
        Ok(Ok(val)) => val,
        Ok(Err(_)) => {
            handle.log().push(
                proto::PluginLogLevel::Action,
                &format!("hook {} {} → 500", method.as_str(), path),
            );
            return (StatusCode::INTERNAL_SERVER_ERROR, "handler failed").into_response();
        }
        Err(_) => {
            handle.log().push(
                proto::PluginLogLevel::Action,
                &format!("hook {} {} → 504", method.as_str(), path),
            );
            return (StatusCode::GATEWAY_TIMEOUT, "hook timed out").into_response();
        }
    };

    let status_code = answer
        .get("status")
        .and_then(|v| v.as_u64())
        .and_then(|s| u16::try_from(s).ok())
        .and_then(|s| StatusCode::from_u16(s).ok())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    handle.log().push(
        proto::PluginLogLevel::Action,
        &format!(
            "hook {} {} → {}",
            method.as_str(),
            path,
            status_code.as_u16()
        ),
    );

    const FORBIDDEN_HOOK_RESPONSE_HEADERS: &[&str] = &[
        "set-cookie",
        "content-length",
        "transfer-encoding",
        "connection",
        "keep-alive",
        "proxy-connection",
        "te",
        "trailer",
        "upgrade",
    ];

    let mut resp_headers = HeaderMap::new();
    if let Some(headers_obj) = answer.get("headers").and_then(|v| v.as_object()) {
        for (k, v) in headers_obj {
            if !FORBIDDEN_HOOK_RESPONSE_HEADERS
                .iter()
                .any(|forbidden| k.eq_ignore_ascii_case(forbidden))
                && let (Ok(hname), Some(v_str)) =
                    (axum::http::HeaderName::from_bytes(k.as_bytes()), v.as_str())
                && let Ok(hval) = axum::http::HeaderValue::from_str(v_str)
            {
                resp_headers.insert(hname, hval);
            }
        }
    }
    let resp_body = answer
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_owned();

    (status_code, resp_headers, resp_body).into_response()
}
