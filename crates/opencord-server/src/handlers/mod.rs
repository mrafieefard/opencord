//! Request handlers, one module per request group.

use std::sync::Arc;

use opencord_proto::v1 as proto;
use proto::request::Kind;
use proto::response::Result as Response;

use crate::error::ApiError;
use crate::gateway::session::Session;
use crate::state::AppState;

mod channels;
mod commands;
mod domains;
mod invites;
mod lavalink;
mod members;
mod messages;
pub(crate) mod music;
mod network;
mod private;
mod profile;
mod roles;
pub(crate) mod settings;
pub(crate) mod soundboard;
mod storage;
mod streams;
mod uploads;
mod voice;

/// The caller of a request.
pub struct Ctx<'a> {
    pub state: &'a Arc<AppState>,
    pub user_id: i64,
    /// The gateway session the request came on.
    pub session_id: &'a str,
    pub session: &'a Arc<Session>,
}

pub async fn handle(
    state: &Arc<AppState>,
    session: &Arc<Session>,
    request: proto::Request,
) -> Response {
    let ctx = Ctx {
        state,
        user_id: session.user_id,
        session_id: &session.id,
        session,
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
        Kind::UpdateVoiceSettings(request) => settings::update_voice_settings(ctx, request).await,
        Kind::RefreshMediaToken(_) => Ok(settings::refresh_media_token(ctx)),
        Kind::UpdateVoiceState(request) => voice::update_voice_state(ctx, request).await,
        Kind::ServerMuteMember(request) => voice::server_mute(ctx, request).await,
        Kind::ServerDeafenMember(request) => voice::server_deafen(ctx, request).await,
        Kind::MoveMember(request) => voice::move_member(ctx, request).await,
        Kind::DisconnectMember(request) => voice::disconnect_member(ctx, request).await,
        Kind::RefreshVoiceServer(_) => voice::refresh_voice_server(ctx).await,
        Kind::UpdateVoiceActivity(request) => voice::update_voice_activity(ctx, request).await,
        Kind::CreateStream(request) => streams::create(ctx, request).await,
        Kind::UpdateStream(request) => streams::update(ctx, request).await,
        Kind::DeleteStream(request) => streams::delete(ctx, request).await,
        Kind::WatchStream(request) => streams::watch(ctx, request).await,
        Kind::UnwatchStream(request) => streams::unwatch(ctx, request).await,
        Kind::PlaySoundboardSound(request) => soundboard::play(ctx, request).await,
        Kind::UpdateSoundboardSound(request) => soundboard::update(ctx, request).await,
        Kind::DeleteSoundboardSound(request) => soundboard::delete(ctx, request).await,
        Kind::WatchNetwork(request) => network::watch(ctx, &request),
        Kind::SetPublicAddress(request) => network::set_public_address(ctx, request).await,
        Kind::DetectPublicAddress(_) => network::detect_public_address(ctx).await,
        Kind::UpdateNetworkSettings(request) => network::update_settings(ctx, request).await,
        Kind::AddDomain(request) => domains::add(ctx, request).await,
        Kind::UpdateDomain(request) => domains::update(ctx, request).await,
        Kind::DeleteDomain(request) => domains::delete(ctx, request).await,
        Kind::CheckDomain(request) => domains::check(ctx, &request),
        Kind::FetchDomainChecks(request) => domains::history(ctx, request).await,
        Kind::RequestCertificate(request) => domains::request_certificate(ctx, request).await,
        Kind::UploadCertificate(request) => domains::upload_certificate(ctx, request).await,
        Kind::SetDnsProvider(request) => domains::set_dns_provider(ctx, request).await,
        Kind::AddFreeSubdomain(request) => domains::add_free_subdomain(ctx, request).await,
        Kind::StartTunnel(request) => domains::start_tunnel(ctx, request).await,
        Kind::StopTunnel(_) => domains::stop_tunnel(ctx).await,
        Kind::FetchTunnelLog(_) => domains::tunnel_log(ctx).await,
        Kind::InstallCloudflared(_) => domains::install_cloudflared(ctx).await,
        Kind::FetchLavalinkNodes(request) => lavalink::fetch(ctx, &request),
        Kind::CreateLavalinkNode(request) => lavalink::create(ctx, request).await,
        Kind::UpdateLavalinkNode(request) => lavalink::update(ctx, request).await,
        Kind::DeleteLavalinkNode(request) => lavalink::delete(ctx, &request).await,
        Kind::TestLavalinkNode(request) => lavalink::test(ctx, &request),
        Kind::MoveLavalinkPlayers(request) => lavalink::move_players(ctx, &request).await,
        Kind::MusicSubscribe(request) => music::subscribe(ctx, request).await,
        Kind::MusicSearch(request) => music::search(ctx, request).await,
        Kind::MusicEnqueue(request) => music::enqueue(ctx, request).await,
        Kind::MusicPlayback(request) => music::playback(ctx, request).await,
        Kind::MusicSetMode(request) => music::set_mode(ctx, request).await,
        Kind::MusicQueueMove(request) => music::queue_move(ctx, request).await,
        Kind::MusicQueueRemove(request) => music::queue_remove(ctx, request).await,
        Kind::MusicQueueClear(request) => music::queue_clear(ctx, &request).await,
        Kind::MusicVoteSkip(request) => music::vote_skip(ctx, request).await,
        Kind::FetchMusicSession(request) => music::fetch_session(ctx, &request).await,
        Kind::FetchMusicSettings(_) => Ok(music::fetch_settings(ctx)),
        Kind::UpdateMusicSettings(request) => music::update_settings(ctx, request).await,
        Kind::FetchMusicBlocks(_) => music::fetch_blocks(ctx),
        Kind::MusicBlockMember(request) => music::block_member(ctx, request).await,
        Kind::ExecuteCommand(request) => commands::execute(ctx, request).await,
        Kind::AutocompleteCommand(request) => commands::autocomplete(ctx, request).await,
        Kind::FetchCommandSettings(_) => commands::fetch_settings(ctx),
        Kind::UpdateCommandSettings(request) => commands::update_settings(ctx, request).await,
        Kind::CreateUpload(request) => uploads::create(ctx, request).await,
        Kind::CancelUpload(request) => uploads::cancel(ctx, request).await,
        Kind::UpdateUploadSettings(request) => uploads::update_settings(ctx, request).await,
        Kind::GetStorage(_) => storage::get(ctx).await,
        Kind::AddStorage(request) => storage::add(ctx, request).await,
        Kind::UpdateStorage(request) => storage::update(ctx, request).await,
        Kind::TestStorage(request) => storage::test(ctx, request).await,
        Kind::ActivateStorage(request) => storage::activate(ctx, request).await,
        Kind::RemoveStorage(request) => storage::remove(ctx, request).await,
        Kind::StartStorageMove(_) => storage::start_move(ctx).await,
        Kind::StopStorageMove(_) => storage::stop_move(ctx).await,
        Kind::RegisterDevice(request) => private::register_device(ctx, request).await,
        Kind::UploadKeys(request) => private::upload_keys(ctx, request).await,
        Kind::UnregisterDevice(_) => private::unregister_device(ctx).await,
        Kind::FetchDevices(request) => private::fetch_devices(ctx, request).await,
        Kind::ClaimKeys(request) => private::claim_keys(ctx, request).await,
        Kind::UpdatePrivacy(request) => private::update_privacy(ctx, request).await,
        Kind::FetchPendingEvents(_)
        | Kind::AckPendingEvents(_)
        | Kind::SendPrivateMessage(_)
        | Kind::StartPrivateTyping(_)
        | Kind::SendFriendRequest(_)
        | Kind::AnswerFriendRequest(_)
        | Kind::CancelFriendRequest(_)
        | Kind::RemoveFriend(_) => Err(ApiError::new(
            proto::ErrorCode::PrivateMessagesUnavailable,
            "private messages are not yet available",
        )),
    }
}

fn ack() -> Response {
    Response::Ack(proto::Ack {})
}
