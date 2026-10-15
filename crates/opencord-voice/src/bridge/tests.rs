//! The bridge against a stand-in for Lavalink that speaks the voice
//! protocol as Lavalink's voice client (Koe 3) does.

use std::net::SocketAddr;
use std::time::Duration;

use axum::Router;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{ConnectInfo, State};
use axum::response::Response;
use axum::routing::get;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

use super::rtp::{self, Mode};
use super::*;

const CHANNEL: i64 = 1_234_567_890_123;
const LAVALINK_USER: i64 = 99;
const OPUS: &[u8] = &[0xfc, 0xff, 0xfe];

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

struct Harness {
    bridge: MusicBridge,
    frames: mpsc::Receiver<MusicFrame>,
    events: mpsc::UnboundedReceiver<BridgeEvent>,
    url: String,
}

async fn harness() -> Harness {
    let (bridge, frames, events) = MusicBridge::start(BridgeConfig {
        udp_port: 0,
        public_ip: None,
        heartbeat: Duration::from_millis(500),
    })
    .await
    .unwrap();
    async fn upgrade(
        State(bridge): State<MusicBridge>,
        ConnectInfo(peer): ConnectInfo<SocketAddr>,
        upgrade: WebSocketUpgrade,
    ) -> Response {
        upgrade.on_upgrade(move |socket| async move { bridge.serve(socket, peer).await })
    }
    let app = Router::new()
        .route("/", get(upgrade))
        .with_state(bridge.clone())
        .into_make_service_with_connect_info::<SocketAddr>();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/?v=8", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await });
    Harness {
        bridge,
        frames,
        events,
        url,
    }
}

fn player(token: &str) -> ExpectedPlayer {
    ExpectedPlayer {
        channel_id: CHANNEL,
        user_id: LAVALINK_USER,
        token: token.to_owned(),
        session_id: "lavalink-session".to_owned(),
        allowed: None,
    }
}

async fn send(socket: &mut Socket, message: Value) {
    socket
        .send(Message::Text(message.to_string().into()))
        .await
        .unwrap();
}

/// The next text message, skipping nothing else.
async fn receive(socket: &mut Socket) -> Value {
    loop {
        let message = tokio::time::timeout(Duration::from_secs(5), socket.next())
            .await
            .expect("the bridge answers")
            .expect("the socket stays open")
            .unwrap();
        if let Message::Text(text) = message {
            return serde_json::from_str(text.as_str()).unwrap();
        }
    }
}

/// The close code the bridge ends with.
async fn closed(socket: &mut Socket) -> u16 {
    loop {
        match tokio::time::timeout(Duration::from_secs(5), socket.next())
            .await
            .expect("the bridge closes")
        {
            Some(Ok(Message::Close(Some(frame)))) => return u16::from(frame.code),
            Some(Ok(_)) => {}
            other => panic!("ended without a close frame: {other:?}"),
        }
    }
}

fn identify(token: &str) -> Value {
    json!({
        "op": 0,
        "d": {
            "server_id": CHANNEL.to_string(),
            "user_id": LAVALINK_USER.to_string(),
            "max_dave_protocol_version": 1,
            "session_id": "lavalink-session",
            "token": token,
            "video": true,
        },
    })
}

/// A connected stand-in: through Identify, IP discovery and Select
/// Protocol, holding the key.
struct Lavalink {
    socket: Socket,
    ssrc: u32,
    key: [u8; 32],
    audio_to: SocketAddr,
    /// Sends audio, from another port than discovery used.
    sender: UdpSocket,
    sequence: u16,
    last_seq: i64,
}

impl Lavalink {
    async fn connect(url: &str, token: &str, mode: Mode) -> Self {
        let (mut socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        let hello = receive(&mut socket).await;
        assert_eq!(hello["op"], 8);
        assert!(hello["d"]["heartbeat_interval"].as_u64().unwrap() > 0);
        send(&mut socket, identify(token)).await;
        let ready = receive(&mut socket).await;
        assert_eq!(ready["op"], 2, "{ready}");
        let ssrc = u32::try_from(ready["d"]["ssrc"].as_u64().unwrap()).unwrap();
        let modes: Vec<&str> = ready["d"]["modes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|mode| mode.as_str().unwrap())
            .collect();
        assert!(modes.contains(&"aead_xchacha20_poly1305_rtpsize"));
        let ip: std::net::IpAddr = ready["d"]["ip"].as_str().unwrap().parse().unwrap();
        let port = u16::try_from(ready["d"]["port"].as_u64().unwrap()).unwrap();
        let audio_to = SocketAddr::new(ip, port);
        assert!(ip.is_loopback(), "a node on this machine uses loopback");

        let discovery = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let mut request = [0u8; rtp::DISCOVERY_LEN];
        request[..2].copy_from_slice(&1u16.to_be_bytes());
        request[2..4].copy_from_slice(&70u16.to_be_bytes());
        request[4..8].copy_from_slice(&ssrc.to_be_bytes());
        discovery.send_to(&request, audio_to).await.unwrap();
        let mut response = [0u8; 128];
        let (size, _) =
            tokio::time::timeout(Duration::from_secs(5), discovery.recv_from(&mut response))
                .await
                .expect("discovery is answered")
                .unwrap();
        assert_eq!(size, rtp::DISCOVERY_LEN);
        let seen_port = u16::from_be_bytes([response[72], response[73]]);
        assert_eq!(seen_port, discovery.local_addr().unwrap().port());

        send(
            &mut socket,
            json!({
                "op": 1,
                "d": {
                    "protocol": "udp",
                    "codecs": [],
                    "rtc_connection_id": "x",
                    "data": { "address": "127.0.0.1", "port": seen_port, "mode": mode.name() },
                    "address": "127.0.0.1",
                    "port": seen_port,
                    "mode": mode.name(),
                },
            }),
        )
        .await;
        let description = receive(&mut socket).await;
        assert_eq!(description["op"], 4, "{description}");
        assert_eq!(description["d"]["dave_protocol_version"], 0);
        assert_eq!(description["d"]["mode"], mode.name());
        let key: Vec<u8> = description["d"]["secret_key"]
            .as_array()
            .unwrap()
            .iter()
            .map(|byte| u8::try_from(byte.as_u64().unwrap()).unwrap())
            .collect();
        let last_seq = description["seq"].as_i64().unwrap();
        Self {
            socket,
            ssrc,
            key: key.try_into().unwrap(),
            audio_to,
            sender: UdpSocket::bind("127.0.0.1:0").await.unwrap(),
            sequence: 100,
            last_seq,
        }
    }

    async fn play(&mut self, mode: Mode) {
        self.sequence = self.sequence.wrapping_add(1);
        let header = rtp::header(
            120,
            self.sequence,
            u32::from(self.sequence) * 960,
            self.ssrc,
        );
        let packet = rtp::seal(mode, &self.key, u32::from(self.sequence), header, OPUS);
        self.sender.send_to(&packet, self.audio_to).await.unwrap();
    }
}

async fn next_frame(frames: &mut mpsc::Receiver<MusicFrame>) -> MusicFrame {
    tokio::time::timeout(Duration::from_secs(5), frames.recv())
        .await
        .expect("a frame comes")
        .unwrap()
}

#[tokio::test]
async fn lavalinks_audio_arrives_decrypted_in_either_mode() {
    for mode in rtp::MODES {
        let mut harness = harness().await;
        harness.bridge.expect(player("t1"));

        let mut lavalink = Lavalink::connect(&harness.url, "t1", mode).await;
        lavalink.play(mode).await;
        let frame = next_frame(&mut harness.frames).await;

        assert_eq!(frame.channel_id, CHANNEL);
        assert_eq!(frame.packet.sequence, lavalink.sequence);
        assert_eq!(&frame.packet.opus[..], OPUS);
        assert!(matches!(
            harness.events.recv().await,
            Some(BridgeEvent::Connected {
                channel_id: CHANNEL,
                ..
            })
        ));
        assert_eq!(
            harness.events.recv().await,
            Some(BridgeEvent::Discovered {
                channel_id: CHANNEL,
                from: "127.0.0.1".parse().unwrap(),
            })
        );
        assert_eq!(harness.bridge.stats().packets, 1);
    }
}

#[tokio::test]
async fn heartbeats_are_acknowledged() {
    let harness = harness().await;
    harness.bridge.expect(player("t1"));
    let mut lavalink = Lavalink::connect(&harness.url, "t1", Mode::Aes256Gcm).await;

    send(
        &mut lavalink.socket,
        json!({ "op": 3, "d": { "t": 1234, "seq_ack": 2 } }),
    )
    .await;
    let ack = receive(&mut lavalink.socket).await;

    assert_eq!(ack, json!({ "op": 6, "d": { "t": 1234 } }));
}

#[tokio::test]
async fn a_wrong_or_spent_token_is_refused() {
    let harness = harness().await;
    harness.bridge.expect(player("right"));
    for token in ["wrong", "righ", ""] {
        let (mut socket, _) = tokio_tungstenite::connect_async(&harness.url)
            .await
            .unwrap();
        receive(&mut socket).await;
        send(&mut socket, identify(token)).await;
        assert_eq!(closed(&mut socket).await, close::AUTHENTICATION_FAILED);
    }
    let _first = Lavalink::connect(&harness.url, "right", Mode::Aes256Gcm).await;

    let (mut again, _) = tokio_tungstenite::connect_async(&harness.url)
        .await
        .unwrap();
    receive(&mut again).await;
    send(&mut again, identify("right")).await;

    assert_eq!(closed(&mut again).await, close::AUTHENTICATION_FAILED);
}

#[tokio::test]
async fn an_unknown_mode_or_protocol_is_refused() {
    let harness = harness().await;
    harness.bridge.expect(player("t1"));
    let (mut socket, _) = tokio_tungstenite::connect_async(&harness.url)
        .await
        .unwrap();
    receive(&mut socket).await;
    send(&mut socket, identify("t1")).await;
    receive(&mut socket).await;

    send(
        &mut socket,
        json!({ "op": 1, "d": { "protocol": "udp", "data": { "mode": "xsalsa20_poly1305" } } }),
    )
    .await;

    assert_eq!(closed(&mut socket).await, close::UNKNOWN_ENCRYPTION_MODE);
}

#[tokio::test]
async fn audio_from_elsewhere_or_tampered_is_dropped() {
    let mut harness = harness().await;
    harness.bridge.expect(player("t1"));
    let mut lavalink = Lavalink::connect(&harness.url, "t1", Mode::Aes256Gcm).await;
    let stranger = UdpSocket::bind("[::1]:0").await;

    // Another SSRC, and a packet sealed with another key.
    let header = rtp::header(120, 1, 0, lavalink.ssrc ^ 1);
    let other_ssrc = rtp::seal(Mode::Aes256Gcm, &lavalink.key, 1, header, OPUS);
    lavalink
        .sender
        .send_to(&other_ssrc, lavalink.audio_to)
        .await
        .unwrap();
    let header = rtp::header(120, 2, 0, lavalink.ssrc);
    let wrong_key = rtp::seal(Mode::Aes256Gcm, &[0; 32], 2, header, OPUS);
    lavalink
        .sender
        .send_to(&wrong_key, lavalink.audio_to)
        .await
        .unwrap();
    if let Ok(stranger) = stranger {
        // The right packet, from an address that did not discover.
        let header = rtp::header(120, 3, 0, lavalink.ssrc);
        let packet = rtp::seal(Mode::Aes256Gcm, &lavalink.key, 3, header, OPUS);
        let to = SocketAddr::new("::1".parse().unwrap(), lavalink.audio_to.port());
        let _ = stranger.send_to(&packet, to).await;
    }
    lavalink.play(Mode::Aes256Gcm).await;

    let frame = next_frame(&mut harness.frames).await;
    assert_eq!(
        frame.packet.sequence, lavalink.sequence,
        "only the good packet"
    );
    let stats = harness.bridge.stats();
    assert_eq!(stats.packets, 1);
    assert_eq!(stats.decrypt_failures, 1);
    assert!(stats.dropped >= 1);
}

#[tokio::test]
async fn a_dropped_websocket_resumes_and_audio_goes_on() {
    let mut harness = harness().await;
    harness.bridge.expect(player("t1"));
    let mut lavalink = Lavalink::connect(&harness.url, "t1", Mode::XChaCha20Poly1305).await;
    lavalink.socket.close(None).await.unwrap();

    let (mut socket, _) = tokio_tungstenite::connect_async(&harness.url)
        .await
        .unwrap();
    receive(&mut socket).await;
    send(
        &mut socket,
        json!({
            "op": 7,
            "d": {
                "server_id": CHANNEL.to_string(),
                "session_id": "lavalink-session",
                "token": "t1",
                "video": true,
                "seq_ack": lavalink.last_seq,
            },
        }),
    )
    .await;

    assert_eq!(receive(&mut socket).await["op"], 9);
    lavalink.play(Mode::XChaCha20Poly1305).await;
    assert_eq!(&next_frame(&mut harness.frames).await.packet.opus[..], OPUS);
}

#[tokio::test]
async fn a_resume_replays_what_was_not_acknowledged() {
    let harness = harness().await;
    harness.bridge.expect(player("t1"));
    let lavalink = Lavalink::connect(&harness.url, "t1", Mode::Aes256Gcm).await;
    drop(lavalink.socket);

    let (mut socket, _) = tokio_tungstenite::connect_async(&harness.url)
        .await
        .unwrap();
    receive(&mut socket).await;
    // As if Ready were the last message seen: the Session Description again.
    let resume = json!({
        "op": 7,
        "d": {
            "server_id": CHANNEL.to_string(),
            "session_id": "lavalink-session",
            "token": "t1",
            "seq_ack": lavalink.last_seq - 1,
        },
    });
    send(&mut socket, resume).await;

    assert_eq!(receive(&mut socket).await["op"], 9);
    let replayed = receive(&mut socket).await;
    assert_eq!(replayed["op"], 4);
    assert_eq!(replayed["seq"], lavalink.last_seq);
}

#[tokio::test]
async fn a_new_player_revokes_the_old_connection() {
    let mut harness = harness().await;
    harness.bridge.expect(player("old"));
    let mut old = Lavalink::connect(&harness.url, "old", Mode::Aes256Gcm).await;

    harness.bridge.expect(player("new"));

    assert_eq!(
        closed(&mut old.socket).await,
        close::SESSION_NO_LONGER_VALID
    );
    old.play(Mode::Aes256Gcm).await;
    let mut new = Lavalink::connect(&harness.url, "new", Mode::Aes256Gcm).await;
    new.play(Mode::Aes256Gcm).await;
    let frame = next_frame(&mut harness.frames).await;
    assert_eq!(
        frame.packet.sequence, new.sequence,
        "the old node's audio is refused"
    );
    assert!(harness.frames.try_recv().is_err());
}

#[tokio::test]
async fn cancelling_disconnects_lavalink() {
    let harness = harness().await;
    harness.bridge.expect(player("t1"));
    let mut lavalink = Lavalink::connect(&harness.url, "t1", Mode::Aes256Gcm).await;

    harness.bridge.cancel(CHANNEL);

    assert_eq!(closed(&mut lavalink.socket).await, close::DISCONNECTED);
}

#[tokio::test]
async fn a_strict_node_must_come_from_its_addresses() {
    let mut harness = harness().await;
    harness.bridge.expect(ExpectedPlayer {
        allowed: Some(vec!["192.0.2.7".parse().unwrap()]),
        ..player("t1")
    });
    let (mut socket, _) = tokio_tungstenite::connect_async(&harness.url)
        .await
        .unwrap();
    receive(&mut socket).await;

    send(&mut socket, identify("t1")).await;

    assert_eq!(closed(&mut socket).await, close::AUTHENTICATION_FAILED);
    assert_eq!(
        harness.events.recv().await,
        Some(BridgeEvent::Refused {
            channel_id: CHANNEL,
            peer: "127.0.0.1".parse().unwrap(),
        })
    );
}

#[tokio::test]
async fn silence_past_two_heartbeats_times_out() {
    let harness = harness().await;
    harness.bridge.expect(player("t1"));
    let mut lavalink = Lavalink::connect(&harness.url, "t1", Mode::Aes256Gcm).await;

    // Half a second's heartbeat, missed: closed with 4009 (resumable).
    let code = tokio::time::timeout(Duration::from_secs(10), closed(&mut lavalink.socket))
        .await
        .unwrap();

    assert_eq!(code, u16::from(CloseCode::Library(close::SESSION_TIMEOUT)));
}

#[test]
fn private_and_shared_addresses_count_as_local() {
    for local in [
        "127.0.0.1",
        "10.1.2.3",
        "192.168.1.5",
        "100.100.1.1",
        "fd00::1",
        "::1",
    ] {
        assert!(is_local(local.parse().unwrap()), "{local}");
    }
    for public in ["203.0.113.9", "8.8.8.8", "2001:db8::1", "100.128.0.1"] {
        assert!(!is_local(public.parse().unwrap()), "{public}");
    }
}
