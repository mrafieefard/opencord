//! Joining and leaving voice, and moderating people in voice channels.

use opencord_common::channel::ChannelKind;
use opencord_common::permissions::Permissions;
use opencord_proto::v1 as proto;
use proto::response::Result as Response;

use super::{Ctx, ack};
use crate::error::ApiError;
use crate::permissions::{require_channel, require_outranks_member};
use crate::voice::states::{self, Moderation, SelfFlags, VoiceState, check_join, suppressed};
use crate::voice::{self, announce, send_server_update, server_update};

pub async fn update_voice_state(
    ctx: &Ctx<'_>,
    request: proto::UpdateVoiceState,
) -> Result<Response, ApiError> {
    ctx.state
        .rate_limits
        .check_voice_state(ctx.user_id)
        .map_err(ApiError::rate_limited)?;
    let _writes = ctx.state.write_lock().await;
    let Some(channel_id) = request.channel_id else {
        return Ok(Response::VoiceState(leave(ctx)));
    };
    let flags = SelfFlags {
        mute: request.self_mute,
        deaf: request.self_deaf,
        video: request.self_video,
        stream: request.self_stream,
    };
    let (shown, previous_channel, update) = {
        let guild = ctx.state.guild();
        let mut voice_states = ctx.state.voice();
        check_join(
            &guild,
            &voice_states,
            voice::config(ctx.state),
            ctx.user_id,
            channel_id,
            flags,
        )?;
        let previous = voice_states.get(ctx.user_id).cloned();
        let joined = VoiceState {
            user_id: ctx.user_id,
            channel_id,
            session_id: ctx.session_id.to_owned(),
            self_mute: flags.mute,
            self_deaf: flags.deaf,
            self_video: flags.video,
            // Going live is CreateStream; a move ends the stream.
            self_stream: voice_states
                .streams
                .of(ctx.user_id)
                .is_some_and(|stream| stream.channel_id == channel_id),
            suppress: suppressed(&guild, ctx.user_id, channel_id),
        };
        let connecting = previous.as_ref().is_none_or(|previous| {
            previous.channel_id != channel_id || previous.session_id != joined.session_id
        });
        let update = match connecting {
            true => Some(
                server_update(ctx.state, &guild, &voice_states, &joined)
                    .ok_or_else(no_voice_node)?,
            ),
            false => None,
        };
        let shown = voice_states.to_proto(&joined);
        voice_states.put(joined);
        (shown, previous.map(|previous| previous.channel_id), update)
    };
    let channels: Vec<i64> = previous_channel.into_iter().chain([channel_id]).collect();
    announce(ctx.state, shown.clone(), &channels);
    if let Some(update) = update {
        send_server_update(ctx.state, ctx.session_id, update);
    }
    Ok(Response::VoiceState(shown))
}

/// Leaves voice, from whichever session holds the voice state.
fn leave(ctx: &Ctx<'_>) -> proto::VoiceState {
    let removed = ctx.state.voice().remove(ctx.user_id);
    match removed {
        Some(previous) => {
            let shown = states::left(ctx.user_id, &previous.session_id);
            announce(ctx.state, shown.clone(), &[previous.channel_id]);
            shown
        }
        None => states::left(ctx.user_id, ctx.session_id),
    }
}

pub async fn server_mute(
    ctx: &Ctx<'_>,
    request: proto::ServerMuteMember,
) -> Result<Response, ApiError> {
    moderate(
        ctx,
        request.user_id,
        Permissions::MUTE_MEMBERS,
        |moderation| moderation.mute = request.value,
    )
    .await
}

pub async fn server_deafen(
    ctx: &Ctx<'_>,
    request: proto::ServerDeafenMember,
) -> Result<Response, ApiError> {
    moderate(
        ctx,
        request.user_id,
        Permissions::DEAFEN_MEMBERS,
        |moderation| moderation.deaf = request.value,
    )
    .await
}

async fn moderate(
    ctx: &Ctx<'_>,
    user_id: i64,
    needed: Permissions,
    change: impl FnOnce(&mut Moderation),
) -> Result<Response, ApiError> {
    let _writes = ctx.state.write_lock().await;
    let (shown, channel_id) = {
        let guild = ctx.state.guild();
        let mut voice_states = ctx.state.voice();
        let current = connected(&voice_states, user_id)?;
        require_channel(&guild, ctx.user_id, current.channel_id, needed)?;
        if user_id != ctx.user_id {
            require_outranks_member(&guild, ctx.user_id, user_id)?;
        }
        let mut moderation = voice_states.moderation(user_id);
        change(&mut moderation);
        voice_states.set_moderation(user_id, moderation);
        (voice_states.to_proto(&current), current.channel_id)
    };
    announce(ctx.state, shown.clone(), &[channel_id]);
    Ok(Response::VoiceState(shown))
}

pub async fn move_member(ctx: &Ctx<'_>, request: proto::MoveMember) -> Result<Response, ApiError> {
    let _writes = ctx.state.write_lock().await;
    let (moved, from, update) = {
        let guild = ctx.state.guild();
        let mut voice_states = ctx.state.voice();
        let current = connected(&voice_states, request.user_id)?;
        require_channel(
            &guild,
            ctx.user_id,
            current.channel_id,
            Permissions::MOVE_MEMBERS,
        )?;
        let destination = require_channel(
            &guild,
            ctx.user_id,
            request.channel_id,
            Permissions::MOVE_MEMBERS,
        )?;
        if destination.kind != ChannelKind::Voice {
            return Err(ApiError::invalid_argument("that is not a voice channel"));
        }
        if request.user_id != ctx.user_id {
            require_outranks_member(&guild, ctx.user_id, request.user_id)?;
        }
        if current.channel_id == destination.id {
            return Ok(Response::VoiceState(voice_states.to_proto(&current)));
        }
        let can_join = guild
            .channel_permissions(request.user_id, destination.id)
            .contains(Permissions::VIEW_CHANNEL | Permissions::CONNECT);
        if !can_join {
            return Err(ApiError::forbidden("that member cannot join that channel"));
        }
        let cap = voice::config(ctx.state).max_participants_per_channel;
        let present = voice_states.in_channel(destination.id).count();
        if u32::try_from(present).unwrap_or(u32::MAX) >= cap {
            return Err(ApiError::voice_channel_full());
        }
        let moved = VoiceState {
            channel_id: destination.id,
            suppress: suppressed(&guild, request.user_id, destination.id),
            ..current.clone()
        };
        let update =
            server_update(ctx.state, &guild, &voice_states, &moved).ok_or_else(no_voice_node)?;
        voice_states.put(moved.clone());
        (moved, current.channel_id, update)
    };
    let shown = ctx.state.voice().to_proto(&moved);
    announce(ctx.state, shown.clone(), &[from, moved.channel_id]);
    send_server_update(ctx.state, &moved.session_id, update);
    Ok(Response::VoiceState(shown))
}

pub async fn disconnect_member(
    ctx: &Ctx<'_>,
    request: proto::DisconnectMember,
) -> Result<Response, ApiError> {
    let _writes = ctx.state.write_lock().await;
    let removed = {
        let guild = ctx.state.guild();
        let mut voice_states = ctx.state.voice();
        let current = connected(&voice_states, request.user_id)?;
        require_channel(
            &guild,
            ctx.user_id,
            current.channel_id,
            Permissions::MOVE_MEMBERS,
        )?;
        if request.user_id != ctx.user_id {
            require_outranks_member(&guild, ctx.user_id, request.user_id)?;
        }
        voice_states.remove(request.user_id);
        current
    };
    announce(
        ctx.state,
        states::left(removed.user_id, &removed.session_id),
        &[removed.channel_id],
    );
    Ok(ack())
}

/// A fresh token for the session holding the caller's voice state, whose
/// voice connection failed.
pub async fn refresh_voice_server(ctx: &Ctx<'_>) -> Result<Response, ApiError> {
    ctx.state
        .rate_limits
        .check_voice_state(ctx.user_id)
        .map_err(ApiError::rate_limited)?;
    let _writes = ctx.state.write_lock().await;
    let update = {
        let guild = ctx.state.guild();
        let voice_states = ctx.state.voice();
        let current = voice_states
            .get(ctx.user_id)
            .filter(|current| current.session_id == ctx.session_id)
            .cloned()
            .ok_or_else(|| ApiError::voice_not_connected("this session is"))?;
        server_update(ctx.state, &guild, &voice_states, &current).ok_or_else(no_voice_node)?
    };
    send_server_update(ctx.state, ctx.session_id, update);
    Ok(ack())
}

fn no_voice_node() -> ApiError {
    ApiError::conflict("no voice server is available right now")
}

fn connected(voice_states: &states::VoiceStates, user_id: i64) -> Result<VoiceState, ApiError> {
    voice_states
        .get(user_id)
        .cloned()
        .ok_or_else(|| ApiError::voice_not_connected("that member is"))
}
