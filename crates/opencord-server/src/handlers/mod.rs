//! Request handlers, one module per request group.

use std::sync::Arc;

use opencord_proto::v1 as proto;
use proto::request::Kind;
use proto::response::Result as Response;

use crate::error::ApiError;
use crate::gateway::session::Session;
use crate::state::AppState;

mod channels;
mod invites;
mod members;
mod messages;
mod profile;
mod roles;
mod settings;

/// The caller of a request.
pub struct Ctx<'a> {
    pub state: &'a Arc<AppState>,
    pub user_id: i64,
}

pub async fn handle(
    state: &Arc<AppState>,
    session: &Arc<Session>,
    request: proto::Request,
) -> Response {
    let ctx = Ctx {
        state,
        user_id: session.user_id,
    };
    let result = match state.rate_limits.check_request(ctx.user_id) {
        Ok(()) => dispatch(&ctx, request).await,
        Err(wait) => Err(ApiError::rate_limited(wait)),
    };
    result.unwrap_or_else(|error| Response::Error(error.to_proto()))
}

async fn dispatch(ctx: &Ctx<'_>, request: proto::Request) -> Result<Response, ApiError> {
    let kind = request
        .kind
        .ok_or_else(|| ApiError::invalid_argument("the request is empty"))?;
    match kind {
        Kind::SendMessage(request) => messages::send(ctx, request).await,
        Kind::EditMessage(request) => messages::edit(ctx, request).await,
        Kind::DeleteMessage(request) => messages::delete(ctx, request).await,
        Kind::FetchMessages(request) => messages::fetch(ctx, request).await,
        Kind::StartTyping(request) => messages::start_typing(ctx, request),
        Kind::CreateChannel(request) => channels::create(ctx, request).await,
        Kind::UpdateChannel(request) => channels::update(ctx, request).await,
        Kind::DeleteChannel(request) => channels::delete(ctx, request).await,
        Kind::ReorderChannels(request) => channels::reorder(ctx, request).await,
        Kind::SetChannelOverwrite(request) => channels::set_overwrite(ctx, request).await,
        Kind::DeleteChannelOverwrite(request) => channels::delete_overwrite(ctx, request).await,
        Kind::CreateRole(request) => roles::create(ctx, request).await,
        Kind::UpdateRole(request) => roles::update(ctx, request).await,
        Kind::DeleteRole(request) => roles::delete(ctx, request).await,
        Kind::ReorderRoles(request) => roles::reorder(ctx, request).await,
        Kind::AddMemberRole(request) => roles::add_to_member(ctx, request).await,
        Kind::RemoveMemberRole(request) => roles::remove_from_member(ctx, request).await,
        Kind::KickMember(request) => members::kick(ctx, request).await,
        Kind::BanMember(request) => members::ban(ctx, request).await,
        Kind::UnbanMember(request) => members::unban(ctx, request).await,
        Kind::FetchBans(_) => members::fetch_bans(ctx).await,
        Kind::UpdateNickname(request) => members::update_nickname(ctx, request).await,
        Kind::UpdateProfile(request) => profile::update_profile(ctx, request).await,
        Kind::UpdatePresence(request) => profile::update_presence(ctx, request),
        Kind::CreateInvite(request) => invites::create(ctx, request).await,
        Kind::FetchInvites(_) => invites::fetch(ctx).await,
        Kind::RevokeInvite(request) => invites::revoke(ctx, request).await,
        Kind::UpdateServer(request) => settings::update_server(ctx, request).await,
    }
}

fn ack() -> Response {
    Response::Ack(proto::Ack {})
}
