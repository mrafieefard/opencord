// SPDX-License-Identifier: MPL-2.0

//! `OpenView`, `CloseView` and `SendViewEvent` (plan M2 Task 10, design
//! 2026-11-07 §14.3).

use opencord_common::channel::ChannelKind;
use opencord_common::permissions::Permissions;
use opencord_plugins::limits::{
    VIEW_ERROR_REPORT_MAX_CHARS, VIEW_EVENT_MAX_BYTES, VIEWS_PER_SESSION,
};
use opencord_proto::v1 as proto;
use proto::response::Result as Response;

use super::{Ctx, ack};
use crate::error::ApiError;
use crate::gateway::session::{PLUGIN_CHANNELS, Session};
use crate::permissions::require_channel;
use crate::plugins::assets::client_files;
use crate::plugins::views::{self, ViewInfo, valid_event_name};

fn session<'a>(ctx: &Ctx<'a>) -> Result<&'a std::sync::Arc<Session>, ApiError> {
    ctx.session
        .ok_or_else(|| ApiError::invalid_argument("only members' apps open views"))
}

fn unavailable(message: &str) -> ApiError {
    ApiError::new(proto::ErrorCode::PluginUnavailable, message)
}

pub async fn open(ctx: &Ctx<'_>, request: proto::OpenView) -> Result<Response, ApiError> {
    let session = session(ctx)?;
    let channel_id = match request.place.and_then(|place| place.kind) {
        Some(proto::view_place::Kind::ChannelId(channel_id)) => channel_id,
        Some(proto::view_place::Kind::ActivitySessionId(_)) => {
            return Err(ApiError::invalid_argument("not implemented"));
        }
        None => return Err(ApiError::invalid_argument("the view's place is missing")),
    };
    if !matches!(request.platform.as_str(), "desktop" | "phone") {
        return Err(ApiError::invalid_argument(
            "platform must be desktop or phone",
        ));
    }
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
    let (plugin, show_errors) = {
        let guild = state.guild();
        let channel = require_channel(&guild, ctx.user_id, channel_id, Permissions::VIEW_CHANNEL)?;
        let plugin = channel
            .plugin
            .clone()
            .filter(|_| channel.kind == ChannelKind::Plugin)
            .ok_or_else(|| ApiError::invalid_argument("only plugin channels open views"))?;
        // A dev token works only while its maker manages plugins (dev.rs),
        // so holding one adds no one to who sees errors.
        let show_errors = guild
            .base_permissions(ctx.user_id)
            .contains(Permissions::MANAGE_PLUGINS);
        (plugin, show_errors)
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
    if state.plugins.views.count_for_session(&session.id) >= VIEWS_PER_SESSION {
        return Err(ApiError::new(
            proto::ErrorCode::PluginLimit,
            "too many open views",
        ));
    }
    let view = ViewInfo {
        id: state.ids.next_id(),
        plugin_id: plugin.plugin_id.clone(),
        user_id: ctx.user_id,
        session_id: session.id.clone(),
        channel_id,
        platform: request.platform,
    };
    state.plugins.views.insert(view.clone());
    views::opened(state, &view);
    Ok(Response::ViewOpened(proto::ViewOpened {
        view_id: view.id,
        plugin_id: plugin.plugin_id,
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
