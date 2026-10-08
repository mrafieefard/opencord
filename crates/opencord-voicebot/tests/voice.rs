//! Phase 2 V1 acceptance: voicebots talking through a real server's voice
//! node.

use std::time::{Duration, Instant};

use opencord_common::address::format_fingerprint;
use opencord_core::api::types::{MediaEvent, SpeakingChange, VoiceConnectionState};
use opencord_core::media::MediaOptions;
use opencord_server::config::{Config, ExternalNode, VoiceMode};
use opencord_server::server::{self, ServerHandle};
use opencord_server::voice_node::{self, VoiceNodeConfig, VoiceNodeHandle};
use opencord_voicebot::{VoiceSession, Voicebot};

struct TestServer {
    handle: ServerHandle,
    _dir: tempfile::TempDir,
}

impl TestServer {
    async fn start() -> Self {
        Self::start_with(|_| {}).await
    }

    async fn start_with(customize: impl FnOnce(&mut Config)) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.server.bind = "127.0.0.1:0".parse().unwrap();
        config.server.public_host = "127.0.0.1".to_owned();
        config.server.data_dir = dir.path().join("data");
        config.voice.udp_port = 0;
        customize(&mut config);
        let handle = server::start(config).await.unwrap();
        Self { handle, _dir: dir }
    }

    fn address(&self) -> String {
        format!("127.0.0.1:{}", self.handle.local_addr.port())
    }
}

/// A main server without voice of its own, and two external voice nodes.
struct Cluster {
    server: TestServer,
    nodes: Vec<VoiceNodeHandle>,
    _dir: tempfile::TempDir,
}

async fn cluster() -> Cluster {
    let dir = tempfile::tempdir().unwrap();
    let mut planned = Vec::new();
    for name in ["a", "b"] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("wss://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let secret_file = dir.path().join(format!("{name}.secret"));
        std::fs::write(&secret_file, format!("{name}-shared-secret-for-tests\n")).unwrap();
        planned.push((name, listener, endpoint, secret_file));
    }
    let external_nodes: Vec<ExternalNode> = planned
        .iter()
        .map(|(_, _, endpoint, secret_file)| ExternalNode {
            endpoint: endpoint.clone(),
            secret_file: secret_file.clone(),
        })
        .collect();
    let server = TestServer::start_with(|config| {
        config.voice.mode = VoiceMode::External;
        config.voice.external_nodes = external_nodes;
    })
    .await;
    let mut nodes = Vec::new();
    for (name, listener, endpoint, secret_file) in planned {
        let mut config = VoiceNodeConfig::default();
        config.node.main_server = server.address();
        config.node.main_fingerprint = Some(format_fingerprint(&server.handle.fingerprint));
        config.node.secret_file = secret_file;
        config.node.endpoint = endpoint;
        config.node.udp_port = 0;
        config.node.data_dir = dir.path().join(name);
        let node = voice_node::start_on(config, listener).await.unwrap();
        node.registered().await;
        nodes.push(node);
    }
    Cluster {
        server,
        nodes,
        _dir: dir,
    }
}

impl Cluster {
    /// Stops the node at `endpoint` the way a crash would for its clients
    /// and the main server: every connection to it ends.
    async fn kill(&mut self, endpoint: &str) {
        let index = self
            .nodes
            .iter()
            .position(|node| node.endpoint() == endpoint)
            .expect("a node at that endpoint");
        self.nodes.remove(index).shutdown().await;
    }
}

/// An owner and a member bot, both in the General voice channel.
struct Call {
    _server: TestServer,
    owner: Voicebot,
    owner_voice: VoiceSession,
    member: Voicebot,
    member_voice: VoiceSession,
}

async fn call() -> Call {
    let server = TestServer::start().await;
    let (owner, owner_voice, member, member_voice) = two_bots_in_general(&server).await;
    Call {
        _server: server,
        owner,
        owner_voice,
        member,
        member_voice,
    }
}

/// The same call on a cluster's external nodes.
struct ExternalCall {
    cluster: Cluster,
    owner: Voicebot,
    owner_voice: VoiceSession,
    member: Voicebot,
    member_voice: VoiceSession,
}

async fn external_call() -> ExternalCall {
    let cluster = cluster().await;
    let (owner, owner_voice, member, member_voice) = two_bots_in_general(&cluster.server).await;
    ExternalCall {
        cluster,
        owner,
        owner_voice,
        member,
        member_voice,
    }
}

async fn two_bots_in_general(
    server: &TestServer,
) -> (Voicebot, VoiceSession, Voicebot, VoiceSession) {
    let owner = Voicebot::connect(
        &server.address(),
        server.handle.claim_token.clone(),
        "Owner",
    )
    .await
    .unwrap();
    let invite = owner
        .client
        .create_invite(&owner.server_key, None, None)
        .await
        .unwrap();
    let member = Voicebot::connect(&invite.link, None, "Member")
        .await
        .unwrap();
    let general = owner.voice_channel("General").unwrap();
    let owner_voice = owner.join(general).await.unwrap();
    let member_voice = member.join(general).await.unwrap();
    (owner, owner_voice, member, member_voice)
}

/// Both bots play a tone for a second; asserts each heard the other.
async fn assert_they_hear_each_other(
    (owner, owner_voice): (&Voicebot, &VoiceSession),
    (member, member_voice): (&Voicebot, &VoiceSession),
) {
    owner_voice.clear_heard();
    member_voice.clear_heard();
    owner_voice
        .play_tone(440.0, Duration::from_secs(1))
        .await
        .unwrap();
    member_voice
        .play_tone(660.0, Duration::from_secs(1))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    let member_heard = member_voice.heard();
    let owner_heard = owner_voice.heard();
    let from_owner = &member_heard[&owner.user_id()];
    let from_member = &owner_heard[&member.user_id()];
    assert!(from_owner.decoded >= 45, "{from_owner:?}");
    assert!(from_member.decoded >= 45, "{from_member:?}");
    assert!(from_owner.peak_dbfs > -20.0, "{from_owner:?}");
    assert!(!member_heard.contains_key(&member.user_id()), "no echo");
}

/// The longest silence between `start` and the last arrival.
fn longest_gap(start: Instant, arrivals: &[Instant]) -> Duration {
    let mut times: Vec<Instant> = arrivals.iter().copied().filter(|at| *at >= start).collect();
    times.sort();
    times.insert(0, start);
    times
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .max()
        .unwrap_or_default()
}

#[tokio::test]
async fn two_voicebots_hear_each_other_through_the_embedded_node() {
    let call = call().await;

    assert_they_hear_each_other(
        (&call.owner, &call.owner_voice),
        (&call.member, &call.member_voice),
    )
    .await;
}

#[tokio::test]
async fn two_voicebots_hear_each_other_through_an_external_node() {
    let call = external_call().await;
    let endpoints: Vec<&str> = call
        .cluster
        .nodes
        .iter()
        .map(|node| node.endpoint())
        .collect();

    assert_they_hear_each_other(
        (&call.owner, &call.owner_voice),
        (&call.member, &call.member_voice),
    )
    .await;
    assert!(endpoints.contains(&call.owner_voice.gateway_url().as_str()));
    assert_eq!(
        call.member_voice.gateway_url(),
        call.owner_voice.gateway_url()
    );
}

#[tokio::test]
async fn killing_an_external_node_moves_its_call_and_the_bots_reconnect_within_5_s() {
    let mut call = external_call().await;
    let first = call.owner_voice.gateway_url();

    let killed_at = Instant::now();
    call.cluster.kill(&first).await;
    for voice in [&call.owner_voice, &call.member_voice] {
        voice
            .wait_until(|view| view.connections == 2 && view.media_connected)
            .await
            .unwrap();
    }
    let reconnected_in = killed_at.elapsed();

    assert!(
        reconnected_in < Duration::from_secs(5),
        "{reconnected_in:?}"
    );
    assert_ne!(call.owner_voice.gateway_url(), first);
    assert_eq!(
        call.member_voice.gateway_url(),
        call.owner_voice.gateway_url()
    );
    assert_they_hear_each_other(
        (&call.owner, &call.owner_voice),
        (&call.member, &call.member_voice),
    )
    .await;
}

#[tokio::test]
async fn a_server_mute_stops_forwarding_within_100_ms() {
    let call = call().await;
    let member_id = call.member.user_id();
    let talking = call.member_voice.play_tone(440.0, Duration::from_secs(2));
    call.owner_voice
        .wait_until(|view| view.heard.get(&member_id).is_some_and(|h| h.decoded >= 10))
        .await
        .unwrap();

    let muted_at = Instant::now();
    call.owner
        .client
        .server_mute(&call.owner.server_key, member_id, true)
        .await
        .unwrap();
    talking.await.unwrap();

    let last = call.owner_voice.heard()[&member_id].last.unwrap();
    assert!(
        last <= muted_at + Duration::from_millis(100),
        "audio kept coming {:?} after the mute",
        last.saturating_duration_since(muted_at)
    );
}

#[tokio::test]
async fn a_deafened_voicebot_receives_no_audio_packets() {
    let call = call().await;
    call.member.client.set_voice_self(None, Some(true));
    tokio::time::sleep(Duration::from_millis(200)).await;
    call.member_voice.clear_heard();

    call.owner_voice
        .play_tone(440.0, Duration::from_millis(600))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert!(
        call.member_voice.heard().is_empty(),
        "{:?}",
        call.member_voice.heard()
    );
}

#[tokio::test]
async fn a_voice_gateway_resume_does_not_interrupt_media() {
    let call = call().await;
    let member_id = call.member.user_id();
    let talking = call.member_voice.play_tone(440.0, Duration::from_secs(2));
    call.owner_voice
        .wait_until(|view| view.heard.get(&member_id).is_some_and(|h| h.decoded >= 10))
        .await
        .unwrap();

    let dropped_at = Instant::now();
    call.owner_voice.drop_gateway();
    talking.await.unwrap();

    let heard = &call.owner_voice.heard()[&member_id];
    let gap = longest_gap(dropped_at, &heard.arrivals);
    assert!(gap < Duration::from_millis(200), "a gap of {gap:?}");
    assert_eq!(
        call.owner_voice.connections(),
        1,
        "it resumed, not rejoined"
    );
    assert_eq!(call.owner_voice.closed(), None);
}

#[tokio::test]
async fn the_app_learns_who_starts_and_stops_speaking() {
    let server = TestServer::start().await;
    let owner = Voicebot::connect(
        &server.address(),
        server.handle.claim_token.clone(),
        "Owner",
    )
    .await
    .unwrap();
    let invite = owner
        .client
        .create_invite(&owner.server_key, None, None)
        .await
        .unwrap();
    // The member runs the app's voice media, without devices.
    let member = Voicebot::connect(&invite.link, None, "Member")
        .await
        .unwrap();
    let mut media = member
        .client
        .enable_media(MediaOptions {
            open_devices: false,
        })
        .unwrap();
    let general = owner.voice_channel("General").unwrap();
    let owner_voice = owner.join(general).await.unwrap();
    member
        .client
        .voice_join(&member.server_key, general)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(15), async {
        while let Some(event) = media.recv().await {
            if let MediaEvent::ConnectionState {
                state: VoiceConnectionState::Connected,
                ..
            } = event
            {
                return;
            }
        }
    })
    .await
    .expect("the member's voice media did not connect");

    owner_voice.play_tone(440.0, Duration::from_millis(600));
    let changes = tokio::time::timeout(Duration::from_secs(5), async {
        let mut changes = Vec::new();
        while let Some(event) = media.recv().await {
            if let MediaEvent::Speaking {
                server_key,
                channel_id,
                changes: these,
            } = event
            {
                assert_eq!(
                    (server_key.as_str(), channel_id),
                    (member.server_key.as_str(), general)
                );
                changes.extend(these);
                if changes.len() >= 2 {
                    return changes;
                }
            }
        }
        changes
    })
    .await
    .expect("no speaking changes arrived");

    let owner_id = owner.user_id();
    assert_eq!(
        changes,
        vec![
            SpeakingChange {
                user_id: owner_id,
                speaking: true
            },
            SpeakingChange {
                user_id: owner_id,
                speaking: false
            },
        ]
    );
}

/// Waits until `watcher` has `count` more intact pictures from `user_id`
/// than `before`.
async fn sees_camera(watcher: &VoiceSession, user_id: i64, before: u64, count: u64) {
    watcher
        .wait_until(|view| {
            view.seen
                .get(&user_id)
                .is_some_and(|seen| seen.intact >= before + count)
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn a_voicebots_camera_reaches_another_voicebot_intact() {
    let call = call().await;
    let owner = call.owner.user_id();

    call.owner_voice.publish_camera("cam-owner").await.unwrap();
    call.member_voice.watch(360, false);
    sees_camera(&call.member_voice, owner, 0, 30).await;

    let seen = call.member_voice.seen()[&owner].clone();
    assert_eq!(seen.intact, seen.frames, "a picture arrived damaged");
    assert_eq!(seen.layers[2], 0, "taller than the tile");
    assert!(seen.keyframes >= 1);
}

#[tokio::test]
async fn a_voicebots_camera_comes_back_when_its_call_moves_to_another_node() {
    let mut call = external_call().await;
    let owner = call.owner.user_id();
    call.owner_voice.publish_camera("cam-owner").await.unwrap();
    call.member_voice.watch(360, false);
    sees_camera(&call.member_voice, owner, 0, 10).await;

    let first = call.owner_voice.gateway_url();
    call.cluster.kill(&first).await;
    for voice in [&call.owner_voice, &call.member_voice] {
        voice
            .wait_until(|view| view.connections == 2 && view.media_connected)
            .await
            .unwrap();
    }
    let before = call.member_voice.seen()[&owner].intact;

    sees_camera(&call.member_voice, owner, before, 10).await;
}

#[tokio::test]
async fn a_voicebots_encoded_camera_is_decoded_by_another_voicebot() {
    let call = call().await;
    let owner = call.owner.user_id();

    call.owner_voice
        .publish_encoded_camera("cam-owner")
        .await
        .unwrap();
    call.member_voice.watch(360, true);
    call.member_voice
        .wait_until(|view| view.seen.get(&owner).is_some_and(|seen| seen.decoded >= 30))
        .await
        .unwrap();

    let seen = call.member_voice.seen()[&owner].clone();
    assert!(seen.decoded + 1 >= seen.frames, "{seen:?}");
    assert_eq!(seen.intact, 0, "real H.264, not the test pattern");
    assert_eq!(seen.layers[2], 0, "taller than the tile");
    assert!(seen.layers[1] > 0, "{seen:?}");
}
