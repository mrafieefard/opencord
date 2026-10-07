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
    assert!(on_body.contains("\"voice_udp_port\":7711"), "{on_body}");
    assert!(off_body.contains("\"voice_enabled\":false"), "{off_body}");
}
