//! Profile pictures and the server icon: each member sets their own, and
//! whoever manages the server sets its icon. Each is a 256-pixel square PNG
//! (`opencord_common::avatar`), stored once by its SHA-256 in
//! `data/avatars`, and fetched by members with their media token like
//! every other file here: never public.

use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::{Path, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use opencord_common::avatar::{self, MAX_UPLOAD_BYTES};
use opencord_common::permissions::Permissions;
use opencord_proto::v1 as proto;
use proto::event::Kind;
use proto::response::Result as Reply;

use crate::db::meta::ServerMeta;
use crate::db::users;
use crate::error::ApiError;
use crate::media::{authorize, reply, reply_error};
use crate::permissions::require_base;
use crate::state::{AppState, Audience};

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/media/avatar", post(set_avatar).delete(clear_avatar))
        .route("/media/server-icon", post(set_icon).delete(clear_icon))
        .route("/media/avatars/{hash}", get(get_picture))
}

fn picture_path(data_dir: &FsPath, hash: &str) -> PathBuf {
    data_dir.join("avatars").join(format!("{hash}.png"))
}

/// The picture in the body: read up to its limit and no further.
async fn read_picture(body: Body) -> Result<Vec<u8>, ApiError> {
    let bytes = to_bytes(body, MAX_UPLOAD_BYTES + 1)
        .await
        .map_err(|_| ApiError::invalid_argument("a picture is at most 8 MB"))?;
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err(ApiError::invalid_argument("a picture is at most 8 MB"));
    }
    Ok(bytes.to_vec())
}

/// Stores the picture in [bytes] as a 256-pixel square PNG, kept as it is
/// when it already is one; its hash.
async fn store(state: &AppState, bytes: Vec<u8>) -> Result<String, ApiError> {
    let png = tokio::task::spawn_blocking(move || {
        if avatar::is_normalized(&bytes) {
            Ok(bytes)
        } else {
            avatar::normalize(&bytes)
        }
    })
    .await
    .map_err(|_| ApiError::internal())?
    .map_err(|error| ApiError::invalid_argument(error.to_string()))?;
    let hash = avatar::hash(&png);
    let path = picture_path(&state.config.server.data_dir, &hash);
    if tokio::fs::try_exists(&path).await.unwrap_or(false) {
        return Ok(hash);
    }
    let failed = |error: std::io::Error| {
        tracing::error!(%error, "a picture couldn't be stored");
        ApiError::internal()
    };
    if let Some(folder) = path.parent() {
        tokio::fs::create_dir_all(folder).await.map_err(failed)?;
    }
    // Written aside first, so a reader never sees half a picture.
    let partial = path.with_extension("png.part");
    tokio::fs::write(&partial, &png).await.map_err(failed)?;
    tokio::fs::rename(&partial, &path).await.map_err(failed)?;
    Ok(hash)
}

/// `POST /media/avatar` with a picture as the body: the member's profile
/// picture. The answer is their `User`.
async fn set_avatar(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    match change_avatar(&state, &headers, Some(body)).await {
        Ok(user) => reply(StatusCode::OK, Reply::User(user)),
        Err(error) => reply_error(&error),
    }
}

/// `DELETE /media/avatar`: the member's profile picture goes.
async fn clear_avatar(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    match change_avatar(&state, &headers, None).await {
        Ok(user) => reply(StatusCode::OK, Reply::User(user)),
        Err(error) => reply_error(&error),
    }
}

async fn change_avatar(
    state: &AppState,
    headers: &HeaderMap,
    body: Option<Body>,
) -> Result<proto::User, ApiError> {
    let user_id = authorize(state, headers)?;
    let hash = match body {
        Some(body) => Some(store(state, read_picture(body).await?).await?),
        None => None,
    };
    let _writes = state.write_lock().await;
    users::set_avatar_hash(&mut *state.db.acquire().await?, user_id, hash.as_deref()).await?;
    let member = {
        let mut guild = state.guild_mut();
        let member = guild
            .members
            .get_mut(&user_id)
            .ok_or_else(|| ApiError::not_found("member"))?;
        member.user.avatar_hash = hash;
        member.clone()
    };
    state.broadcast(
        Kind::MemberUpdate(proto::MemberUpdate {
            member: Some(member.to_proto()),
        }),
        Audience::Everyone,
    );
    Ok(member.user.to_proto())
}

/// `POST /media/server-icon` with a picture as the body: the server's
/// icon. Manage server. The answer is the `ServerInfo`.
async fn set_icon(State(state): State<Arc<AppState>>, headers: HeaderMap, body: Body) -> Response {
    match change_icon(&state, &headers, Some(body)).await {
        Ok(info) => reply(StatusCode::OK, Reply::Server(info)),
        Err(error) => reply_error(&error),
    }
}

/// `DELETE /media/server-icon`: the server's icon goes.
async fn clear_icon(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    match change_icon(&state, &headers, None).await {
        Ok(info) => reply(StatusCode::OK, Reply::Server(info)),
        Err(error) => reply_error(&error),
    }
}

async fn change_icon(
    state: &AppState,
    headers: &HeaderMap,
    body: Option<Body>,
) -> Result<proto::ServerInfo, ApiError> {
    let user_id = authorize(state, headers)?;
    require_base(&state.guild(), user_id, Permissions::MANAGE_SERVER)?;
    let hash = match body {
        Some(body) => Some(store(state, read_picture(body).await?).await?),
        None => None,
    };
    let _writes = state.write_lock().await;
    // Checked again under the lock: the permission may have gone meanwhile.
    let meta = {
        let guild = state.guild();
        require_base(&guild, user_id, Permissions::MANAGE_SERVER)?;
        ServerMeta {
            icon_hash: hash,
            ..guild.meta.clone()
        }
    };
    meta.save(&mut *state.db.acquire().await?).await?;
    let info = {
        let mut guild = state.guild_mut();
        guild.meta = meta;
        guild.server_info()
    };
    state.broadcast(
        Kind::ServerUpdate(proto::ServerUpdate {
            server: Some(info.clone()),
        }),
        Audience::Everyone,
    );
    Ok(info)
}

/// `GET /media/avatars/{hash}`: a profile picture or server icon, by its
/// hash. It never changes, so it can be kept.
async fn get_picture(
    State(state): State<Arc<AppState>>,
    Path(hash): Path<String>,
    headers: HeaderMap,
) -> Response {
    if let Err(error) = authorize(&state, &headers) {
        return reply_error(&error);
    }
    if !avatar::valid_hash(&hash) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    match tokio::fs::read(picture_path(&state.config.server.data_dir, &hash)).await {
        Ok(bytes) => (
            [
                (CONTENT_TYPE, "image/png"),
                (CACHE_CONTROL, "private, max-age=31536000, immutable"),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
