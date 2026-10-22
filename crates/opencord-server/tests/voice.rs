//! Voice states on the main gateway (Phase 2 plan V0): joining, leaving,
//! limits, moderation and settings, without media.

mod common;

use std::time::Duration;

use common::{TestClient, TestServer, channel_id, invite, join, owner, self_id};
use opencord_common::permissions::Permissions;
use opencord_proto::v1 as proto;
use opencord_voice::token::{self, Presenter};
use proto::event::Kind as Event;
use proto::request::Kind as Request;
use proto::response::Result as Response;

const QUIET: Duration = Duration::from_millis(300);

struct Room {
    server: TestServer,
    owner: TestClient,
    owner_id: i64,
    member: TestClient,
    member_id: i64,
    voice: i64,
    invite: String,
}

async fn room() -> Room {
    let server = TestServer::start().await;
    let (mut owner, owner_ready) = owner(&server).await;
    let invite = invite(&mut owner, None).await;
    let (member, member_ready) = join(&server, 2, &invite).await;
    Room {
        voice: channel_id(&owner_ready, "General"),
        owner_id: self_id(&owner_ready),
        member_id: self_id(&member_ready),
        server,
        owner,
        member,
        invite,
    }
}

fn join_voice(channel_id: i64) -> Request {
    Request::UpdateVoiceState(proto::UpdateVoiceState {
        channel_id: Some(channel_id),
        ..Default::default()
    })
}

fn leave_voice() -> Request {
    Request::UpdateVoiceState(proto::UpdateVoiceState::default())
}

fn voice_state(result: Response) -> proto::VoiceState {
    match result {
        Response::VoiceState(state) => state,
        other => panic!("expected a voice state, got {other:?}"),
    }
}

fn code(error: &proto::Error) -> proto::ErrorCode {
    proto::ErrorCode::try_from(error.code).unwrap()
}

/// The next voice state announced for `user_id`.
async fn state_of(client: &mut TestClient, user_id: i64) -> proto::VoiceState {
    client
        .wait_for(|event| match event {
            Event::VoiceStateUpdate(proto::VoiceStateUpdate {
                voice_state: Some(state),
            }) if state.user_id == user_id => Some(state.clone()),
            _ => None,
        })
        .await
}

async fn server_update(client: &mut TestClient) -> proto::VoiceServerUpdate {
    client
        .wait_for(|event| match event {
            Event::VoiceServerUpdate(update) => Some(update.clone()),
            _ => None,
        })
        .await
}

fn is_voice_event(event: &Event) -> bool {
    matches!(
        event,
        Event::VoiceStateUpdate(_) | Event::VoiceServerUpdate(_)
    )
}

async fn create_voice_channel(client: &mut TestClient, name: &str) -> i64 {
    match client
        .ok(Request::CreateChannel(proto::CreateChannel {
            kind: proto::ChannelKind::Voice as i32,
            name: name.to_owned(),
            topic: None,
            parent_id: None,
        }))
        .await
    {
        Response::Channel(channel) => channel.id,
        other => panic!("expected a channel, got {other:?}"),
    }
}

fn deny_everyone(channel_id: i64, everyone_role_id: i64, deny: Permissions) -> Request {
    Request::SetChannelOverwrite(proto::SetChannelOverwrite {
        channel_id,
        overwrite: Some(proto::PermissionOverwrite {
            target_kind: proto::OverwriteTarget::Role as i32,
            target_id: everyone_role_id,
            allow: 0,
            deny: deny.bits(),
        }),
    })
}

fn everyone_role(ready: &proto::Ready) -> i64 {
    ready.server.as_ref().unwrap().everyone_role_id
}

#[tokio::test]
async fn members_who_can_see_the_channel_see_people_join_and_leave() {
    let mut room = room().await;
    let (mut outsider, outsider_ready) = join(&room.server, 3, &room.invite).await;
    room.owner
        .ok(Request::SetChannelOverwrite(proto::SetChannelOverwrite {
            channel_id: room.voice,
            overwrite: Some(proto::PermissionOverwrite {
                target_kind: proto::OverwriteTarget::Member as i32,
                target_id: self_id(&outsider_ready),
                allow: 0,
                deny: Permissions::VIEW_CHANNEL.bits(),
            }),
        }))
        .await;

    let joined = voice_state(room.member.ok(join_voice(room.voice)).await);

    assert_eq!(joined.channel_id, Some(room.voice));
    assert_eq!(state_of(&mut room.owner, room.member_id).await, joined);
    assert_eq!(state_of(&mut room.member, room.member_id).await, joined);
    outsider.assert_no_event(QUIET, is_voice_event).await;

    let left = voice_state(room.member.ok(leave_voice()).await);

    assert_eq!(left.channel_id, None);
    assert_eq!(
        state_of(&mut room.owner, room.member_id).await.channel_id,
        None
    );
    outsider.assert_no_event(QUIET, is_voice_event).await;
}

#[tokio::test]
async fn joining_sends_a_voice_token_for_that_session_and_channel() {
    let mut room = room().await;
    let joined = voice_state(room.member.ok(join_voice(room.voice)).await);

    let update = server_update(&mut room.member).await;

    assert_eq!(update.channel_id, room.voice);
    assert_eq!(update.endpoint, "");
    assert_eq!(
        update.certificate_fingerprint,
        room.server.handle.fingerprint.to_vec()
    );
    let presenter = Presenter {
        user_id: room.member_id,
        session_id: &joined.session_id,
        channel_id: room.voice,
    };
    let claims = token::verify(
        &room.server.handle.voice_key,
        &update.token,
        presenter,
        i64::try_from(common::now_ms()).unwrap(),
    )
    .unwrap();
    assert!(Permissions::from_bits_truncate(claims.permissions).contains(Permissions::CONNECT));
    assert_eq!(claims.limits.unwrap().voice_bitrate, 64_000);
    room.owner
        .assert_no_event(QUIET, |event| matches!(event, Event::VoiceServerUpdate(_)))
        .await;
}

#[tokio::test]
async fn ready_shows_who_is_in_voice_and_the_voice_settings() {
    let mut room = room().await;
    room.owner.ok(join_voice(room.voice)).await;

    let (_late, ready) = join(&room.server, 3, &room.invite).await;

    assert!(ready.voice_enabled);
    let states: Vec<(i64, Option<i64>)> = ready
        .voice_states
        .iter()
        .map(|state| (state.user_id, state.channel_id))
        .collect();
    assert_eq!(states, [(room.owner_id, Some(room.voice))]);
    let settings = ready.voice_settings.unwrap();
    assert_eq!(settings.max_voice_bitrate, 96_000);
    assert!(!ready.media_token.unwrap().token.is_empty());
    let voice = ready
        .channels
        .iter()
        .find(|channel| channel.id == room.voice)
        .unwrap();
    assert_eq!(
        (voice.bitrate, voice.user_limit, voice.text_in_voice),
        (64_000, 0, true)
    );
}

#[tokio::test]
async fn joining_needs_connect() {
    let mut room = room().await;
    let ready_everyone = {
        let (_, ready) = join(&room.server, 3, &room.invite).await;
        everyone_role(&ready)
    };
    room.owner
        .ok(deny_everyone(
            room.voice,
            ready_everyone,
            Permissions::CONNECT,
        ))
        .await;

    let error = room.member.error(join_voice(room.voice)).await;

    assert_eq!(code(&error), proto::ErrorCode::Forbidden);
    room.owner.assert_no_event(QUIET, is_voice_event).await;
}

#[tokio::test]
async fn the_user_limit_holds_and_move_members_bypasses_it() {
    let mut room = room().await;
    room.owner
        .ok(Request::UpdateChannel(proto::UpdateChannel {
            channel_id: room.voice,
            user_limit: Some(1),
            ..Default::default()
        }))
        .await;
    let (mut third, _) = join(&room.server, 3, &room.invite).await;
    room.member.ok(join_voice(room.voice)).await;

    let error = third.error(join_voice(room.voice)).await;
    let owner = room.owner.request(join_voice(room.voice)).await;

    assert_eq!(code(&error), proto::ErrorCode::VoiceChannelFull);
    assert_eq!(voice_state(owner).channel_id, Some(room.voice));
}

#[tokio::test]
async fn joining_from_another_session_takes_the_voice_state_over() {
    let mut room = room().await;
    let first = voice_state(room.member.ok(join_voice(room.voice)).await);
    let (mut second, _) = {
        let mut client = TestClient::connect(&room.server, common::key(2)).await;
        let ready = client.identify("user2", None, None).await.unwrap();
        (client, ready)
    };

    let taken = voice_state(second.ok(join_voice(room.voice)).await);

    assert_ne!(taken.session_id, first.session_id);
    assert_eq!(server_update(&mut second).await.channel_id, room.voice);
    let seen = state_of(&mut room.member, room.member_id).await;
    let seen = if seen.session_id == first.session_id {
        state_of(&mut room.member, room.member_id).await
    } else {
        seen
    };
    assert_eq!(seen.session_id, taken.session_id);
}

#[tokio::test]
async fn moderators_mute_deafen_move_and_disconnect() {
    let mut room = room().await;
    let afk = create_voice_channel(&mut room.owner, "Lounge").await;
    let not_connected = room
        .owner
        .error(Request::ServerMuteMember(proto::ServerMuteMember {
            user_id: room.member_id,
            value: true,
        }))
        .await;
    assert_eq!(code(&not_connected), proto::ErrorCode::VoiceNotConnected);
    room.member.ok(join_voice(room.voice)).await;
    room.owner.ok(join_voice(room.voice)).await;

    let muted = voice_state(
        room.owner
            .ok(Request::ServerMuteMember(proto::ServerMuteMember {
                user_id: room.member_id,
                value: true,
            }))
            .await,
    );
    let deafened = voice_state(
        room.owner
            .ok(Request::ServerDeafenMember(proto::ServerDeafenMember {
                user_id: room.member_id,
                value: true,
            }))
            .await,
    );
    assert!(muted.server_mute);
    assert!(deafened.server_mute && deafened.server_deaf);
    let refused = room
        .member
        .error(Request::ServerMuteMember(proto::ServerMuteMember {
            user_id: room.owner_id,
            value: true,
        }))
        .await;
    assert_eq!(code(&refused), proto::ErrorCode::Forbidden);

    let moved = voice_state(
        room.owner
            .ok(Request::MoveMember(proto::MoveMember {
                user_id: room.member_id,
                channel_id: afk,
            }))
            .await,
    );
    assert_eq!(moved.channel_id, Some(afk));
    assert!(moved.server_mute, "server mute follows the member");
    let moved_to = room
        .member
        .wait_for(|event| match event {
            Event::VoiceServerUpdate(update) if update.channel_id == afk => Some(update.clone()),
            _ => None,
        })
        .await;
    assert!(!moved_to.token.is_empty());

    room.owner
        .ok(Request::DisconnectMember(proto::DisconnectMember {
            user_id: room.member_id,
        }))
        .await;
    loop {
        if state_of(&mut room.member, room.member_id)
            .await
            .channel_id
            .is_none()
        {
            break;
        }
    }
}

#[tokio::test]
async fn losing_sight_of_the_channel_or_the_server_ends_the_voice_state() {
    let mut room = room().await;
    let (mut third, third_ready) = join(&room.server, 3, &room.invite).await;
    let third_id = self_id(&third_ready);
    room.member.ok(join_voice(room.voice)).await;
    third.ok(join_voice(room.voice)).await;

    room.owner
        .ok(Request::SetChannelOverwrite(proto::SetChannelOverwrite {
            channel_id: room.voice,
            overwrite: Some(proto::PermissionOverwrite {
                target_kind: proto::OverwriteTarget::Member as i32,
                target_id: room.member_id,
                allow: 0,
                deny: Permissions::VIEW_CHANNEL.bits(),
            }),
        }))
        .await;
    loop {
        if state_of(&mut room.owner, room.member_id)
            .await
            .channel_id
            .is_none()
        {
            break;
        }
    }

    room.owner
        .ok(Request::KickMember(proto::KickMember {
            user_id: third_id,
            reason: None,
        }))
        .await;
    loop {
        if state_of(&mut room.owner, third_id)
            .await
            .channel_id
            .is_none()
        {
            break;
        }
    }
}

#[tokio::test]
async fn voice_settings_are_checked_saved_and_announced() {
    let mut room = room().await;
    let afk = create_voice_channel(&mut room.owner, "AFK").await;
    room.member.ok(join_voice(afk)).await;

    let refused = room
        .member
        .error(Request::UpdateVoiceSettings(proto::UpdateVoiceSettings {
            camera_allowed: Some(false),
            ..Default::default()
        }))
        .await;
    let invalid = room
        .owner
        .error(Request::UpdateVoiceSettings(proto::UpdateVoiceSettings {
            screen_share_max_fps: Some(24),
            ..Default::default()
        }))
        .await;
    let saved = room
        .owner
        .ok(Request::UpdateVoiceSettings(proto::UpdateVoiceSettings {
            camera_allowed: Some(false),
            afk_channel_id: Some(afk),
            ..Default::default()
        }))
        .await;

    assert_eq!(code(&refused), proto::ErrorCode::Forbidden);
    assert_eq!(code(&invalid), proto::ErrorCode::InvalidArgument);
    let Response::VoiceSettings(saved) = saved else {
        panic!("expected voice settings");
    };
    assert!(!saved.camera_allowed);
    let announced = room
        .member
        .wait_for(|event| match event {
            Event::VoiceSettingsUpdate(update) => update.settings,
            _ => None,
        })
        .await;
    assert_eq!(announced, saved);
    assert!(state_of(&mut room.member, room.member_id).await.suppress);
}

#[tokio::test]
async fn voice_channel_settings_are_checked() {
    let mut room = room().await;
    let (_, ready) = join(&room.server, 3, &room.invite).await;
    let general = channel_id(&ready, "general");
    let update = |channel_id, bitrate, user_limit| {
        Request::UpdateChannel(proto::UpdateChannel {
            channel_id,
            bitrate,
            user_limit,
            ..Default::default()
        })
    };

    let too_fast = room
        .owner
        .error(update(room.voice, Some(96_001), None))
        .await;
    let too_many = room.owner.error(update(room.voice, None, Some(100))).await;
    let text = room.owner.error(update(general, Some(64_000), None)).await;
    let saved = room
        .owner
        .ok(Request::UpdateChannel(proto::UpdateChannel {
            channel_id: room.voice,
            bitrate: Some(96_000),
            user_limit: Some(5),
            text_in_voice: Some(false),
            ..Default::default()
        }))
        .await;

    for error in [too_fast, too_many, text] {
        assert_eq!(code(&error), proto::ErrorCode::InvalidArgument);
    }
    let Response::Channel(saved) = saved else {
        panic!("expected a channel");
    };
    assert_eq!(
        (saved.bitrate, saved.user_limit, saved.text_in_voice),
        (96_000, 5, false)
    );
}

#[tokio::test]
async fn the_media_endpoints_need_a_media_token() {
    let server = TestServer::start().await;
    let (_owner, ready) = owner(&server).await;
    let token = ready.media_token.unwrap().token;
    let sha = "ab".repeat(32);
    let sounds = server.dir.path().join("data").join("sounds");
    std::fs::create_dir_all(&sounds).unwrap();
    std::fs::write(sounds.join(format!("{}.opus", "cd".repeat(32))), b"OggS").unwrap();
    let path = |hash: &str| format!("/media/sounds/{hash}");

    let (anonymous, _) = server.get(&path(&sha)).await;
    let (forged, _) = server
        .get_with(&path(&sha), &[("Authorization", "Bearer forged")])
        .await;
    let bearer = format!("Bearer {token}");
    let (missing, _) = server
        .get_with(&path(&sha), &[("Authorization", &bearer)])
        .await;
    let (bad_hash, _) = server
        .get_with(&path("not-a-hash"), &[("Authorization", &bearer)])
        .await;
    let (found, body) = server
        .get_with(&path(&"cd".repeat(32)), &[("Authorization", &bearer)])
        .await;

    assert_eq!(anonymous, 401);
    assert_eq!(forged, 401);
    assert_eq!(missing, 404);
    assert_eq!(bad_hash, 400);
    assert_eq!((found, body.as_str()), (200, "OggS"));
}

#[tokio::test]
async fn media_tokens_can_be_refreshed() {
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;

    let Response::MediaToken(token) = owner
        .ok(Request::RefreshMediaToken(proto::RefreshMediaToken {}))
        .await
    else {
        panic!("expected a media token");
    };

    let bearer = format!("Bearer {}", token.token);
    let (status, _) = server
        .get_with(
            &format!("/media/sounds/{}", "ab".repeat(32)),
            &[("Authorization", &bearer)],
        )
        .await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn info_says_whether_voice_is_on() {
    let on = TestServer::start().await;
    let off = TestServer::start_with(|config| config.voice.enabled = false).await;

    let (_, on_body) = on.get("/info").await;
    let (_, off_body) = off.get("/info").await;

    assert!(on_body.contains("\"voice_enabled\":true"), "{on_body}");
    assert!(
        !on_body.contains("\"voice_udp_port\":null"),
        "the port it bound: {on_body}"
    );
    assert!(off_body.contains("\"voice_enabled\":false"), "{off_body}");
    assert!(off_body.contains("\"voice_udp_port\":null"), "{off_body}");
}

#[tokio::test]
async fn the_session_in_voice_can_ask_for_a_fresh_voice_token() {
    let mut room = room().await;
    room.member.ok(join_voice(room.voice)).await;
    let first = server_update(&mut room.member).await;

    room.member
        .ok(Request::RefreshVoiceServer(proto::RefreshVoiceServer {}))
        .await;
    let fresh = server_update(&mut room.member).await;

    assert_eq!(fresh.channel_id, room.voice);
    assert_ne!(fresh.token, first.token);
}

/// Joining the channel this session is already in changes nothing: no new
/// token, so its media connection carries on.
#[tokio::test]
async fn joining_the_channel_the_session_is_in_again_keeps_its_connection() {
    let mut room = room().await;
    room.member.ok(join_voice(room.voice)).await;
    server_update(&mut room.member).await;

    room.member.ok(join_voice(room.voice)).await;
    let again = tokio::time::timeout(QUIET, server_update(&mut room.member)).await;

    assert!(again.is_err(), "a fresh token would reconnect its media");
}

/// Another session of the same user, not in voice, cannot take the voice
/// state over by asking for a token: it joins, which everyone hears of.
#[tokio::test]
async fn another_session_of_the_same_user_gets_no_voice_token() {
    let mut room = room().await;
    room.member.ok(join_voice(room.voice)).await;
    server_update(&mut room.member).await;
    let mut second = TestClient::connect(&room.server, common::key(2)).await;
    second.identify("user2", None, None).await.unwrap();

    let refused = second
        .error(Request::RefreshVoiceServer(proto::RefreshVoiceServer {}))
        .await;

    assert_eq!(code(&refused), proto::ErrorCode::VoiceNotConnected);
}

#[tokio::test]
async fn only_the_session_in_voice_gets_a_voice_token() {
    let mut room = room().await;

    let outside = room
        .member
        .error(Request::RefreshVoiceServer(proto::RefreshVoiceServer {}))
        .await;

    assert_eq!(code(&outside), proto::ErrorCode::VoiceNotConnected);
}

#[tokio::test]
async fn a_voice_channel_holds_a_chat_for_those_who_can_see_it() {
    let mut room = room().await;
    let sent = match room
        .member
        .ok(Request::SendMessage(proto::SendMessage {
            channel_id: room.voice,
            content: "can you hear me?".to_owned(),
            nonce: "n1".to_owned(),
            attachment_ids: Vec::new(),
        }))
        .await
    {
        Response::Message(message) => message,
        other => panic!("expected a message, got {other:?}"),
    };
    assert_eq!(sent.channel_id, room.voice);

    // Everyone who can see the channel gets it, in voice or not.
    let created = room
        .owner
        .wait_for(|event| match event {
            Event::MessageCreate(proto::MessageCreate {
                message: Some(message),
            }) => Some(message.clone()),
            _ => None,
        })
        .await;
    assert_eq!(created.content, "can you hear me?");
    let history = match room
        .owner
        .ok(Request::FetchMessages(proto::FetchMessages {
            channel_id: room.voice,
            before: None,
            limit: 50,
        }))
        .await
    {
        Response::Messages(list) => list.messages,
        other => panic!("expected messages, got {other:?}"),
    };
    assert_eq!(history.len(), 1);

    // Send messages is still needed.
    let everyone = {
        let (_, ready) = join(&room.server, 3, &room.invite).await;
        everyone_role(&ready)
    };
    room.owner
        .ok(deny_everyone(
            room.voice,
            everyone,
            Permissions::SEND_MESSAGES,
        ))
        .await;
    let refused = room
        .member
        .error(Request::SendMessage(proto::SendMessage {
            channel_id: room.voice,
            content: "and now?".to_owned(),
            nonce: "n2".to_owned(),
            attachment_ids: Vec::new(),
        }))
        .await;
    assert_eq!(code(&refused), proto::ErrorCode::Forbidden);
}

#[tokio::test]
async fn a_voice_channel_with_its_chat_off_takes_no_messages() {
    let mut room = room().await;
    room.owner
        .ok(Request::UpdateChannel(proto::UpdateChannel {
            channel_id: room.voice,
            text_in_voice: Some(false),
            ..Default::default()
        }))
        .await;

    let refused = room
        .owner
        .error(Request::SendMessage(proto::SendMessage {
            channel_id: room.voice,
            content: "anyone?".to_owned(),
            nonce: "n1".to_owned(),
            attachment_ids: Vec::new(),
        }))
        .await;

    assert_eq!(code(&refused), proto::ErrorCode::InvalidArgument);
}

fn report_idle(input_idle_ms: Option<u64>) -> Request {
    Request::UpdateVoiceActivity(proto::UpdateVoiceActivity { input_idle_ms })
}

#[tokio::test]
async fn people_idle_for_the_timeout_are_moved_to_the_afk_channel() {
    let mut room = room().await;
    let afk = create_voice_channel(&mut room.owner, "AFK").await;
    room.owner
        .ok(Request::UpdateVoiceSettings(proto::UpdateVoiceSettings {
            afk_channel_id: Some(afk),
            afk_timeout_s: Some(300),
            ..Default::default()
        }))
        .await;
    let (mut third, third_ready) = join(&room.server, 3, &room.invite).await;
    let third_id = self_id(&third_ready);
    for client in [&mut room.owner, &mut room.member, &mut third] {
        voice_state(client.ok(join_voice(room.voice)).await);
    }
    // The member has been away; the owner too, but streaming; the third
    // person said they were away, then came back.
    room.member.ok(report_idle(Some(0))).await;
    room.owner
        .ok(Request::CreateStream(proto::CreateStream {
            channel_id: room.voice,
            source_kind: proto::StreamSourceKind::Screen as i32,
            resolution: proto::ScreenShareResolution::ScreenShareResolution720p as i32,
            fps: 30,
            has_audio: false,
        }))
        .await;
    room.owner.ok(report_idle(Some(0))).await;
    third.ok(report_idle(Some(0))).await;
    third.ok(report_idle(None)).await;
    // The last voice event before the checks: the owner going live.
    room.owner
        .wait_for(|event| match event {
            Event::VoiceStateUpdate(proto::VoiceStateUpdate {
                voice_state: Some(state),
            }) if state.user_id == room.owner_id && state.self_stream => Some(()),
            _ => None,
        })
        .await;

    // Not yet: four minutes.
    room.server
        .handle
        .check_afk_after(Duration::from_secs(240))
        .await;
    room.owner.assert_no_event(QUIET, is_voice_event).await;

    room.server
        .handle
        .check_afk_after(Duration::from_secs(301))
        .await;

    let moved = state_of(&mut room.owner, room.member_id).await;
    assert_eq!(moved.channel_id, Some(afk));
    assert!(moved.suppress, "the AFK channel suppresses");
    // The moved member is sent to the AFK channel's voice node.
    room.member
        .wait_for(|event| match event {
            Event::VoiceServerUpdate(update) if update.channel_id == afk => Some(()),
            _ => None,
        })
        .await;
    room.owner
        .assert_no_event(QUIET, |event| match event {
            Event::VoiceStateUpdate(proto::VoiceStateUpdate {
                voice_state: Some(state),
            }) => state.user_id == room.owner_id || state.user_id == third_id,
            _ => false,
        })
        .await;
}

#[tokio::test]
async fn idle_reports_from_another_session_are_ignored() {
    let mut room = room().await;
    voice_state(room.member.ok(join_voice(room.voice)).await);
    let (mut other_device, _) = join(&room.server, 2, &room.invite).await;

    other_device.ok(report_idle(Some(0))).await;

    let (_, idle) = room.server.handle.voice_activity(room.member_id).unwrap();
    assert_eq!(idle, None);
    room.member.ok(report_idle(Some(1_000))).await;
    let (_, idle) = room.server.handle.voice_activity(room.member_id).unwrap();
    assert!(idle.is_some());
}

/// The candidate the embedded node's Ready names, for a voice gateway
/// WebSocket at `url` over `stream`.
async fn media_candidate<S>(
    stream: S,
    url: &str,
    update: &proto::VoiceServerUpdate,
    user_id: i64,
    session_id: &str,
) -> String
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    use futures_util::{SinkExt, StreamExt};
    use opencord_proto::voice::v1 as voice;
    use prost::Message as _;
    use tokio_tungstenite::tungstenite::Message;

    let (mut socket, _) = tokio_tungstenite::client_async(url, stream).await.unwrap();
    let identify = voice::Envelope {
        seq: 0,
        payload: Some(voice::envelope::Payload::Identify(voice::Identify {
            token: update.token.clone(),
            user_id,
            session_id: session_id.to_owned(),
            channel_id: update.channel_id,
            ..voice::Identify::default()
        })),
    };
    socket
        .send(Message::Binary(identify.encode_to_vec().into()))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let Some(Ok(Message::Binary(bytes))) = socket.next().await else {
                panic!("the voice gateway closed");
            };
            let envelope = voice::Envelope::decode(bytes).unwrap();
            if let Some(voice::envelope::Payload::Ready(ready)) = envelope.payload {
                return ready.candidates[0].ip.clone();
            }
        }
    })
    .await
    .expect("no Ready")
}

/// Plan §4 and §9.2–9.3: tunnels and reverse proxies carry no UDP, so a
/// voice gateway reached through one sends media straight to the server's
/// public address; reached directly, to the host the client used.
#[tokio::test]
async fn voice_reached_through_a_tunnel_or_proxy_sends_media_to_the_public_address() {
    let mut room = room().await;
    room.owner
        .ok(Request::SetPublicAddress(proto::SetPublicAddress {
            auto_detect: false,
            override_ipv4: "203.0.113.10".to_owned(),
            override_ipv6: String::new(),
        }))
        .await;
    let local = room
        .server
        .handle
        .set_local_listener_on_any_port(true)
        .await
        .unwrap()
        .unwrap();
    let session_id = voice_state(room.member.ok(join_voice(room.voice)).await).session_id;
    let update = server_update(&mut room.member).await;
    let tcp = tokio::net::TcpStream::connect(("127.0.0.1", local))
        .await
        .unwrap();
    let proxied = media_candidate(
        tcp,
        "ws://tunnel.example/voice",
        &update,
        room.member_id,
        &session_id,
    )
    .await;
    assert_eq!(proxied, "203.0.113.10");

    room.member
        .ok(Request::RefreshVoiceServer(proto::RefreshVoiceServer {}))
        .await;
    let update = server_update(&mut room.member).await;
    let direct = common::tls_connect(&room.server).await;
    let direct = media_candidate(
        direct,
        &format!(
            "wss://localhost:{}/voice",
            room.server.handle.local_addr.port()
        ),
        &update,
        room.member_id,
        &session_id,
    )
    .await;
    assert_eq!(direct, "", "the host the client reached the node with");
}
