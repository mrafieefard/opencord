// SPDX-License-Identifier: MPL-2.0

//! `OpenView`, `CloseView` and `SendViewEvent` (plan M2 Task 10, design
//! 2026-11-07 §14.3); views in activity sessions (plan M3 Task 5, §15.3).

use std::sync::Arc;

use opencord_common::channel::ChannelKind;
use opencord_common::permissions::Permissions;
use opencord_plugins::limits::{
    VIEW_ERROR_REPORT_MAX_CHARS, VIEW_EVENT_MAX_BYTES, VIEWS_PER_SESSION,
};
use opencord_proto::v1 as proto;
use proto::response::Result as Response;

use super::{Ctx, ack};
use crate::error::ApiError;
use crate::gateway::session::{ACTIVITIES, PLUGIN_CHANNELS, Session};
use crate::permissions::require_channel;
use crate::plugins::PluginHandle;
use crate::plugins::activities;
use crate::plugins::assets::client_files;
use crate::plugins::views::{self, ViewInfo, valid_event_name};

fn session<'a>(ctx: &Ctx<'a>) -> Result<&'a Arc<Session>, ApiError> {
    ctx.session
        .ok_or_else(|| ApiError::invalid_argument("only members' apps open views"))
}

fn unavailable(message: &str) -> ApiError {
    ApiError::new(proto::ErrorCode::PluginUnavailable, message)
}

pub async fn open(ctx: &Ctx<'_>, request: proto::OpenView) -> Result<Response, ApiError> {
    let session = session(ctx)?;
    let place = request
        .place
        .and_then(|place| place.kind)
        .ok_or_else(|| ApiError::invalid_argument("the view's place is missing"))?;
    if !matches!(request.platform.as_str(), "desktop" | "phone") {
        return Err(ApiError::invalid_argument(
            "platform must be desktop or phone",
        ));
    }
    match place {
        proto::view_place::Kind::ChannelId(channel_id) => {
            open_channel(ctx, session, channel_id, request.platform).await
        }
        proto::view_place::Kind::ActivitySessionId(activity_session_id) => {
            open_activity(ctx, session, activity_session_id, request.platform).await
        }
    }
}

/// A view of a plugin channel.
async fn open_channel(
    ctx: &Ctx<'_>,
    session: &Session,
    channel_id: i64,
    platform: String,
) -> Result<Response, ApiError> {
    // An app without plugin channels never heard of this one.
    if !session.has_feature(PLUGIN_CHANNELS) {
        return Err(ApiError::not_found("channel"));
    }
    let state = ctx.state;
    // Under the write lock: whatever takes access away waits for it, then
    // finds this view.
    let _writes = state.write_lock().await;
    if session.is_ended() {
        return Err(ApiError::invalid_session());
    }
    let plugin = {
        let guild = state.guild();
        let channel = require_channel(&guild, ctx.user_id, channel_id, Permissions::VIEW_CHANNEL)?;
        channel
            .plugin
            .clone()
            .filter(|_| channel.kind == ChannelKind::Plugin)
            .ok_or_else(|| ApiError::invalid_argument("only plugin channels open views"))?
    };
    if state.plugins.channel_types.state(&plugin) != proto::PluginChannelState::Available {
        return Err(unavailable("this channel is unavailable"));
    }
    let handle = state
        .plugins
        .get(&plugin.plugin_id)
        .ok_or_else(|| unavailable("this channel is unavailable"))?;
    let entry = handle
        .client
        .entry()
        .ok_or_else(|| unavailable("this plugin has no screen"))?
        .to_owned();
    insert(
        ctx,
        session,
        Opening {
            handle,
            entry,
            channel_id,
            activity: None,
            platform,
        },
    )
}

/// A view of an activity session, for a member in its call (plan M3 Task 5,
/// design 2026-11-07 §15.3).
async fn open_activity(
    ctx: &Ctx<'_>,
    session: &Session,
    activity_session_id: i64,
    platform: String,
) -> Result<Response, ApiError> {
    // An app without activities never heard of this one.
    if !session.has_feature(ACTIVITIES) {
        return Err(ApiError::not_found("activity session"));
    }
    let state = ctx.state;
    // Under the write lock: leaving the call and the session's end wait
    // for it, then find this view.
    let _writes = state.write_lock().await;
    if session.is_ended() {
        return Err(ApiError::invalid_session());
    }
    let activity = state
        .plugins
        .activities
        .session(activity_session_id)
        .ok_or_else(|| ApiError::not_found("activity session"))?;
    let in_call = state
        .voice()
        .get(ctx.user_id)
        .is_some_and(|voice_state| voice_state.channel_id == activity.channel_id);
    if !in_call {
        return Err(ApiError::forbidden("join the call to open this activity"));
    }
    let handle = state
        .plugins
        .get(&activity.plugin_id)
        .filter(|handle| handle.state() == proto::PluginState::Running)
        .ok_or_else(|| unavailable("this activity is unavailable"))?;
    let entry = handle
        .client
        .entry()
        .ok_or_else(|| unavailable("this activity is unavailable"))?
        .to_owned();
    insert(
        ctx,
        session,
        Opening {
            handle,
            entry,
            channel_id: activity.channel_id,
            activity: Some(activity.id),
            platform,
        },
    )
}

/// A view whose access was checked, under the write lock.
struct Opening {
    handle: Arc<PluginHandle>,
    entry: String,
    /// The plugin channel, or the activity session's voice channel.
    channel_id: i64,
    activity: Option<i64>,
    platform: String,
}

/// Opens the view within the session's limit, tells its plugin and, when
/// it adds a viewer to an activity session, everyone who sees the session.
fn insert(ctx: &Ctx<'_>, session: &Session, opening: Opening) -> Result<Response, ApiError> {
    let state = ctx.state;
    if state.plugins.views.count_for_session(&session.id) >= VIEWS_PER_SESSION {
        return Err(ApiError::new(
            proto::ErrorCode::PluginLimit,
            "too many open views",
        ));
    }
    // A dev token works only while its maker manages plugins (dev.rs), so
    // holding one adds no one to who sees errors.
    let show_errors = state
        .guild()
        .base_permissions(ctx.user_id)
        .contains(Permissions::MANAGE_PLUGINS);
    let Opening {
        handle,
        entry,
        channel_id,
        activity,
        platform,
    } = opening;
    let new_viewer = activity.is_some_and(|session_id| {
        !state
            .plugins
            .views
            .viewers_of(session_id)
            .contains(&ctx.user_id)
    });
    let view = ViewInfo {
        id: state.ids.next_id(),
        plugin_id: handle.id.clone(),
        user_id: ctx.user_id,
        session_id: session.id.clone(),
        channel_id,
        activity,
        platform,
    };
    state.plugins.views.insert(view.clone());
    if let Some(session_id) = activity.filter(|_| new_viewer) {
        activities::viewers_changed(state, session_id);
    }
    views::opened(state, &view);
    Ok(Response::ViewOpened(proto::ViewOpened {
        view_id: view.id,
        plugin_id: view.plugin_id,
        api: handle.client.api(),
        entry,
        files: client_files(&handle),
        show_errors,
    }))
}

pub fn close(ctx: &Ctx<'_>, request: &proto::CloseView) -> Result<Response, ApiError> {
    let session = session(ctx)?;
    if views::close_own(ctx.state, &session.id, request.view_id) {
        Ok(ack())
    } else {
        Err(ApiError::not_found("view"))
    }
}

/// Not counted against the request rate: the view's own limits apply.
pub fn send_event(ctx: &Ctx<'_>, request: proto::SendViewEvent) -> Result<Response, ApiError> {
    let session = session(ctx)?;
    if !valid_event_name(&request.name) {
        return Err(ApiError::invalid_argument(
            "name must be 1 to 64 of a-z, 0-9, _, ., : and -",
        ));
    }
    if request.data_json.len() > VIEW_EVENT_MAX_BYTES {
        return Err(ApiError::invalid_argument(format!(
            "data must be at most {} KiB of JSON",
            VIEW_EVENT_MAX_BYTES / 1024
        )));
    }
    let bytes = (request.name.len() + request.data_json.len()) as u64;
    let view = match ctx
        .state
        .plugins
        .views
        .take_from(request.view_id, &session.id, bytes)
    {
        Some(Ok(view)) => view,
        Some(Err(views::Limited(wait))) => return Err(ApiError::rate_limited(wait)),
        None => return Err(ApiError::not_found("view")),
    };
    let data = if request.data_json.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(&request.data_json)
            .map_err(|_| ApiError::invalid_argument("data must be JSON"))?
    };
    views::from_client(ctx.state, &view, request.name, data);
    Ok(ack())
}

/// Reports a client-side view error to the plugin's log (at most 1 per view
/// every 10 s, text cut to 2,000 characters).
pub fn report_error(ctx: &Ctx<'_>, request: proto::ReportViewError) -> Result<Response, ApiError> {
    let session = session(ctx)?;
    if let Some(view) = ctx
        .state
        .plugins
        .views
        .report_error(request.view_id, &session.id)?
    {
        let user_name = ctx.state.guild().member_name(view.user_id);
        let sanitized: String = request
            .text
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .chars()
            .take(VIEW_ERROR_REPORT_MAX_CHARS)
            .map(|c| if c.is_control() && c != '\n' { ' ' } else { c })
            .collect();
        let indented = sanitized.replace('\n', "\n    ");
        let log_msg = format!("view error for {user_name}: {indented}");
        if let Some(handle) = ctx.state.plugins.get(&view.plugin_id) {
            handle.log().push(proto::PluginLogLevel::Error, &log_msg);
        }
    }
    Ok(ack())
}
