// SPDX-License-Identifier: MPL-2.0

//! `StartActivity` and `EndActivity` (plan M3 Task 4, design 2026-11-07
//! §15.2 and §15.4).

use opencord_common::permissions::Permissions;
use opencord_plugins::limits::ACTIVITY_SESSIONS_PER_CALL;
use opencord_proto::v1 as proto;
use proto::response::Result as Response;

use super::{Ctx, ack};
use crate::error::ApiError;
use crate::gateway::session::ACTIVITIES;
use crate::permissions::require_channel;
use crate::plugins::activities::{self, Session};
use crate::state::now_ms;

fn unavailable() -> ApiError {
    ApiError::new(
        proto::ErrorCode::PluginUnavailable,
        "this activity is unavailable",
    )
}

pub async fn start(ctx: &Ctx<'_>, request: proto::StartActivity) -> Result<Response, ApiError> {
    let state = ctx.state;
    // Under the write lock: whatever ends a session (the call emptying,
    // the plugin stopping) waits for it, then finds this one.
    let _writes = state.write_lock().await;
    // An app that never heard of activities never heard of this channel's.
    if !ctx
        .session
        .is_some_and(|session| session.has_feature(ACTIVITIES))
    {
        return Err(ApiError::not_found("channel"));
    }
    let in_call = state
        .voice()
        .get(ctx.user_id)
        .is_some_and(|voice_state| voice_state.channel_id == request.channel_id);
    if !in_call {
        return Err(ApiError::invalid_argument(
            "join the call to start an activity",
        ));
    }
    require_channel(
        &state.guild(),
        ctx.user_id,
        request.channel_id,
        Permissions::START_ACTIVITIES,
    )?;
    let running = state.plugins.get(&request.plugin_id).is_some_and(|handle| {
        handle.state() == proto::PluginState::Running && handle.client.entry().is_some()
    });
    if !running {
        return Err(unavailable());
    }
    let activity = state
        .plugins
        .activities
        .get(&request.plugin_id, &request.activity_key)
        .ok_or_else(unavailable)?;
    let in_this_call = state.plugins.activities.sessions_in(request.channel_id);
    if let Some(session) = in_this_call.iter().find(|session| {
        session.plugin_id == request.plugin_id && session.activity_key == request.activity_key
    }) {
        return Ok(Response::ActivitySession(session.to_proto(Vec::new())));
    }
    if in_this_call.len() >= ACTIVITY_SESSIONS_PER_CALL {
        return Err(ApiError::new(
            proto::ErrorCode::PluginLimit,
            format!("this call already has {ACTIVITY_SESSIONS_PER_CALL} activities"),
        ));
    }
    let session = Session {
        id: state.ids.next_id(),
        channel_id: request.channel_id,
        plugin_id: request.plugin_id,
        activity_key: request.activity_key,
        name: activity.name,
        icon: activity.icon,
        plugin_name: activity.plugin_name,
        starter_id: ctx.user_id,
        started_at_ms: now_ms(),
    };
    Ok(Response::ActivitySession(activities::add(state, session)))
}

pub async fn end(ctx: &Ctx<'_>, request: proto::EndActivity) -> Result<Response, ApiError> {
    let state = ctx.state;
    let _writes = state.write_lock().await;
    let session = {
        let guild = state.guild();
        let session = state
            .plugins
            .activities
            .session(request.session_id)
            .filter(|session| guild.can_view(ctx.user_id, session.channel_id))
            .ok_or_else(|| ApiError::not_found("activity session"))?;
        let manages = guild
            .channel_permissions(ctx.user_id, session.channel_id)
            .contains(Permissions::MANAGE_CHANNELS);
        if session.starter_id != ctx.user_id && !manages {
            return Err(ApiError::forbidden(
                "only its starter or someone who manages the channel ends an activity",
            ));
        }
        session
    };
    activities::end(state, session.id, proto::ActivityEndReason::Ended);
    Ok(ack())
}
