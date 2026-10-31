//! The bridge in production, Phase 3 L3 (plan §12.5, §13): music for a
//! channel on an external voice node goes through that node's bridge, and
//! the audio watchdog restarts a silent player or leaves a node that stops
//! answering. Against the fake Lavalink, whose voice client signs in at the
//! bridge and does IP discovery but sends no audio.

mod common;

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use common::fake_lavalink::FakeLavalink;
use common::music::*;
use common::*;
use opencord_common::address::format_fingerprint;
use opencord_proto::v1 as proto;
use opencord_server::config::{ExternalNode, VoiceMode};
use opencord_server::voice_node::{self, VoiceNodeConfig};
use proto::MusicAction as Action;
use proto::request::Kind;

/// Waits until `check` holds, for at most `within`.
async fn eventually(what: &str, within: Duration, check: impl Fn() -> bool) {
    let deadline = Instant::now() + within;
    while !check() {
        assert!(Instant::now() < deadline, "never: {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Bodies the fake got for `channel_id` with a voice state in them.
fn voice_states(lavalink: &FakeLavalink, channel_id: i64) -> Vec<serde_json::Value> {
    lavalink
        .fake
        .players
        .lock()
        .unwrap()
        .iter()
        .filter(|(guild, body)| *guild == channel_id && body.get("voice").is_some())
        .map(|(_, body)| body["voice"].clone())
        .collect()
}

#[tokio::test]
async fn music_on_an_external_node_goes_through_its_bridge() {
    let dir = tempfile::tempdir().unwrap();
    let secret_file = dir.path().join("node.secret");
    std::fs::write(&secret_file, "a-shared-secret-for-the-music-node").unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("wss://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let server = TestServer::start_with(|config| {
        config.voice.mode = VoiceMode::External;
        config.voice.external_nodes = vec![ExternalNode {
            endpoint: endpoint.clone(),
            secret_file: secret_file.clone(),
        }];
    })
    .await;
    let mut config = VoiceNodeConfig::default();
    config.node.main_server = format!("127.0.0.1:{}", server.handle.local_addr.port());
    config.node.main_fingerprint = Some(format_fingerprint(&server.handle.fingerprint));
    config.node.secret_file = secret_file;
    config.node.endpoint = endpoint.clone();
    config.node.udp_port = 0;
    config.node.data_dir = dir.path().join("node");
    config.music.enabled = true;
    config.music.audio_udp_port = 0;
    let node = voice_node::start_on(config, listener).await.unwrap();
    node.registered().await;

    let lavalink = FakeLavalink::start(PASSWORD).await;
    *lavalink.fake.bridge_fingerprint.lock().unwrap() = Some(node.fingerprint);
    let (mut owner, ready) = owner(&server).await;
    add_node(&mut owner, "Home", &lavalink).await;
    let voice = channel_id(&ready, "General");
    owner.ok(join_voice(voice)).await;
    let checklist = nodes(
        owner
            .ok(Kind::FetchLavalinkNodes(proto::FetchLavalinkNodes {
                watch: false,
            }))
            .await,
    )
    .bridge
    .unwrap();
    assert_eq!(
        checklist.node_bridges,
        [endpoint.trim_start_matches("wss://")]
    );

    let found = search(&mut owner, voice, "song").await;
    owner.ok(enqueue(voice, found)).await;
    fake_plays(&lavalink, voice, "song 1").await;
    let states = voice_states(&lavalink, voice);
    assert_eq!(
        states[0]["endpoint"],
        endpoint.trim_start_matches("wss://"),
        "Lavalink is sent to the node's bridge"
    );
    let bridge = node.music_bridge().unwrap();
    eventually(
        "Lavalink signs in at the node",
        Duration::from_secs(5),
        || bridge.stats().connections == 1,
    )
    .await;

    owner.ok(playback(voice, Action::Stop)).await;
    eventually(
        "the node's bridge lets Lavalink go",
        Duration::from_secs(5),
        || bridge.stats().connections == 0,
    )
    .await;
    drop(owner);
    node.shutdown().await;
}

#[tokio::test]
async fn a_silent_player_is_restarted_once_on_its_node() {
    let Room {
        server,
        mut owner,
        voice,
        lavalink,
        ..
    } = room().await;
    *lavalink.fake.bridge_fingerprint.lock().unwrap() = Some(server.handle.fingerprint);
    let found = search(&mut owner, voice, "song").await;
    owner.ok(enqueue(voice, found)).await;
    fake_plays(&lavalink, voice, "song 1").await;
    assert_eq!(voice_states(&lavalink, voice).len(), 1);

    // Signed in, discovered, then nothing for 2 s: its player starts again
    // with a new voice state, from where it was.
    eventually("the player is restarted", Duration::from_secs(6), || {
        voice_states(&lavalink, voice).len() == 2
    })
    .await;
    let restarted = lavalink
        .fake
        .players
        .lock()
        .unwrap()
        .last()
        .unwrap()
        .1
        .clone();
    assert_eq!(
        restarted["track"]["userData"]["channelId"],
        voice.to_string()
    );

    // Still silent: not restarted again within 30 s.
    tokio::time::sleep(Duration::from_secs(4)).await;
    assert_eq!(voice_states(&lavalink, voice).len(), 2);
}

#[tokio::test]
async fn a_silent_node_that_does_not_answer_loses_the_channel() {
    let Room {
        server,
        mut owner,
        voice,
        lavalink: first,
        node: first_id,
        ..
    } = room().await;
    let second = FakeLavalink::start(PASSWORD).await;
    let second_id = add_node(&mut owner, "Spare", &second).await;
    for lavalink in [&first, &second] {
        *lavalink.fake.bridge_fingerprint.lock().unwrap() = Some(server.handle.fingerprint);
    }
    let found = search(&mut owner, voice, "song").await;
    owner.ok(enqueue(voice, found)).await;
    // Either node may get the channel: the one that plays goes quiet.
    eventually("a node plays", Duration::from_secs(5), || {
        [&first, &second]
            .iter()
            .any(|lavalink| lavalink.fake.player(voice).is_some())
    })
    .await;
    let (silent, silent_id, other) = if first.fake.player(voice).is_some() {
        (&first, first_id, &second)
    } else {
        (&second, second_id, &first)
    };
    silent.fake.info_hangs.store(true, Ordering::Relaxed);
    fake_plays(other, voice, "song 1").await;
    let list = nodes(
        owner
            .ok(Kind::FetchLavalinkNodes(proto::FetchLavalinkNodes {
                watch: false,
            }))
            .await,
    );
    let node = list.nodes.iter().find(|node| node.id == silent_id).unwrap();
    assert_eq!(node.health, proto::LavalinkHealth::Unavailable as i32);
    assert_eq!(
        node.reason,
        "REST is failing: Lavalink stopped sending audio and doesn't answer"
    );
}

/// The bridge's next message, past heartbeat acks.
async fn bridge_message<S>(socket: &mut tokio_tungstenite::WebSocketStream<S>) -> serde_json::Value
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::Message;
    loop {
        let Some(Ok(Message::Text(text))) = socket.next().await else {
            panic!("the bridge closed");
        };
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        if value["op"] != 6 {
            return value;
        }
    }
}

/// Signs in at the bridge as Lavalink would with `token`, for channel 1:
/// the audio address the bridge's Ready names.
async fn ready_ip<S>(socket: &mut tokio_tungstenite::WebSocketStream<S>, token: &str) -> String
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    use futures_util::SinkExt;
    use tokio_tungstenite::tungstenite::Message;
    assert_eq!(bridge_message(socket).await["op"], 8, "Hello");
    let identify = serde_json::json!({
        "op": 0,
        "d": {
            "server_id": "1",
            "user_id": "99",
            "session_id": "lavalink-session",
            "token": token,
        },
    });
    socket
        .send(Message::Text(identify.to_string().into()))
        .await
        .unwrap();
    let ready = bridge_message(socket).await;
    assert_eq!(ready["op"], 2, "{ready}");
    ready["d"]["ip"].as_str().unwrap().to_owned()
}

fn expected(token: &str) -> opencord_voice::bridge::ExpectedPlayer {
    opencord_voice::bridge::ExpectedPlayer {
        channel_id: 1,
        user_id: 99,
        token: token.to_owned(),
        session_id: "lavalink-session".to_owned(),
        allowed: None,
    }
}

/// Through a reverse proxy such as Traefik the bridge sees only the proxy:
/// Lavalink is told the server's public address for audio, as through a
/// Cloudflare tunnel (D70).
#[tokio::test]
async fn lavalink_through_a_proxy_is_told_the_public_address() {
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;
    owner
        .ok(Kind::SetPublicAddress(proto::SetPublicAddress {
            auto_detect: false,
            override_ipv4: "203.0.113.10".to_owned(),
            override_ipv6: String::new(),
        }))
        .await;
    let bridge = server.handle.music_bridge().expect("a bridge").clone();
    let public: std::net::IpAddr = "203.0.113.10".parse().unwrap();
    eventually(
        "the bridge knows the public address",
        Duration::from_secs(5),
        || bridge.public_ip() == Some(public),
    )
    .await;

    // Plain HTTP on the main port: a proxy that took TLS off.
    bridge.expect(expected("through-a-proxy"));
    let tcp = tokio::net::TcpStream::connect(("127.0.0.1", server.handle.local_addr.port()))
        .await
        .unwrap();
    let (mut proxied, _) = tokio_tungstenite::client_async("ws://music.example/?v=8", tcp)
        .await
        .unwrap();
    assert_eq!(
        ready_ip(&mut proxied, "through-a-proxy").await,
        "203.0.113.10"
    );

    // Over TLS from this machine: a Lavalink beside the server, on loopback.
    bridge.expect(expected("direct"));
    let mut direct = websocket(&server, "/?v=8").await.unwrap();
    let ip: std::net::IpAddr = ready_ip(&mut direct, "direct").await.parse().unwrap();
    assert!(ip.is_loopback(), "{ip}");
}
