//! One WebSocket from Lavalink: the server side of Discord's voice
//! gateway, v8, as far as Lavalink uses it (Phase 3 plan §13.2). Hello,
//! then Identify (or Resume), Ready, Select Protocol and the Session
//! Description; heartbeats until it closes.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use axum::extract::ws::{CloseFrame, Message, WebSocket};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::time::Instant;

use super::rtp::{MODES, Mode, Opener};
use super::{Attached, BridgeEvent, Live, MusicBridge, RESUME_BUFFER, close};

/// Time Lavalink has to identify after Hello.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// Lavalink's messages are small; longer ones are refused.
const MAX_MESSAGE: usize = 16 * 1024;
const OUTBOUND_CAPACITY: usize = 64;

mod op {
    pub const IDENTIFY: u64 = 0;
    pub const SELECT_PROTOCOL: u64 = 1;
    pub const READY: u64 = 2;
    pub const HEARTBEAT: u64 = 3;
    pub const SESSION_DESCRIPTION: u64 = 4;
    pub const HEARTBEAT_ACK: u64 = 6;
    pub const RESUME: u64 = 7;
    pub const HELLO: u64 = 8;
    pub const RESUMED: u64 = 9;
}

static NEXT_SOCKET: AtomicU64 = AtomicU64::new(1);

pub(super) enum Outbound {
    Text(String),
    Close(u16),
}

#[derive(Deserialize)]
struct Identify {
    server_id: String,
    user_id: String,
    session_id: String,
    token: String,
}

#[derive(Deserialize)]
struct Resume {
    server_id: String,
    session_id: String,
    token: String,
    #[serde(default)]
    seq_ack: Option<i64>,
}

#[derive(Deserialize)]
struct SelectProtocol {
    protocol: String,
    #[serde(default)]
    data: Option<SelectData>,
    #[serde(default)]
    mode: Option<String>,
}

#[derive(Deserialize)]
struct SelectData {
    #[serde(default)]
    mode: Option<String>,
}

/// What a connection is, once its handshake is done.
#[derive(Clone, Copy)]
struct Session {
    channel_id: i64,
    socket: u64,
}

pub(super) async fn serve(bridge: MusicBridge, socket: WebSocket, peer: SocketAddr) {
    let peer = peer.ip().to_canonical();
    let (sink, mut stream) = socket.split();
    let (outbound, frames) = mpsc::channel(OUTBOUND_CAPACITY);
    let writer = tokio::spawn(write(sink, frames));
    let attached = Attached {
        id: NEXT_SOCKET.fetch_add(1, Ordering::Relaxed),
        outbound,
    };
    if !bridge.attempt(peer) {
        let _ = attached
            .outbound
            .send(Outbound::Close(close::RATE_LIMITED))
            .await;
    } else {
        let heartbeat = bridge.config.heartbeat;
        let hello = json!({
            "op": op::HELLO,
            "d": { "v": 8, "heartbeat_interval": heartbeat.as_millis() as u64 },
        });
        let _ = attached
            .outbound
            .send(Outbound::Text(hello.to_string()))
            .await;
        match handshake(&bridge, &mut stream, &attached, peer).await {
            Ok(session) => {
                let code = run(&bridge, &mut stream, &attached, session, heartbeat).await;
                bridge.detach(session, code);
                if let Some(code) = code {
                    let _ = attached.outbound.send(Outbound::Close(code)).await;
                }
            }
            Err(Some(code)) => {
                let _ = attached.outbound.send(Outbound::Close(code)).await;
            }
            Err(None) => {}
        }
    }
    drop(attached);
    let _ = writer.await;
}

async fn write(mut sink: SplitSink<WebSocket, Message>, mut frames: mpsc::Receiver<Outbound>) {
    while let Some(outbound) = frames.recv().await {
        match outbound {
            Outbound::Text(text) => {
                if sink.send(Message::Text(text.into())).await.is_err() {
                    return;
                }
            }
            Outbound::Close(code) => {
                let frame = CloseFrame {
                    code,
                    reason: "".into(),
                };
                let _ = sink.send(Message::Close(Some(frame))).await;
                let _ = sink.close().await;
                return;
            }
        }
    }
    let _ = sink.close().await;
}

enum Next {
    Message {
        op: u64,
        data: Value,
    },
    /// Binary frames (DAVE's): nothing to do with them at version 0.
    Ignored,
    Closed,
    Invalid,
}

async fn next(stream: &mut SplitStream<WebSocket>) -> Next {
    loop {
        let message = match stream.next().await {
            Some(Ok(message)) => message,
            Some(Err(_)) | None => return Next::Closed,
        };
        let text = match message {
            Message::Text(text) => text,
            Message::Binary(_) => return Next::Ignored,
            Message::Close(_) => return Next::Closed,
            Message::Ping(_) | Message::Pong(_) => continue,
        };
        if text.len() > MAX_MESSAGE {
            return Next::Invalid;
        }
        let Ok(value) = serde_json::from_str::<Value>(text.as_str()) else {
            return Next::Invalid;
        };
        let Some(op) = value.get("op").and_then(Value::as_u64) else {
            return Next::Invalid;
        };
        let data = value.get("d").cloned().unwrap_or(Value::Null);
        return Next::Message { op, data };
    }
}

/// Identify or Resume. `Err` carries the close code, if one is due.
async fn handshake(
    bridge: &MusicBridge,
    stream: &mut SplitStream<WebSocket>,
    attached: &Attached,
    peer: IpAddr,
) -> Result<Session, Option<u16>> {
    let deadline = Instant::now() + HANDSHAKE_TIMEOUT;
    loop {
        let message = tokio::time::timeout_at(deadline, next(stream))
            .await
            .map_err(|_| Some(close::NOT_AUTHENTICATED))?;
        let (op, data) = match message {
            Next::Message { op, data } => (op, data),
            Next::Ignored => continue,
            Next::Closed => return Err(None),
            Next::Invalid => return Err(Some(close::BAD_REQUEST)),
        };
        match op {
            op::HEARTBEAT => ack(attached, &data).await,
            op::IDENTIFY => {
                let identify: Identify =
                    serde_json::from_value(data).map_err(|_| Some(close::BAD_REQUEST))?;
                return bridge.identify(identify, attached, peer);
            }
            op::RESUME => {
                let resume: Resume =
                    serde_json::from_value(data).map_err(|_| Some(close::BAD_REQUEST))?;
                return bridge.resume(resume, attached);
            }
            _ => return Err(Some(close::NOT_AUTHENTICATED)),
        }
    }
}

/// After the handshake: heartbeats and Select Protocol. Returns the code
/// to close with, if any.
async fn run(
    bridge: &MusicBridge,
    stream: &mut SplitStream<WebSocket>,
    attached: &Attached,
    session: Session,
    heartbeat: Duration,
) -> Option<u16> {
    // Two missed heartbeats and a half.
    let allowed_silence = heartbeat * 5 / 2;
    let mut deadline = Instant::now() + allowed_silence;
    loop {
        let message = match tokio::time::timeout_at(deadline, next(stream)).await {
            Err(_) => return Some(close::SESSION_TIMEOUT),
            Ok(message) => message,
        };
        let (op, data) = match message {
            Next::Message { op, data } => (op, data),
            Next::Ignored => continue,
            Next::Closed => return None,
            Next::Invalid => return Some(close::BAD_REQUEST),
        };
        match op {
            op::HEARTBEAT => {
                deadline = Instant::now() + allowed_silence;
                ack(attached, &data).await;
            }
            op::SELECT_PROTOCOL => {
                let Ok(select) = serde_json::from_value::<SelectProtocol>(data) else {
                    return Some(close::BAD_REQUEST);
                };
                if select.protocol != "udp" {
                    return Some(close::UNKNOWN_PROTOCOL);
                }
                let mode = select
                    .data
                    .and_then(|data| data.mode)
                    .or(select.mode)
                    .as_deref()
                    .and_then(Mode::from_name);
                let Some(mode) = mode else {
                    return Some(close::UNKNOWN_ENCRYPTION_MODE);
                };
                bridge.select(session, mode);
            }
            op::IDENTIFY | op::RESUME => return Some(close::ALREADY_AUTHENTICATED),
            // Speaking, video, media sink wants, client flags, platform:
            // nothing the bridge needs.
            _ => {}
        }
    }
}

async fn ack(attached: &Attached, data: &Value) {
    let nonce = data.get("t").cloned().unwrap_or_else(|| data.clone());
    let ack = json!({ "op": op::HEARTBEAT_ACK, "d": { "t": nonce } });
    let _ = attached
        .outbound
        .send(Outbound::Text(ack.to_string()))
        .await;
}

impl MusicBridge {
    fn identify(
        &self,
        identify: Identify,
        attached: &Attached,
        peer: IpAddr,
    ) -> Result<Session, Option<u16>> {
        let refused = Err(Some(close::AUTHENTICATION_FAILED));
        let Ok(channel_id) = identify.server_id.parse::<i64>() else {
            return refused;
        };
        let mut shared = self.lock();
        let Some((player, _)) = shared.expected.get(&channel_id) else {
            return refused;
        };
        let signed_in = identify.user_id == player.user_id.to_string()
            && same(&identify.token, &player.token)
            && same(&identify.session_id, &player.session_id);
        let from_allowed = player
            .allowed
            .as_ref()
            .is_none_or(|allowed| allowed.contains(&peer));
        if !(signed_in && from_allowed) {
            drop(shared);
            if signed_in {
                let _ = self
                    .config
                    .events
                    .send(BridgeEvent::Refused { channel_id, peer });
            }
            return refused;
        }
        // Single use: the token is spent.
        let Some((player, _)) = shared.expected.remove(&channel_id) else {
            return refused;
        };
        let ended = shared.remove_live(channel_id);
        let ssrc = loop {
            let ssrc = random_u32();
            if ssrc != 0 && !shared.by_ssrc.contains_key(&ssrc) {
                break ssrc;
            }
        };
        shared.next_connection += 1;
        let connection = shared.next_connection;
        shared.by_ssrc.insert(ssrc, channel_id);
        shared.live.insert(
            channel_id,
            Live {
                connection,
                player,
                ssrc,
                media: None,
                discovered: None,
                heard: false,
                last_audio: None,
                flowing: false,
                jitter: 0.0,
                last_packet: None,
                socket: Some(attached.clone()),
                detached_at: None,
                sent: Default::default(),
                seq: 0,
            },
        );
        drop(shared);
        self.ended(channel_id, ended, close::SESSION_NO_LONGER_VALID);
        let ready = json!({
            "op": op::READY,
            "d": {
                "ssrc": ssrc,
                "ip": self.audio_ip(peer).to_string(),
                "port": self.config.udp_port,
                "modes": MODES.iter().map(|mode| mode.name()).collect::<Vec<_>>(),
                "heartbeat_interval": self.config.heartbeat.as_millis() as u64,
            },
        });
        let session = Session {
            channel_id,
            socket: attached.id,
        };
        self.send(session, ready);
        let _ = self.config.events.send(BridgeEvent::Connected {
            channel_id,
            connection,
        });
        Ok(session)
    }

    fn resume(&self, resume: Resume, attached: &Attached) -> Result<Session, Option<u16>> {
        let invalid = Err(Some(close::SESSION_NO_LONGER_VALID));
        let Ok(channel_id) = resume.server_id.parse::<i64>() else {
            return invalid;
        };
        let mut shared = self.lock();
        let Some(live) = shared.live.get_mut(&channel_id) else {
            return invalid;
        };
        if !same(&resume.token, &live.player.token)
            || !same(&resume.session_id, &live.player.session_id)
        {
            return invalid;
        }
        // What Lavalink missed must still be buffered, or it starts over.
        let acked = resume.seq_ack.unwrap_or(-1);
        let oldest = live.sent.front().map_or(live.seq + 1, |(seq, _)| *seq);
        if i64::from(oldest) > acked + 1 {
            return invalid;
        }
        if let Some(old) = live.socket.replace(attached.clone()) {
            let _ = old.outbound.try_send(Outbound::Close(1000));
        }
        live.detached_at = None;
        let missed: Vec<String> = live
            .sent
            .iter()
            .filter(|(seq, _)| i64::from(*seq) > acked)
            .map(|(_, text)| text.clone())
            .collect();
        drop(shared);
        let resumed = json!({ "op": op::RESUMED, "d": null });
        let _ = attached
            .outbound
            .try_send(Outbound::Text(resumed.to_string()));
        for text in missed {
            let _ = attached.outbound.try_send(Outbound::Text(text));
        }
        Ok(Session {
            channel_id,
            socket: attached.id,
        })
    }

    /// Select Protocol: a fresh key, and the Session Description with it.
    fn select(&self, session: Session, mode: Mode) {
        let mut key = [0u8; 32];
        getrandom::fill(&mut key).expect("the system has randomness");
        {
            let mut shared = self.lock();
            let Some(live) = shared.live.get_mut(&session.channel_id) else {
                return;
            };
            live.media = Some(Arc::new(Opener::new(mode, &key)));
        }
        let description = json!({
            "op": op::SESSION_DESCRIPTION,
            "d": {
                "mode": mode.name(),
                "secret_key": key.to_vec(),
                // No end-to-end encryption: Lavalink sends plain Opus.
                "dave_protocol_version": 0,
                "audio_codec": "opus",
                "video_codec": "H264",
                "media_session_id": format!("{:016x}", random_u32()),
            },
        });
        self.send(session, description);
    }

    /// Sends a message Lavalink acknowledges, numbered for v8's resume.
    fn send(&self, session: Session, mut message: Value) {
        let mut shared = self.lock();
        let Some(live) = shared.live.get_mut(&session.channel_id) else {
            return;
        };
        live.seq += 1;
        message["seq"] = live.seq.into();
        let text = message.to_string();
        live.sent.push_back((live.seq, text.clone()));
        while live.sent.len() > RESUME_BUFFER {
            live.sent.pop_front();
        }
        if let Some(socket) = live.socket.as_ref().filter(|s| s.id == session.socket) {
            let _ = socket.outbound.try_send(Outbound::Text(text));
        }
    }

    /// The WebSocket ended: the connection may resume, unless it broke the
    /// protocol.
    fn detach(&self, session: Session, code: Option<u16>) {
        let resumable = matches!(code, None | Some(close::SESSION_TIMEOUT));
        let mut shared = self.lock();
        let Some(live) = shared.live.get_mut(&session.channel_id) else {
            return;
        };
        if live.socket.as_ref().is_none_or(|s| s.id != session.socket) {
            return;
        }
        live.socket = None;
        live.detached_at = Some(std::time::Instant::now());
        if !resumable {
            let ended = shared.remove_live(session.channel_id);
            drop(shared);
            self.ended(
                session.channel_id,
                ended,
                code.unwrap_or(close::BAD_REQUEST),
            );
        }
    }

    /// Where Lavalink should send audio: the address it can reach. A node on
    /// this machine or network uses this machine's local address, others
    /// the public one.
    fn audio_ip(&self, peer: IpAddr) -> IpAddr {
        if peer.is_loopback() {
            return peer;
        }
        let local = super::local_address_towards(peer);
        if super::is_local(peer) {
            return local.unwrap_or(peer);
        }
        self.public_ip().or(local).unwrap_or(peer)
    }
}

/// Compares secrets in time that does not depend on where they differ.
fn same(given: &str, expected: &str) -> bool {
    let (given, expected) = (given.as_bytes(), expected.as_bytes());
    given.len() == expected.len()
        && given
            .iter()
            .zip(expected)
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
}

fn random_u32() -> u32 {
    let mut bytes = [0u8; 4];
    getrandom::fill(&mut bytes).expect("the system has randomness");
    u32::from_ne_bytes(bytes)
}
