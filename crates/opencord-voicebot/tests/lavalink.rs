//! Phase 3 L0: an unmodified Lavalink plays into a voice channel through
//! the audio bridge, and a voicebot there hears a minute of it without a
//! gap (plan §21.2). On the way, the bridge drops Lavalink's WebSocket, as
//! a network hiccup would, and Lavalink resumes it.
//!
//! Ignored by default: it needs a Lavalink 4.2 or newer that trusts the
//! server's certificate. `docs/music.md` shows how to run one.
//!
//! - `OPENCORD_TEST_LAVALINK`: its address, `http://127.0.0.1:2333`
//! - `OPENCORD_TEST_LAVALINK_PASSWORD`
//! - `OPENCORD_TEST_BRIDGE_TLS`: a directory with `cert.pem` and `key.pem`
//!   for `localhost`, which that Lavalink trusts
//! - `OPENCORD_TEST_TRACK`: what to play, over a minute of 330 Hz on the
//!   left and 440 Hz on the right: a file path, with its local source.
//!   `docs/music.md` shows how to make one with ffmpeg.
//! - `OPENCORD_TEST_SECONDS`: how long to listen, 60 by default

use std::time::{Duration, Instant};

use futures_util::StreamExt;
use opencord_core::api::types::ChannelKind;
use opencord_server::config::Config;
use opencord_server::server;
use opencord_voice::bridge::ExpectedPlayer;
use opencord_voicebot::Voicebot;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

/// The internal Lavalink user id (plan §12.3).
const LAVALINK_USER: i64 = 1_300_000_000_000_000_002;

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a Lavalink: see the module documentation"]
async fn a_minute_of_lavalink_reaches_the_channel_without_a_gap() {
    let lavalink = env("OPENCORD_TEST_LAVALINK");
    let password = env("OPENCORD_TEST_LAVALINK_PASSWORD");
    let tls = std::path::PathBuf::from(env("OPENCORD_TEST_BRIDGE_TLS"));
    let track = env("OPENCORD_TEST_TRACK");
    let seconds: u64 = std::env::var("OPENCORD_TEST_SECONDS")
        .ok()
        .and_then(|seconds| seconds.parse().ok())
        .unwrap_or(60);

    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.server.bind = "127.0.0.1:0".parse().unwrap();
    config.server.public_host = "localhost".to_owned();
    config.server.data_dir = dir.path().join("data");
    config.tls.cert = Some(tls.join("cert.pem"));
    config.tls.key = Some(tls.join("key.pem"));
    config.voice.udp_port = 0;
    config.music.audio_udp_port = 0;
    let server = server::start(config).await.unwrap();
    let port = server.local_addr.port();
    let owner = Voicebot::connect(
        &format!("127.0.0.1:{port}"),
        server.claim_token.clone(),
        "Listener",
    )
    .await
    .unwrap();
    let channel = owner
        .client
        .create_channel(
            &owner.server_key,
            ChannelKind::Voice,
            "Music".to_owned(),
            None,
            None,
        )
        .await
        .unwrap();
    let listener = owner.join(channel.id).await.unwrap();

    let session = LavalinkSession::open(&lavalink, &password).await;
    let player = ExpectedPlayer {
        channel_id: channel.id,
        user_id: LAVALINK_USER,
        token: "l0-token".to_owned(),
        session_id: "l0-voice-session".to_owned(),
        allowed: None,
    };
    let bridge = server.music_bridge().expect("music is on").clone();
    bridge.expect(player.clone());
    let path = format!("/v4/sessions/{}/players/{}", session.id, channel.id);
    let body = json!({
        "track": { "identifier": track },
        "voice": {
            "token": player.token,
            "endpoint": format!("localhost:{port}"),
            "sessionId": player.session_id,
            "channelId": channel.id.to_string(),
        },
    });
    let (status, reply) = request(&lavalink, &password, "PATCH", &path, Some(&body)).await;
    assert_eq!(status, 200, "Lavalink took the player: {reply}");

    let deadline = Instant::now() + Duration::from_secs(20);
    while listener.music_heard().is_none() {
        assert!(Instant::now() < deadline, "no music reached the channel");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let started = Instant::now();
    tokio::time::sleep(Duration::from_secs(seconds / 3)).await;
    bridge.drop_socket(channel.id);
    tokio::time::sleep(Duration::from_secs(seconds - seconds / 3)).await;
    let heard = listener.music_heard().unwrap();
    let stats = bridge.stats();
    let (_, state) = request(&lavalink, &password, "GET", &path, None).await;
    let _ = request(&lavalink, &password, "DELETE", &path, None).await;

    let longest_wait = heard
        .arrivals
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .max()
        .unwrap_or_default();
    println!(
        "{} packets in {:?}, {} decoded, lost {}, longest wait {longest_wait:?}, peak \
         {:.1} dBFS, bridge {stats:?}",
        heard.packets,
        started.elapsed(),
        heard.decoded,
        heard.lost,
        heard.peak_dbfs,
    );
    println!("player: {state}");
    assert!(listener.music_active());
    assert_eq!(heard.lost, 0, "no frame was lost");
    assert_eq!(heard.decoded, heard.packets, "every frame is plain Opus");
    assert!(
        heard.packets >= seconds * 50 - 10,
        "{} packets",
        heard.packets
    );
    assert!(
        longest_wait < Duration::from_millis(200),
        "{longest_wait:?}"
    );
    // The track is 330 Hz on the left and 440 Hz on the right.
    let (left, right) = (heard.tone_dbfs(0, 330.0), heard.tone_dbfs(1, 440.0));
    let (left_leak, right_leak) = (heard.tone_dbfs(0, 440.0), heard.tone_dbfs(1, 330.0));
    println!("left {left:.1} ({left_leak:.1}), right {right:.1} ({right_leak:.1}) dBFS");
    assert!(
        left > left_leak + 20.0 && right > right_leak + 20.0,
        "stereo arrives as sent"
    );
    assert_eq!(stats.decrypt_failures, 0);
    assert_eq!(stats.connections, 1, "the dropped WebSocket resumed");
    drop(session);
    server.shutdown().await;
}

/// A Lavalink session: its WebSocket, kept open, and the id `ready` gave.
struct LavalinkSession {
    id: String,
    socket: tokio::task::JoinHandle<()>,
}

impl Drop for LavalinkSession {
    fn drop(&mut self) {
        self.socket.abort();
    }
}

impl LavalinkSession {
    async fn open(base: &str, password: &str) -> Self {
        let url = format!("{}/v4/websocket", base.replacen("http", "ws", 1));
        let mut request = url.into_client_request().unwrap();
        let headers = request.headers_mut();
        headers.insert("Authorization", password.parse().unwrap());
        headers.insert("User-Id", LAVALINK_USER.to_string().parse().unwrap());
        headers.insert("Client-Name", "Opencord-test/0.1".parse().unwrap());
        let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
        let id = loop {
            let message = socket.next().await.unwrap().unwrap();
            let Ok(text) = message.into_text() else {
                continue;
            };
            let value: Value = serde_json::from_str(text.as_str()).unwrap();
            if value["op"] == "ready" {
                break value["sessionId"].as_str().unwrap().to_owned();
            }
        };
        let socket = tokio::spawn(async move {
            while let Some(Ok(message)) = socket.next().await {
                if let Ok(text) = message.into_text()
                    && text.contains("\"op\":\"event\"")
                {
                    let event: Value = serde_json::from_str(text.as_str()).unwrap_or_default();
                    println!("lavalink: {} {}", event["type"], event["code"]);
                }
            }
        });
        Self { id, socket }
    }
}

/// One plain HTTP/1.1 request to Lavalink's REST API.
async fn request(
    base: &str,
    password: &str,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> (u16, String) {
    let host = base.trim_start_matches("http://");
    let mut stream = tokio::net::TcpStream::connect(host).await.unwrap();
    let body = body.map(Value::to_string).unwrap_or_default();
    let head = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\nAuthorization: {password}\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await.unwrap();
    stream.write_all(body.as_bytes()).await.unwrap();
    let mut reply = String::new();
    stream.read_to_string(&mut reply).await.unwrap();
    let status = reply
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    let body = reply
        .split("\r\n\r\n")
        .nth(1)
        .unwrap_or_default()
        .to_owned();
    (status, body)
}
