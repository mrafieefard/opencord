//! The media HTTP endpoints (Phase 2 plan §4.4), authenticated with media
//! tokens. Sound files are content-addressed and never change. Uploads are
//! read up to the size limit and no further, then parsed: what the client
//! says about a file is never trusted (plan §11.3, §14).
//!
//! Uploads answer with a protobuf `Response`, the same envelope as gateway
//! requests: the new sound, an ack, or an error with its code.

use std::sync::Arc;

use axum::Router;
use axum::body::{Body, Bytes, to_bytes};
use axum::extract::{Path, Query, State};
use axum::http::header::{AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use opencord_common::permissions::Permissions;
use opencord_common::sound::{self, EXTERNAL_CACHE_BYTES, MAX_FILE_BYTES};
use opencord_common::validation;
use opencord_proto::v1 as proto;
use prost::Message as _;
use proto::event::Kind;
use proto::response::Result as Reply;
use serde::Deserialize;

use crate::db::sounds::{self, ExternalRow};
use crate::error::ApiError;
use crate::handlers::soundboard::check_volume;
use crate::permissions::require_base;
use crate::soundboard::{self, Sound};
use crate::state::{AppState, Audience, now_ms};
use crate::voice::media_token;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/media/sounds/{sha256}", get(get_sound))
        .route("/media/sounds", post(upload_sound))
        .route("/media/external-sounds", post(upload_external_sound))
}

async fn get_sound(
    State(state): State<Arc<AppState>>,
    Path(sha256): Path<String>,
    headers: HeaderMap,
) -> Response {
    if let Err(error) = authorize(&state, &headers) {
        return reply_error(&error);
    }
    let Some(hash) = sound::sha256(&sha256) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    match tokio::fs::read(soundboard::file_path(&state.config.server.data_dir, &hash)).await {
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

#[derive(Debug, Deserialize)]
struct NewSound {
    name: String,
    emoji: Option<String>,
    volume: Option<u32>,
}

/// `POST /media/sounds?name=…&emoji=…&volume=…` with the Ogg Opus file as
/// the body: a new sound on this server's soundboard.
async fn upload_sound(
    State(state): State<Arc<AppState>>,
    Query(new): Query<NewSound>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    match add_sound(&state, new, &headers, body).await {
        Ok(sound) => reply(StatusCode::CREATED, Reply::SoundboardSound(sound)),
        Err(error) => reply_error(&error),
    }
}

async fn add_sound(
    state: &AppState,
    new: NewSound,
    headers: &HeaderMap,
    body: Body,
) -> Result<proto::SoundboardSound, ApiError> {
    let user_id = authorize(state, headers)?;
    require_base(&state.guild(), user_id, Permissions::MANAGE_SOUNDBOARD)?;
    let name = validation::sound_name(&new.name)?;
    let emoji = validation::sound_emoji(new.emoji.as_deref().unwrap_or_default())?;
    let volume = check_volume(new.volume)?;
    let bytes = read_limited(body).await?;
    let checked = soundboard::check_file(&bytes)?;

    let _writes = state.write_lock().await;
    let max_sounds = state.guild().voice_settings.max_sounds;
    if state.guild().sounds.len() >= usize::try_from(max_sounds).unwrap_or(usize::MAX) {
        return Err(ApiError::soundboard_full(max_sounds));
    }
    store(state, &checked.sha256, &bytes).await?;
    let sound = Sound {
        id: state.ids.next_id(),
        name,
        emoji,
        volume,
        sha256: checked.sha256,
        size_bytes: checked.size_bytes,
        duration_ms: checked.duration_ms,
        channels: checked.channels,
        uploader_id: Some(user_id),
        created_at: now_ms(),
    };
    let mut conn = state.db.acquire().await?;
    sounds::insert(&mut conn, &sound.to_row()).await?;
    state.guild_mut().sounds.insert(sound.id, sound.clone());
    let shown = sound.to_proto();
    state.broadcast(
        Kind::SoundboardSoundCreate(proto::SoundboardSoundCreate {
            sound: Some(shown.clone()),
        }),
        Audience::Everyone,
    );
    Ok(shown)
}

/// `POST /media/external-sounds` with the Ogg Opus file as the body: a
/// sound from another server, kept so this server's listeners can fetch it
/// (plan §11.5). The answer is an ack; then play it as `external:<sha256>`.
async fn upload_external_sound(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    match add_external_sound(&state, &headers, body).await {
        Ok(()) => reply(StatusCode::OK, Reply::Ack(proto::Ack {})),
        Err(error) => reply_error(&error),
    }
}

async fn add_external_sound(
    state: &AppState,
    headers: &HeaderMap,
    body: Body,
) -> Result<(), ApiError> {
    let user_id = authorize(state, headers)?;
    if !state.guild().voice_settings.allow_external_sounds {
        return Err(ApiError::forbidden(
            "sounds from other servers are turned off here",
        ));
    }
    state
        .rate_limits
        .check_external_sound(user_id)
        .map_err(ApiError::rate_limited)?;
    let bytes = read_limited(body).await?;
    let checked = soundboard::check_file(&bytes)?;

    let _writes = state.write_lock().await;
    store(state, &checked.sha256, &bytes).await?;
    let mut conn = state.db.acquire().await?;
    sounds::put_external(
        &mut conn,
        &ExternalRow {
            sha256: checked.sha256.to_vec(),
            size_bytes: i64::from(checked.size_bytes),
            duration_ms: i64::from(checked.duration_ms),
            uploaded_by: Some(user_id),
            last_used_at: now_ms(),
        },
    )
    .await?;
    // Least recently played first, until the cache fits again.
    let rows = sounds::list_external(&mut conn).await?;
    let limit = i64::try_from(EXTERNAL_CACHE_BYTES).unwrap_or(i64::MAX);
    let mut total: i64 = rows.iter().map(|row| row.size_bytes).sum();
    for row in rows {
        if total <= limit {
            break;
        }
        if row.sha256 == checked.sha256 {
            continue;
        }
        sounds::delete_external(&mut conn, &row.sha256).await?;
        total -= row.size_bytes;
        let Ok(hash) = <[u8; 32]>::try_from(row.sha256.as_slice()) else {
            continue;
        };
        let used_here = state
            .guild()
            .sounds
            .values()
            .any(|sound| sound.sha256 == hash);
        if !used_here {
            soundboard::remove_file(&state.config.server.data_dir, &hash).await;
        }
    }
    Ok(())
}

/// The upload, read no further than the size limit.
async fn read_limited(body: Body) -> Result<Bytes, ApiError> {
    to_bytes(body, MAX_FILE_BYTES).await.map_err(|_| {
        ApiError::sound_invalid(format!(
            "the file is larger than {} KB",
            MAX_FILE_BYTES / 1024
        ))
    })
}

async fn store(state: &AppState, sha256: &[u8; 32], bytes: &[u8]) -> Result<(), ApiError> {
    soundboard::store(&state.config.server.data_dir, sha256, bytes)
        .await
        .map_err(|error| {
            tracing::error!(%error, "a sound file could not be stored");
            ApiError::internal()
        })
}

fn reply(status: StatusCode, result: Reply) -> Response {
    let body = proto::Response {
        result: Some(result),
    }
    .encode_to_vec();
    (status, [(CONTENT_TYPE, "application/x-protobuf")], body).into_response()
}

fn reply_error(error: &ApiError) -> Response {
    use proto::ErrorCode as Code;
    let status = match error.code {
        Code::Unauthorized => StatusCode::UNAUTHORIZED,
        Code::Forbidden => StatusCode::FORBIDDEN,
        Code::NotFound => StatusCode::NOT_FOUND,
        Code::RateLimited => StatusCode::TOO_MANY_REQUESTS,
        Code::SoundboardFull | Code::Conflict => StatusCode::CONFLICT,
        Code::SoundTooLong | Code::SoundInvalid | Code::InvalidArgument => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    reply(status, Reply::Error(error.to_proto()))
}

/// The member a request's bearer token belongs to.
fn authorize(state: &AppState, headers: &HeaderMap) -> Result<i64, ApiError> {
    let token = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or_else(|| ApiError::unauthorized("a media token is needed"))?;
    let server_id = state.guild().meta.server_id;
    let user_id = media_token::verify(
        &state.voice_key.verifying_key(),
        token.trim(),
        &server_id,
        now_ms(),
    )
    .map_err(|_| ApiError::unauthorized("the media token is not valid"))?;
    if state.guild().members.contains_key(&user_id) {
        Ok(user_id)
    } else {
        Err(ApiError::forbidden("you are not a member of this server"))
    }
}
