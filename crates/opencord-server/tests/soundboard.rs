//! The soundboard on the server (Phase 2 plan V8): uploads checked by
//! parsing the file, whatever the client says it is, and every play
//! checked again before anyone is told to play it.

mod common;

use std::time::Duration;

use common::{TestClient, TestServer, channel_id, invite, join, owner, self_id};
use opencord_common::permissions::Permissions;
use opencord_common::sound::ogg;
use opencord_proto::v1 as proto;
use prost::Message as _;
use proto::event::Kind as Event;
use proto::request::Kind as Request;
use proto::response::Result as Response;
use sha2::{Digest, Sha256};

const QUIET: Duration = Duration::from_millis(300);

struct Room {
    server: TestServer,
    owner: TestClient,
    owner_token: String,
    member: TestClient,
    member_id: i64,
    member_token: String,
    voice: i64,
    everyone: i64,
    invite: String,
}

async fn room() -> Room {
    let server = TestServer::start().await;
    let (mut owner, owner_ready) = owner(&server).await;
    let invite = invite(&mut owner, None).await;
    let (member, member_ready) = join(&server, 2, &invite).await;
    Room {
        voice: channel_id(&owner_ready, "General"),
        everyone: owner_ready.server.as_ref().unwrap().everyone_role_id,
        owner_token: media_token(&owner_ready),
        member_id: self_id(&member_ready),
        member_token: media_token(&member_ready),
        server,
        owner,
        member,
        invite,
    }
}

fn media_token(ready: &proto::Ready) -> String {
    ready.media_token.as_ref().unwrap().token.clone()
}

/// An Ogg Opus file of silent 20 ms CELT packets; `claimed` is what its
/// final granule position says, which a modified client can set at will.
fn sound_file(seconds: f64, claimed: f64) -> Vec<u8> {
    let samples = (seconds * 48_000.0) as u64;
    let packets: Vec<Vec<u8>> = (0..samples.div_ceil(960))
        .map(|n| vec![31 << 3, n as u8])
        .collect();
    ogg::write(1, 0, &packets, (claimed * 48_000.0) as u64)
}

async fn post(server: &TestServer, path: &str, token: &str, body: &[u8]) -> (u16, Response) {
    let authorization = format!("Bearer {token}");
    let (status, bytes) = server
        .send_http("POST", path, &[("Authorization", &authorization)], body)
        .await;
    let response = proto::Response::decode(bytes.as_slice()).unwrap();
    (status, response.result.unwrap())
}

async fn upload(server: &TestServer, token: &str, name: &str, body: &[u8]) -> (u16, Response) {
    let path = format!("/media/sounds?name={name}&emoji=%F0%9F%94%94&volume=80");
    post(server, &path, token, body).await
}

fn uploaded(reply: (u16, Response)) -> proto::SoundboardSound {
    match reply {
        (201, Response::SoundboardSound(sound)) => sound,
        other => panic!("expected a new sound, got {other:?}"),
    }
}

fn refused(reply: (u16, Response)) -> (u16, proto::ErrorCode) {
    match reply {
        (status, Response::Error(error)) => {
            (status, proto::ErrorCode::try_from(error.code).unwrap())
        }
        other => panic!("expected an error, got {other:?}"),
    }
}

fn code(error: &proto::Error) -> proto::ErrorCode {
    proto::ErrorCode::try_from(error.code).unwrap()
}

fn join_voice(channel_id: i64) -> Request {
    Request::UpdateVoiceState(proto::UpdateVoiceState {
        channel_id: Some(channel_id),
        ..Default::default()
    })
}

fn play(channel_id: i64, sound_ref: &str) -> Request {
    Request::PlaySoundboardSound(proto::PlaySoundboardSound {
        channel_id,
        sound_ref: sound_ref.to_owned(),
        ..Default::default()
    })
}

fn server_ref(sound: &proto::SoundboardSound) -> String {
    format!("server:{}", hex::encode(&sound.sha256))
}

fn settings(change: proto::UpdateVoiceSettings) -> Request {
    Request::UpdateVoiceSettings(change)
}

fn deny_in(channel_id: i64, role_id: i64, deny: Permissions) -> Request {
    Request::SetChannelOverwrite(proto::SetChannelOverwrite {
        channel_id,
        overwrite: Some(proto::PermissionOverwrite {
            target_kind: proto::OverwriteTarget::Role as i32,
            target_id: role_id,
            allow: 0,
            deny: deny.bits(),
        }),
    })
}

async fn effect(client: &mut TestClient) -> proto::VoiceChannelEffect {
    client
        .wait_for(|event| match event {
            Event::VoiceChannelEffect(effect) => Some(effect.clone()),
            _ => None,
        })
        .await
}

#[tokio::test]
async fn a_six_second_file_is_refused_even_from_a_modified_client() {
    let mut room = room().await;
    let (server, token) = (&room.server, room.owner_token.as_str());

    let too_long = upload(server, token, "Long", &sound_file(6.0, 6.0)).await;
    // Claims one second in its header; its packets play for six.
    let lying = upload(server, token, "Liar", &sound_file(6.0, 1.0)).await;
    let not_ogg = upload(server, token, "Wave", b"RIFF\0\0\0\0WAVEfmt ").await;
    let too_big = upload(server, token, "Big", &vec![0; 300 * 1024]).await;
    let sound = uploaded(upload(server, token, "Chime", &sound_file(4.5, 4.5)).await);

    assert_eq!(refused(too_long), (400, proto::ErrorCode::SoundTooLong));
    assert_eq!(refused(lying), (400, proto::ErrorCode::SoundTooLong));
    assert_eq!(refused(not_ogg), (400, proto::ErrorCode::SoundInvalid));
    assert_eq!(refused(too_big), (400, proto::ErrorCode::SoundInvalid));
    assert_eq!(sound.name, "Chime");
    assert_eq!(sound.emoji.as_deref(), Some("🔔"));
    assert_eq!(sound.volume, 80);
    assert_eq!(sound.duration_ms, 4_500);
    let announced = room
        .member
        .wait_for(|event| match event {
            Event::SoundboardSoundCreate(created) => created.sound.clone(),
            _ => None,
        })
        .await;
    assert_eq!(announced, sound);

    // The file is served by its hash, and new sessions get the sound.
    let file = sound_file(4.5, 4.5);
    assert_eq!(sound.sha256, Sha256::digest(&file).to_vec());
    let path = format!("/media/sounds/{}", hex::encode(&sound.sha256));
    let authorization = format!("Bearer {}", room.member_token);
    let (status, served) = room
        .server
        .send_http("GET", &path, &[("Authorization", &authorization)], &[])
        .await;
    assert_eq!((status, served), (200, file));
    let (_, ready) = join(&room.server, 3, &room.invite).await;
    assert_eq!(ready.soundboard_sounds, [sound]);
}

#[tokio::test]
async fn uploading_needs_manage_soundboard_a_token_and_room() {
    let mut room = room().await;
    room.owner
        .ok(settings(proto::UpdateVoiceSettings {
            max_sounds: Some(1),
            ..Default::default()
        }))
        .await;
    let file = sound_file(1.0, 1.0);

    let by_member = upload(&room.server, &room.member_token, "Mine", &file).await;
    let no_token = upload(&room.server, "not-a-token", "Mine", &file).await;
    let short_name = upload(&room.server, &room.owner_token, "x", &file).await;
    uploaded(upload(&room.server, &room.owner_token, "First", &file).await);
    let full = upload(&room.server, &room.owner_token, "Second", &file).await;

    assert_eq!(refused(by_member), (403, proto::ErrorCode::Forbidden));
    assert_eq!(refused(no_token), (401, proto::ErrorCode::Unauthorized));
    assert_eq!(
        refused(short_name),
        (400, proto::ErrorCode::InvalidArgument)
    );
    assert_eq!(refused(full), (409, proto::ErrorCode::SoundboardFull));
}

#[tokio::test]
async fn a_sound_is_played_for_everyone_in_the_channel_after_the_servers_checks() {
    let mut room = room().await;
    let sound = uploaded(
        upload(
            &room.server,
            &room.owner_token,
            "Chime",
            &sound_file(1.0, 1.0),
        )
        .await,
    );
    let (mut outsider, _) = join(&room.server, 3, &room.invite).await;
    let sound_ref = server_ref(&sound);

    let not_in_voice = room.member.error(play(room.voice, &sound_ref)).await;
    room.owner.ok(join_voice(room.voice)).await;
    room.member.ok(join_voice(room.voice)).await;
    let unknown = room
        .member
        .error(play(room.voice, &format!("server:{}", "00".repeat(32))))
        .await;
    room.member.ok(play(room.voice, &sound_ref)).await;
    let cooling_down = room.member.error(play(room.voice, &sound_ref)).await;

    assert_eq!(code(&not_in_voice), proto::ErrorCode::VoiceNotConnected);
    assert_eq!(code(&unknown), proto::ErrorCode::NotFound);
    assert_eq!(code(&cooling_down), proto::ErrorCode::SoundCooldown);
    assert!(cooling_down.retry_after_ms.unwrap() <= 3_000);
    // The player hears it too, and so does everyone else in the channel.
    for client in [&mut room.owner, &mut room.member] {
        let heard = effect(client).await;
        assert_eq!(heard.user_id, room.member_id);
        assert_eq!(heard.sound_ref, sound_ref);
        assert_eq!((heard.name.as_str(), heard.volume), ("Chime", 80));
    }
    outsider
        .assert_no_event(QUIET, |event| matches!(event, Event::VoiceChannelEffect(_)))
        .await;
}

#[tokio::test]
async fn playing_needs_use_soundboard_and_no_server_mute_or_deafen() {
    let mut room = room().await;
    let chime = "default:chime";
    room.owner.ok(join_voice(room.voice)).await;
    room.member.ok(join_voice(room.voice)).await;

    room.owner
        .ok(Request::ServerMuteMember(proto::ServerMuteMember {
            user_id: room.member_id,
            value: true,
        }))
        .await;
    let muted = room.member.error(play(room.voice, chime)).await;
    room.owner
        .ok(Request::ServerMuteMember(proto::ServerMuteMember {
            user_id: room.member_id,
            value: false,
        }))
        .await;
    room.member
        .ok(Request::UpdateVoiceState(proto::UpdateVoiceState {
            channel_id: Some(room.voice),
            self_deaf: true,
            ..Default::default()
        }))
        .await;
    let deafened = room.member.error(play(room.voice, chime)).await;
    room.owner
        .ok(deny_in(
            room.voice,
            room.everyone,
            Permissions::USE_SOUNDBOARD,
        ))
        .await;
    // Self-muted people can play sounds; this one lacks the permission.
    room.member
        .ok(Request::UpdateVoiceState(proto::UpdateVoiceState {
            channel_id: Some(room.voice),
            self_mute: true,
            ..Default::default()
        }))
        .await;
    let no_permission = room.member.error(play(room.voice, chime)).await;

    assert_eq!(code(&muted), proto::ErrorCode::Forbidden);
    assert_eq!(code(&deafened), proto::ErrorCode::Forbidden);
    assert_eq!(code(&no_permission), proto::ErrorCode::Forbidden);
}

#[tokio::test]
async fn sounds_from_another_server_need_the_permission_and_an_upload() {
    let mut room = room().await;
    let file = sound_file(2.0, 2.0);
    let sound_ref = format!("external:{}", hex::encode(Sha256::digest(&file)));
    let named = |sound_ref: &str| {
        Request::PlaySoundboardSound(proto::PlaySoundboardSound {
            channel_id: room.voice,
            sound_ref: sound_ref.to_owned(),
            name: Some("Elsewhere".to_owned()),
            emoji: Some("🎺".to_owned()),
            volume: Some(50),
        })
    };
    room.owner.ok(join_voice(room.voice)).await;
    room.member.ok(join_voice(room.voice)).await;

    let not_uploaded = room.member.error(named(&sound_ref)).await;
    let (status, ack) = post(
        &room.server,
        "/media/external-sounds",
        &room.member_token,
        &file,
    )
    .await;
    assert_eq!((status, ack), (200, Response::Ack(proto::Ack {})));
    room.member.ok(named(&sound_ref)).await;
    let heard = effect(&mut room.owner).await;
    room.owner
        .ok(deny_in(
            room.voice,
            room.everyone,
            Permissions::USE_EXTERNAL_SOUNDS,
        ))
        .await;
    let no_permission = room.member.error(named(&sound_ref)).await;
    room.owner
        .ok(settings(proto::UpdateVoiceSettings {
            allow_external_sounds: Some(false),
            ..Default::default()
        }))
        .await;
    let turned_off = refused(
        post(
            &room.server,
            "/media/external-sounds",
            &room.member_token,
            &file,
        )
        .await,
    );

    assert_eq!(code(&not_uploaded), proto::ErrorCode::NotFound);
    assert_eq!(heard.sound_ref, sound_ref);
    assert_eq!((heard.name.as_str(), heard.volume), ("Elsewhere", 50));
    assert_eq!(heard.emoji.as_deref(), Some("🎺"));
    assert_eq!(code(&no_permission), proto::ErrorCode::Forbidden);
    assert_eq!(turned_off, (403, proto::ErrorCode::Forbidden));
}

#[tokio::test]
async fn default_sounds_play_unless_the_server_turns_them_off() {
    let mut room = room().await;
    room.member.ok(join_voice(room.voice)).await;

    room.member.ok(play(room.voice, "default:tada")).await;
    let heard = effect(&mut room.member).await;
    room.owner
        .ok(settings(proto::UpdateVoiceSettings {
            allow_default_sounds: Some(false),
            sound_cooldown_s: Some(0),
            ..Default::default()
        }))
        .await;
    let turned_off = room.member.error(play(room.voice, "default:tada")).await;
    room.owner
        .ok(settings(proto::UpdateVoiceSettings {
            soundboard_enabled: Some(false),
            ..Default::default()
        }))
        .await;
    let disabled = room.member.error(play(room.voice, "default:chime")).await;

    assert_eq!(
        (heard.name.as_str(), heard.emoji.as_deref()),
        ("Ta-da", Some("🎉"))
    );
    assert_eq!(code(&turned_off), proto::ErrorCode::Forbidden);
    assert_eq!(code(&disabled), proto::ErrorCode::Forbidden);
}

#[tokio::test]
async fn at_most_three_sounds_play_at_once_in_a_channel() {
    let mut room = room().await;
    let (mut third, _) = join(&room.server, 3, &room.invite).await;
    let (mut fourth, _) = join(&room.server, 4, &room.invite).await;
    for client in [&mut room.owner, &mut room.member, &mut third, &mut fourth] {
        client.ok(join_voice(room.voice)).await;
    }

    for client in [&mut room.owner, &mut room.member, &mut third] {
        client.ok(play(room.voice, "default:applause")).await;
    }
    let full = fourth.error(play(room.voice, "default:applause")).await;

    assert_eq!(code(&full), proto::ErrorCode::SoundCooldown);
    assert!(full.retry_after_ms.is_some());
}

#[tokio::test]
async fn editing_and_deleting_a_sound_reach_everyone() {
    let mut room = room().await;
    let sound = uploaded(
        upload(
            &room.server,
            &room.owner_token,
            "Chime",
            &sound_file(1.0, 1.0),
        )
        .await,
    );

    let edit = Request::UpdateSoundboardSound(proto::UpdateSoundboardSound {
        sound_id: sound.id,
        name: Some("Doorbell".to_owned()),
        emoji: Some(String::new()),
        volume: Some(40),
    });
    let not_allowed = room.member.error(edit.clone()).await;
    let edited = match room.owner.ok(edit).await {
        Response::SoundboardSound(sound) => sound,
        other => panic!("expected a sound, got {other:?}"),
    };
    let announced = room
        .member
        .wait_for(|event| match event {
            Event::SoundboardSoundUpdate(update) => update.sound.clone(),
            _ => None,
        })
        .await;
    room.owner
        .ok(Request::DeleteSoundboardSound(
            proto::DeleteSoundboardSound { sound_id: sound.id },
        ))
        .await;
    let deleted = room
        .member
        .wait_for(|event| match event {
            Event::SoundboardSoundDelete(deleted) => Some(deleted.sound_id),
            _ => None,
        })
        .await;
    let path = format!("/media/sounds/{}", hex::encode(&sound.sha256));
    let authorization = format!("Bearer {}", room.member_token);
    let (status, _) = room
        .server
        .send_http("GET", &path, &[("Authorization", &authorization)], &[])
        .await;

    assert_eq!(code(&not_allowed), proto::ErrorCode::Forbidden);
    assert_eq!(
        (edited.name.as_str(), edited.emoji.as_deref(), edited.volume),
        ("Doorbell", None, 40)
    );
    assert_eq!(announced, edited);
    assert_eq!(deleted, sound.id);
    assert_eq!(status, 404);
}
