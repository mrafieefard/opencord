//! Screen shares: going live, changing quality, stopping, and watching
//! (Phase 2 plan §9).

use opencord_common::permissions::Permissions;
use opencord_proto::v1 as proto;
use proto::event::Kind;
use proto::response::Result as Response;

use super::{Ctx, ack};
use crate::error::ApiError;
use crate::permissions::require_channel;
use crate::state::Audience;
use crate::voice::streams::{self, Stream, check_quality, check_source};

pub async fn create(ctx: &Ctx<'_>, request: proto::CreateStream) -> Result<Response, ApiError> {
    ctx.state
        .rate_limits
        .check_stream_create(ctx.user_id)
        .map_err(ApiError::rate_limited)?;
    let _writes = ctx.state.write_lock().await;
    let (stream, created) = {
        let guild = ctx.state.guild();
        let mut voice = ctx.state.voice();
        let in_channel = voice
            .get(ctx.user_id)
            .is_some_and(|vs| vs.channel_id == request.channel_id);
        if !in_channel {
            return Err(ApiError::voice_not_connected("you are"));
        }
        require_channel(
            &guild,
            ctx.user_id,
            request.channel_id,
            Permissions::SCREENSHARE,
        )?;
        let source_kind = check_source(request.source_kind)?;
        let resolution = check_quality(&guild.voice_settings, request.resolution, request.fps)?;
        // Going live again keeps the viewers: a new source or quality.
        let previous = voice.streams.remove(ctx.user_id);
        let created = previous.is_none();
        let stream = Stream {
            channel_id: request.channel_id,
            user_id: ctx.user_id,
            source_kind,
            resolution,
            fps: request.fps,
            has_audio: request.has_audio,
            viewers: previous
                .map(|previous| previous.viewers)
                .unwrap_or_default(),
        };
        voice.streams.put(stream.clone());
        (stream, created)
    };
    let shown = stream.to_proto();
    let event = match created {
        true => Kind::StreamCreate(proto::StreamCreate {
            stream: Some(shown.clone()),
        }),
        false => Kind::StreamUpdate(proto::StreamUpdate {
            stream: Some(shown.clone()),
        }),
    };
    ctx.state
        .broadcast(event, Audience::Channel(stream.channel_id));
    streams::set_streaming(ctx.state, ctx.user_id, true);
    Ok(Response::Stream(shown))
}

pub async fn update(ctx: &Ctx<'_>, request: proto::UpdateStream) -> Result<Response, ApiError> {
    let _writes = ctx.state.write_lock().await;
    let stream = {
        let guild = ctx.state.guild();
        let mut voice = ctx.state.voice();
        let current = own_stream(&voice.streams, ctx.user_id, &request.stream_key)?;
        let resolution = request
            .resolution
            .unwrap_or(current.resolution.to_proto() as i32);
        let fps = request.fps.unwrap_or(current.fps);
        let updated = Stream {
            resolution: check_quality(&guild.voice_settings, resolution, fps)?,
            fps,
            has_audio: request.has_audio.unwrap_or(current.has_audio),
            ..current
        };
        voice.streams.put(updated.clone());
        updated
    };
    let shown = stream.to_proto();
    ctx.state.broadcast(
        Kind::StreamUpdate(proto::StreamUpdate {
            stream: Some(shown.clone()),
        }),
        Audience::Channel(stream.channel_id),
    );
    Ok(Response::Stream(shown))
}

pub async fn delete(ctx: &Ctx<'_>, request: proto::DeleteStream) -> Result<Response, ApiError> {
    let _writes = ctx.state.write_lock().await;
    own_stream(&ctx.state.voice().streams, ctx.user_id, &request.stream_key)?;
    streams::end(ctx.state, ctx.user_id);
    streams::set_streaming(ctx.state, ctx.user_id, false);
    Ok(ack())
}

pub async fn watch(ctx: &Ctx<'_>, request: proto::WatchStream) -> Result<Response, ApiError> {
    set_watching(ctx, &request.stream_key, true).await
}

pub async fn unwatch(ctx: &Ctx<'_>, request: proto::UnwatchStream) -> Result<Response, ApiError> {
    set_watching(ctx, &request.stream_key, false).await
}

/// Starts or stops watching: viewers are in the stream's voice channel,
/// and there are at most the server's maximum of them.
async fn set_watching(ctx: &Ctx<'_>, key: &str, watching: bool) -> Result<Response, ApiError> {
    let _writes = ctx.state.write_lock().await;
    let changed = {
        let guild = ctx.state.guild();
        let mut voice = ctx.state.voice();
        let stream = voice
            .streams
            .by_key(key)
            .cloned()
            .ok_or_else(|| ApiError::not_found("stream"))?;
        if watching {
            if stream.user_id == ctx.user_id {
                return Err(ApiError::invalid_argument("that is your own stream"));
            }
            let in_channel = voice
                .get(ctx.user_id)
                .is_some_and(|vs| vs.channel_id == stream.channel_id);
            if !in_channel {
                return Err(ApiError::voice_not_connected("you are"));
            }
            let max = guild.voice_settings.max_stream_viewers;
            let full = stream.viewers.len() >= usize::try_from(max).unwrap_or(usize::MAX);
            if full && !stream.viewers.contains(&ctx.user_id) {
                return Err(ApiError::stream_viewer_limit(max));
            }
        }
        voice
            .streams
            .set_viewer(stream.user_id, ctx.user_id, watching)
    };
    if let Some(stream) = changed {
        streams::viewers_changed(ctx.state, &stream);
    }
    Ok(ack())
}

/// The caller's own live stream that `key` names.
fn own_stream(streams: &streams::Streams, user_id: i64, key: &str) -> Result<Stream, ApiError> {
    let stream = streams
        .by_key(key)
        .ok_or_else(|| ApiError::not_found("stream"))?;
    if stream.user_id != user_id {
        return Err(ApiError::forbidden("that is someone else's stream"));
    }
    Ok(stream.clone())
}
