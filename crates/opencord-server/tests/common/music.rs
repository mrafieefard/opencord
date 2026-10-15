//! Shared by the music tests: a server with a fake Lavalink node, and the
//! requests and waits they make.

use std::time::{Duration, Instant};

use opencord_proto::v1 as proto;
use proto::request::Kind;
use proto::response::Result as Response;
use proto::{MusicAction as Action, MusicPlayState as Play};

use super::fake_lavalink::FakeLavalink;
use super::*;

pub const PASSWORD: &str = "youshallnotpass";

pub fn node_settings(name: &str, port: u16) -> proto::LavalinkNodeSettings {
    proto::LavalinkNodeSettings {
        name: name.to_owned(),
        host: "127.0.0.1".to_owned(),
        port: u32::from(port),
        secure: false,
        priority: proto::LavalinkPriority::Normal as i32,
        max_players: 0,
        strict_address: true,
        enabled: true,
    }
}

pub fn nodes(response: Response) -> proto::LavalinkNodes {
    match response {
        Response::LavalinkNodes(nodes) => *nodes,
        other => panic!("expected the nodes, got {other:?}"),
    }
}

/// Adds a node and waits until it is healthy.
pub async fn add_node(owner: &mut TestClient, name: &str, lavalink: &FakeLavalink) -> i64 {
    let list = nodes(
        owner
            .ok(Kind::CreateLavalinkNode(proto::CreateLavalinkNode {
                settings: Some(node_settings(name, lavalink.port())),
                password: PASSWORD.to_owned(),
            }))
            .await,
    );
    let id = list
        .nodes
        .iter()
        .find(|node| node.settings.as_ref().unwrap().name == name)
        .unwrap()
        .id;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let list = nodes(
            owner
                .ok(Kind::FetchLavalinkNodes(proto::FetchLavalinkNodes {
                    watch: false,
                }))
                .await,
        );
        let node = list.nodes.iter().find(|node| node.id == id).unwrap();
        if node.health == proto::LavalinkHealth::Healthy as i32 {
            return id;
        }
        assert!(Instant::now() < deadline, "{name} never got healthy");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

pub fn join_voice(channel_id: i64) -> Kind {
    Kind::UpdateVoiceState(proto::UpdateVoiceState {
        channel_id: Some(channel_id),
        ..Default::default()
    })
}

pub fn leave_voice() -> Kind {
    Kind::UpdateVoiceState(proto::UpdateVoiceState::default())
}

pub fn session(response: Response) -> proto::MusicSession {
    match response {
        Response::MusicSession(session) => *session,
        other => panic!("expected the music session, got {other:?}"),
    }
}

pub fn code(error: &proto::Error) -> proto::ErrorCode {
    proto::ErrorCode::try_from(error.code).unwrap()
}

pub async fn search(client: &mut TestClient, channel_id: i64, query: &str) -> Vec<String> {
    match client
        .ok(Kind::MusicSearch(proto::MusicSearch {
            channel_id,
            query: query.to_owned(),
            source: String::new(),
        }))
        .await
    {
        Response::MusicSearchResults(results) => results
            .results
            .into_iter()
            .map(|result| result.result_id)
            .collect(),
        other => panic!("expected results, got {other:?}"),
    }
}

pub fn enqueue(channel_id: i64, result_ids: Vec<String>) -> Kind {
    Kind::MusicEnqueue(proto::MusicEnqueue {
        channel_id,
        result_ids,
        before_item_id: None,
        placement: proto::MusicPlacement::End as i32,
    })
}

pub fn playback(channel_id: i64, action: Action) -> Kind {
    Kind::MusicPlayback(proto::MusicPlayback {
        channel_id,
        action: action as i32,
        position_ms: None,
        expected_item_id: None,
    })
}

pub fn snapshot_request(channel_id: i64) -> Kind {
    Kind::MusicSubscribe(proto::MusicSubscribe {
        channel_id,
        subscribe: true,
    })
}

/// Follows the channel's snapshots until one matches, for at most 8 s;
/// asks again when none came for a second. Skips other events, and
/// snapshots older than the one at hand.
pub async fn until(
    client: &mut TestClient,
    channel_id: i64,
    what: &str,
    matches: impl Fn(&proto::MusicSession) -> bool,
) -> proto::MusicSession {
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut now = session(client.ok(snapshot_request(channel_id)).await);
    loop {
        if matches(&now) {
            return now;
        }
        assert!(Instant::now() < deadline, "never {what}: {now:#?}");
        let next = tokio::time::timeout(
            Duration::from_secs(1),
            client.wait_for(|event| match event {
                proto::event::Kind::MusicSnapshot(snapshot) => {
                    snapshot.session.clone().filter(|session| {
                        session.channel_id == channel_id && session.version >= now.version
                    })
                }
                _ => None,
            }),
        )
        .await;
        now = match next {
            Ok(next) => next,
            Err(_) => session(client.ok(snapshot_request(channel_id)).await),
        };
    }
}

pub fn title(item: Option<&proto::MusicQueueItem>) -> &str {
    item.and_then(|item| item.track.as_ref())
        .map_or("", |track| track.title.as_str())
}

pub fn playing(session: &proto::MusicSession, wanted: &str) -> bool {
    session.state == Play::Playing as i32 && title(session.now_playing.as_ref()) == wanted
}

/// Waits until the fake's player for the channel plays `title`.
pub async fn fake_plays(lavalink: &FakeLavalink, channel_id: i64, title: &str) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let player = lavalink.fake.player(channel_id);
        if player.as_ref().and_then(|player| player.title()).as_deref() == Some(title) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the node never played {title}: {player:?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Reports a position once the server listens to reports again: it
/// ignores them for 1.5 s after a track starts or seeks.
pub async fn report_position(lavalink: &FakeLavalink, channel_id: i64, position: u64) {
    tokio::time::sleep(Duration::from_millis(1_600)).await;
    lavalink.fake.report_position(channel_id, position);
}

pub struct Room {
    pub server: TestServer,
    pub owner: TestClient,
    pub member: TestClient,
    pub member_id: i64,
    pub voice: i64,
    pub everyone: i64,
    pub lavalink: FakeLavalink,
    pub node: i64,
}

/// A server with one node, the owner in the voice channel, and a member
/// who is not.
pub async fn room() -> Room {
    let lavalink = FakeLavalink::start(PASSWORD).await;
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let node = add_node(&mut owner, "Home", &lavalink).await;
    let voice = channel_id(&ready, "General");
    owner.ok(join_voice(voice)).await;
    let code = invite(&mut owner, None).await;
    let (member, member_ready) = join(&server, 2, &code).await;
    Room {
        member_id: self_id(&member_ready),
        everyone: ready.server.as_ref().unwrap().everyone_role_id,
        server,
        owner,
        member,
        voice,
        lavalink,
        node,
    }
}
