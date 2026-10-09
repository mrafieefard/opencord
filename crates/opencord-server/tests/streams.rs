//! Screen shares on the main gateway (Phase 2 plan V6): going live within
//! the server's maximum, opt-in watching up to the viewer limit, the
//! viewer list, and streams ending with the voice state or the permission.

mod common;

use std::time::Duration;

use common::{TestClient, TestServer, channel_id, invite, join, owner, self_id};
use opencord_common::permissions::Permissions;
use opencord_proto::v1 as proto;
use proto::event::Kind as Event;
use proto::request::Kind as Request;
use proto::response::Result as Response;

const QUIET: Duration = Duration::from_millis(300);
const P720: proto::ScreenShareResolution = proto::ScreenShareResolution::ScreenShareResolution720p;
const P1080: proto::ScreenShareResolution =
    proto::ScreenShareResolution::ScreenShareResolution1080p;

struct Room {
    server: TestServer,
    owner: TestClient,
    owner_id: i64,
    member: TestClient,
    member_id: i64,
    voice: i64,
    invite: String,
    everyone: i64,
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
        everyone: owner_ready.server.as_ref().unwrap().everyone_role_id,
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

fn go_live(channel_id: i64, resolution: proto::ScreenShareResolution, fps: u32) -> Request {
    Request::CreateStream(proto::CreateStream {
        channel_id,
        source_kind: proto::StreamSourceKind::Screen as i32,
        resolution: resolution as i32,
        fps,
        has_audio: true,
    })
}

fn watch(key: &str) -> Request {
    Request::WatchStream(proto::WatchStream {
        stream_key: key.to_owned(),
    })
}

fn stream(response: Response) -> proto::Stream {
    match response {
        Response::Stream(stream) => stream,
        other => panic!("expected a stream, got {other:?}"),
    }
}

fn code(error: &proto::Error) -> proto::ErrorCode {
    proto::ErrorCode::try_from(error.code).unwrap()
}

async fn stream_deleted(client: &mut TestClient) -> String {
    client
        .wait_for(|event| match event {
            Event::StreamDelete(deleted) => Some(deleted.stream_key.clone()),
            _ => None,
        })
        .await
}

async fn viewers(client: &mut TestClient) -> Vec<i64> {
    client
        .wait_for(|event| match event {
            Event::StreamViewersUpdate(update) => Some(update.viewer_ids.clone()),
            _ => None,
        })
        .await
}

#[tokio::test]
async fn going_live_needs_the_voice_channel_and_stays_within_the_server_maximum() {
    let mut room = room().await;

    let not_in_voice = room.member.error(go_live(room.voice, P720, 30)).await;
    room.member.ok(join_voice(room.voice)).await;
    let too_large = room.member.error(go_live(room.voice, P1080, 30)).await;
    let too_fast = room.member.error(go_live(room.voice, P720, 60)).await;
    let live = stream(room.member.ok(go_live(room.voice, P720, 30)).await);

    assert_eq!(code(&not_in_voice), proto::ErrorCode::VoiceNotConnected);
    assert_eq!(code(&too_large), proto::ErrorCode::QualityLimit);
    assert_eq!(code(&too_fast), proto::ErrorCode::QualityLimit);
    assert_eq!(
        live.stream_key,
        format!("stream:{}:{}", room.voice, room.member_id)
    );
    assert_eq!(live.viewer_count, 0);
    let announced = room
        .owner
        .wait_for(|event| match event {
            Event::StreamCreate(created) => created.stream.clone(),
            _ => None,
        })
        .await;
    assert_eq!(announced, live);
    let streaming = room
        .owner
        .wait_for(|event| match event {
            Event::VoiceStateUpdate(update) => update
                .voice_state
                .clone()
                .filter(|state| state.user_id == room.member_id && state.self_stream),
            _ => None,
        })
        .await;
    assert!(streaming.self_stream);
}

#[tokio::test]
async fn sharing_needs_the_screenshare_permission() {
    let mut room = room().await;
    room.owner
        .ok(Request::SetChannelOverwrite(proto::SetChannelOverwrite {
            channel_id: room.voice,
            overwrite: Some(proto::PermissionOverwrite {
                target_kind: proto::OverwriteTarget::Role as i32,
                target_id: room.everyone,
                allow: 0,
                deny: Permissions::SCREENSHARE.bits(),
            }),
        }))
        .await;
    room.member.ok(join_voice(room.voice)).await;

    let refused = room.member.error(go_live(room.voice, P720, 30)).await;

    assert_eq!(code(&refused), proto::ErrorCode::Forbidden);
}

#[tokio::test]
async fn viewers_opt_in_the_streamer_sees_who_and_the_limit_holds() {
    let mut room = room().await;
    let (mut third, third_ready) = join(&room.server, 3, &room.invite).await;
    room.owner
        .ok(Request::UpdateVoiceSettings(proto::UpdateVoiceSettings {
            max_stream_viewers: Some(1),
            ..Default::default()
        }))
        .await;
    room.member.ok(join_voice(room.voice)).await;
    let key = stream(room.member.ok(go_live(room.voice, P720, 30)).await).stream_key;

    let not_in_voice = room.owner.error(watch(&key)).await;
    room.owner.ok(join_voice(room.voice)).await;
    third.ok(join_voice(room.voice)).await;
    let own = room.member.error(watch(&key)).await;
    room.owner.ok(watch(&key)).await;
    let full = third.error(watch(&key)).await;

    assert_eq!(code(&not_in_voice), proto::ErrorCode::VoiceNotConnected);
    assert_eq!(code(&own), proto::ErrorCode::InvalidArgument);
    assert_eq!(code(&full), proto::ErrorCode::StreamViewerLimit);
    assert_eq!(full.message, "This stream is full (1 viewers)");
    assert_eq!(viewers(&mut room.member).await, [room.owner_id]);
    let count = third
        .wait_for(|event| match event {
            Event::StreamUpdate(update) => update.stream.clone(),
            _ => None,
        })
        .await
        .viewer_count;
    assert_eq!(count, 1);
    // Only the streamer gets the list.
    third
        .assert_no_event(QUIET, |event| {
            matches!(event, Event::StreamViewersUpdate(_))
        })
        .await;

    room.owner
        .ok(Request::UnwatchStream(proto::UnwatchStream {
            stream_key: key.clone(),
        }))
        .await;
    assert_eq!(viewers(&mut room.member).await, Vec::<i64>::new());
    third.ok(watch(&key)).await;
    assert_eq!(viewers(&mut room.member).await, [self_id(&third_ready)]);
}

#[tokio::test]
async fn a_stream_ends_when_its_streamer_leaves_and_viewers_who_leave_stop_watching() {
    let mut room = room().await;
    room.member.ok(join_voice(room.voice)).await;
    room.owner.ok(join_voice(room.voice)).await;
    let key = stream(room.member.ok(go_live(room.voice, P720, 30)).await).stream_key;
    room.owner.ok(watch(&key)).await;
    assert_eq!(viewers(&mut room.member).await, [room.owner_id]);

    room.owner
        .ok(Request::UpdateVoiceState(proto::UpdateVoiceState::default()))
        .await;
    assert_eq!(viewers(&mut room.member).await, Vec::<i64>::new());

    room.member
        .ok(Request::UpdateVoiceState(proto::UpdateVoiceState::default()))
        .await;
    assert_eq!(stream_deleted(&mut room.owner).await, key);
}

#[tokio::test]
async fn a_lowered_maximum_or_a_lost_permission_ends_streams_above_it() {
    let mut room = room().await;
    room.owner
        .ok(Request::UpdateVoiceSettings(proto::UpdateVoiceSettings {
            screen_share_max_resolution: Some(P1080 as i32),
            ..Default::default()
        }))
        .await;
    room.member.ok(join_voice(room.voice)).await;
    let key = stream(room.member.ok(go_live(room.voice, P1080, 30)).await).stream_key;

    room.owner
        .ok(Request::UpdateVoiceSettings(proto::UpdateVoiceSettings {
            screen_share_max_resolution: Some(P720 as i32),
            ..Default::default()
        }))
        .await;
    assert_eq!(stream_deleted(&mut room.member).await, key);

    let key = stream(room.member.ok(go_live(room.voice, P720, 30)).await).stream_key;
    room.owner
        .ok(Request::SetChannelOverwrite(proto::SetChannelOverwrite {
            channel_id: room.voice,
            overwrite: Some(proto::PermissionOverwrite {
                target_kind: proto::OverwriteTarget::Role as i32,
                target_id: room.everyone,
                allow: 0,
                deny: Permissions::SCREENSHARE.bits(),
            }),
        }))
        .await;
    assert_eq!(stream_deleted(&mut room.member).await, key);
    let state = room
        .member
        .wait_for(|event| match event {
            Event::VoiceStateUpdate(update) => update
                .voice_state
                .clone()
                .filter(|state| state.user_id == room.member_id && !state.self_stream),
            _ => None,
        })
        .await;
    assert_eq!(state.channel_id, Some(room.voice));
}

#[tokio::test]
async fn ready_lists_live_streams_and_changes_reach_everyone() {
    let mut room = room().await;
    room.member.ok(join_voice(room.voice)).await;
    let key = stream(room.member.ok(go_live(room.voice, P720, 30)).await).stream_key;

    let updated = stream(
        room.member
            .ok(Request::UpdateStream(proto::UpdateStream {
                stream_key: key.clone(),
                fps: Some(15),
                has_audio: Some(false),
                ..Default::default()
            }))
            .await,
    );
    let (_late, late_ready) = join(&room.server, 3, &room.invite).await;
    let someone_elses = room
        .owner
        .error(Request::DeleteStream(proto::DeleteStream {
            stream_key: key.clone(),
        }))
        .await;

    assert_eq!((updated.fps, updated.has_audio), (15, false));
    assert_eq!(late_ready.streams, [updated]);
    assert_eq!(code(&someone_elses), proto::ErrorCode::Forbidden);

    room.member
        .ok(Request::DeleteStream(proto::DeleteStream {
            stream_key: key.clone(),
        }))
        .await;
    assert_eq!(stream_deleted(&mut room.owner).await, key);
}

#[tokio::test]
async fn going_live_is_rate_limited() {
    let mut room = room().await;
    room.member.ok(join_voice(room.voice)).await;

    for _ in 0..5 {
        room.member.ok(go_live(room.voice, P720, 30)).await;
    }
    let sixth = room.member.error(go_live(room.voice, P720, 30)).await;

    assert_eq!(code(&sixth), proto::ErrorCode::RateLimited);
}
