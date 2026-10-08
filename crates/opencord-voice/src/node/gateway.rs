//! One voice gateway WebSocket: Hello, then Identify or Resume, then
//! heartbeats, the transport and speaking flags until it closes.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use axum::extract::ws::{CloseFrame, Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use opencord_common::voice::close;
use opencord_proto::voice::v1 as voice;
use prost::Message as _;
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;
use voice::envelope::Payload;

use super::VoiceNode;
use super::runtime::Command;
use super::session::{Connection, Outbound, encode};
use super::tracks::MAX_TRACK_ID;

/// Time a client has to answer Hello.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// Messages a client may send per [`RATE_PERIOD`] (plan §14).
const RATE_BURST: u32 = 50;
const RATE_PERIOD: Duration = Duration::from_secs(10);
/// `MediaSinkWants` has its own allowance (plan §14): tiles move often.
const WANTS_BURST: u32 = 20;
const WANTS_PERIOD: Duration = Duration::from_secs(1);
/// More wanted tracks than a full channel could publish are refused.
const MAX_WANTS: usize = 256;
const OUTBOUND_CAPACITY: usize = 256;
/// Larger fields are refused.
const MAX_TOKEN_BYTES: usize = 4096;
const MAX_ICE_FIELD: usize = 256;

static NEXT_CONNECTION_ID: AtomicU64 = AtomicU64::new(1);

pub async fn serve(node: VoiceNode, socket: WebSocket) {
    let (sink, mut stream) = socket.split();
    let (outbound, frames) = mpsc::channel(OUTBOUND_CAPACITY);
    let writer = tokio::spawn(write_frames(sink, frames));
    let connection = Connection {
        id: NEXT_CONNECTION_ID.fetch_add(1, Ordering::Relaxed),
        outbound,
    };
    let heartbeat = node.heartbeat_interval();
    let hello = Payload::Hello(voice::Hello {
        heartbeat_interval_ms: u32::try_from(heartbeat.as_millis()).unwrap_or(u32::MAX),
    });
    let _ = connection
        .outbound
        .send(Outbound::Frame(encode(0, hello)))
        .await;

    if let Some(session) = handshake(&node, &mut stream, &connection).await {
        let code = serve_session(&node, &mut stream, &connection, &session, heartbeat).await;
        node.command(Command::Detached {
            session,
            connection_id: connection.id,
        });
        if let Some(code) = code {
            let _ = connection.outbound.send(Outbound::Close(code)).await;
        }
    }
    drop(connection);
    let _ = writer.await;
}

async fn write_frames(
    mut sink: futures_util::stream::SplitSink<WebSocket, Message>,
    mut frames: mpsc::Receiver<Outbound>,
) {
    while let Some(outbound) = frames.recv().await {
        match outbound {
            Outbound::Frame(frame) => {
                if sink.send(Message::Binary(frame)).await.is_err() {
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

/// Identify or Resume; returns the voice session id.
async fn handshake(
    node: &VoiceNode,
    stream: &mut futures_util::stream::SplitStream<WebSocket>,
    connection: &Connection,
) -> Option<String> {
    let deadline = Instant::now() + HANDSHAKE_TIMEOUT;
    loop {
        let next = tokio::time::timeout_at(deadline, next_envelope(stream)).await;
        let envelope = match next {
            Err(_) => return refuse(connection, close::HANDSHAKE_TIMEOUT).await,
            Ok(Next::Closed) => return None,
            Ok(Next::Invalid) => return refuse(connection, close::INVALID_FRAME).await,
            Ok(Next::Envelope(envelope)) => envelope,
        };
        match envelope.payload {
            Some(Payload::Heartbeat(beat)) => ack(connection, beat.nonce).await,
            Some(Payload::Identify(identify)) => {
                if identify.token.len() > MAX_TOKEN_BYTES {
                    return refuse(connection, close::INVALID_FRAME).await;
                }
                let (reply, answer) = oneshot::channel();
                node.command(Command::Identify {
                    identify,
                    connection: connection.clone(),
                    reply,
                });
                return match answer.await {
                    Ok(Ok(identified)) => Some(identified.session_id),
                    Ok(Err(error)) => {
                        tracing::debug!(%error, "a voice identify was refused");
                        refuse(connection, close::AUTHENTICATION_FAILED).await
                    }
                    Err(_) => refuse(connection, close::NODE_SHUTDOWN).await,
                };
            }
            Some(Payload::Resume(resume)) => {
                let (reply, answer) = oneshot::channel();
                node.command(Command::Resume {
                    resume,
                    connection: connection.clone(),
                    reply,
                });
                return match answer.await {
                    Ok(Some(session)) => Some(session),
                    Ok(None) => refuse(connection, close::SESSION_INVALID).await,
                    Err(_) => refuse(connection, close::NODE_SHUTDOWN).await,
                };
            }
            _ => return refuse(connection, close::INVALID_FRAME).await,
        }
    }
}

/// Serves an identified connection until it closes; returns the close code
/// to send, if the node is the one closing.
async fn serve_session(
    node: &VoiceNode,
    stream: &mut futures_util::stream::SplitStream<WebSocket>,
    connection: &Connection,
    session: &str,
    heartbeat: Duration,
) -> Option<u16> {
    let allowed_silence = heartbeat * 2;
    let mut deadline = Instant::now() + allowed_silence;
    let mut general = Allowance::new(RATE_BURST, RATE_PERIOD);
    let mut wants = Allowance::new(WANTS_BURST, WANTS_PERIOD);
    loop {
        let next = tokio::select! {
            () = connection.outbound.closed() => return None,
            () = tokio::time::sleep_until(deadline) => return Some(close::HEARTBEAT_TIMEOUT),
            next = next_envelope(stream) => next,
        };
        let envelope = match next {
            Next::Closed => return None,
            Next::Invalid => return Some(close::INVALID_FRAME),
            Next::Envelope(envelope) => envelope,
        };
        let now = Instant::now();
        let allowance = match envelope.payload {
            Some(Payload::MediaSinkWants(_)) => &mut wants,
            _ => &mut general,
        };
        if !allowance.take(now) {
            return Some(close::RATE_LIMITED);
        }
        match envelope.payload {
            Some(Payload::Heartbeat(beat)) => {
                deadline = Instant::now() + allowed_silence;
                ack(connection, beat.nonce).await;
            }
            Some(Payload::TransportInfo(info)) => {
                let too_long = info.ice_ufrag.len() > MAX_ICE_FIELD
                    || info.ice_pwd.len() > MAX_ICE_FIELD
                    || info.dtls_fingerprint.len() != 32;
                if too_long {
                    return Some(close::INVALID_FRAME);
                }
                node.command(Command::Transport {
                    session: session.to_owned(),
                    info,
                });
            }
            Some(Payload::Speaking(speaking)) => node.command(Command::Speaking {
                session: session.to_owned(),
                flags: speaking.flags,
            }),
            Some(Payload::PublishTrack(publish)) => {
                // The node checks the rest and answers TrackRejected.
                if publish.track_id.len() > MAX_TRACK_ID || publish.layers.len() > MAX_LAYERS {
                    return Some(close::INVALID_FRAME);
                }
                node.command(Command::Publish {
                    session: session.to_owned(),
                    publish,
                });
            }
            Some(Payload::UnpublishTrack(unpublish)) => {
                if unpublish.track_id.len() > MAX_TRACK_ID {
                    return Some(close::INVALID_FRAME);
                }
                node.command(Command::Unpublish {
                    session: session.to_owned(),
                    track_id: unpublish.track_id,
                });
            }
            Some(Payload::MediaSinkWants(sink_wants)) => {
                let oversized = sink_wants.wants.len() > MAX_WANTS
                    || sink_wants
                        .wants
                        .iter()
                        .any(|want| want.track_id.len() > MAX_TRACK_ID);
                if oversized {
                    return Some(close::INVALID_FRAME);
                }
                node.command(Command::SinkWants {
                    session: session.to_owned(),
                    wants: sink_wants,
                });
            }
            _ => return Some(close::INVALID_FRAME),
        }
    }
}

/// Layers a track may name; more is not a track.
const MAX_LAYERS: usize = 3;

/// At most `burst` messages per `period`, counted in fixed windows.
struct Allowance {
    burst: u32,
    period: Duration,
    window_start: Instant,
    used: u32,
}

impl Allowance {
    fn new(burst: u32, period: Duration) -> Self {
        Self {
            burst,
            period,
            window_start: Instant::now(),
            used: 0,
        }
    }

    fn take(&mut self, now: Instant) -> bool {
        if now.duration_since(self.window_start) >= self.period {
            self.window_start = now;
            self.used = 0;
        }
        self.used += 1;
        self.used <= self.burst
    }
}

async fn ack(connection: &Connection, nonce: u64) {
    let frame = encode(0, Payload::HeartbeatAck(voice::HeartbeatAck { nonce }));
    let _ = connection.outbound.send(Outbound::Frame(frame)).await;
}

async fn refuse(connection: &Connection, code: u16) -> Option<String> {
    let _ = connection.outbound.send(Outbound::Close(code)).await;
    None
}

enum Next {
    Envelope(Box<voice::Envelope>),
    Invalid,
    Closed,
}

async fn next_envelope(stream: &mut futures_util::stream::SplitStream<WebSocket>) -> Next {
    loop {
        match stream.next().await {
            Some(Ok(Message::Binary(bytes))) => {
                return match voice::Envelope::decode(bytes) {
                    Ok(envelope) => Next::Envelope(Box::new(envelope)),
                    Err(_) => Next::Invalid,
                };
            }
            Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
            Some(Ok(Message::Text(_))) => return Next::Invalid,
            Some(Ok(Message::Close(_)) | Err(_)) | None => return Next::Closed,
        }
    }
}
