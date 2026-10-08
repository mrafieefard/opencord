//! The main server's end of the control channel to external voice nodes
//! (Phase 2 plan §3.4). A node answers a challenge with its shared secret
//! and registers; then it reports its load and the voice connections that
//! ended, while this server sends it commands. When the channel closes or
//! goes quiet, the node's channels move to another node, or wait for one.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{CloseFrame, Message, WebSocket};
use axum::extract::{ConnectInfo, State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use internal::control_envelope::Payload;
use opencord_common::voice::close;
use opencord_proto::internal::v1 as internal;
use opencord_proto::v1 as proto;
use opencord_voice::control::{self, NONCE_LEN, SILENT_FOR};
use opencord_voice::node::NodeCommand;
use prost::Message as _;
use tokio::sync::mpsc;
use tokio::time::Instant;

use super::nodes::{CommandSink, NodeLink, Registration};
use crate::config::ExternalNode;
use crate::random;
use crate::state::AppState;

/// Time a node has to answer the challenge.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// Control messages are small.
const FRAME_BYTES: usize = 64 * 1024;
/// Shared secrets shorter than this are refused.
pub const MIN_SECRET_BYTES: usize = 16;

type Sink = SplitSink<WebSocket, Message>;
type Stream = SplitStream<WebSocket>;

pub async fn upgrade(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    upgrade: WebSocketUpgrade,
) -> Response {
    if state.rate_limits.check_voice_node(peer.ip()).is_err() {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    upgrade
        .max_message_size(FRAME_BYTES)
        .max_frame_size(FRAME_BYTES)
        .on_upgrade(move |socket| serve(state, socket))
}

/// Hands commands to a node's control channel.
#[derive(Debug)]
struct ControlSink(mpsc::UnboundedSender<internal::ControlEnvelope>);

impl CommandSink for ControlSink {
    fn send(&self, command: NodeCommand) {
        let _ = self.0.send(control::envelope_for(&command));
    }
}

async fn serve(state: Arc<AppState>, socket: WebSocket) {
    let (mut sink, mut stream) = socket.split();
    let Some((registration, mut commands)) = register(&state, &mut sink, &mut stream).await else {
        return;
    };
    tracing::info!(node = registration.id(), "a voice node registered");
    super::place_unassigned(&state).await;

    let code = run(&state, &registration, &mut sink, &mut stream, &mut commands).await;
    if let Some(code) = code {
        close_with(&mut sink, code).await;
    }
    let lost = state.voice_nodes.unregister(&registration);
    tracing::warn!(
        node = registration.id(),
        channels = lost.len(),
        "a voice node went away"
    );
    if !lost.is_empty() {
        super::place_unassigned(&state).await;
    }
}

/// Challenge, proof, registration. `None` once the node was turned away.
async fn register(
    state: &AppState,
    sink: &mut Sink,
    stream: &mut Stream,
) -> Option<(
    Registration,
    mpsc::UnboundedReceiver<internal::ControlEnvelope>,
)> {
    let nonce = random::bytes::<NONCE_LEN>();
    let challenge = Payload::Challenge(internal::Challenge {
        nonce: nonce.to_vec(),
    });
    send(sink, challenge).await.ok()?;
    let register = match tokio::time::timeout(HANDSHAKE_TIMEOUT, next(stream)).await {
        Err(_) => return refuse(sink, close::HANDSHAKE_TIMEOUT).await,
        Ok(Next::Closed) => return None,
        Ok(Next::Envelope(envelope)) => match envelope.payload {
            Some(Payload::Register(register)) => register,
            _ => return refuse(sink, close::INVALID_FRAME).await,
        },
        Ok(Next::Alive | Next::Invalid) => return refuse(sink, close::INVALID_FRAME).await,
    };
    let Ok(fingerprint) = <[u8; 32]>::try_from(register.certificate_fingerprint.as_slice()) else {
        return refuse(sink, close::INVALID_FRAME).await;
    };
    let Some(node) = authenticate(state, &nonce, &register).await else {
        // Debug formatting escapes whatever an unauthenticated peer sent.
        tracing::warn!(
            endpoint = ?register.endpoint.chars().take(256).collect::<String>(),
            "a voice node could not register: unknown endpoint or wrong secret"
        );
        let error = proto::Error {
            code: proto::ErrorCode::Unauthorized as i32,
            message: "unknown voice node, or the wrong shared secret".to_owned(),
            retry_after_ms: None,
        };
        let _ = send(sink, Payload::Error(error)).await;
        return refuse(sink, close::AUTHENTICATION_FAILED).await;
    };

    // Registered goes out before anyone can be sent to the node, so the node
    // knows the voice key before the first client arrives.
    let registered = internal::Registered {
        node_id: node.endpoint.clone(),
        voice_signing_public_key: state.voice_key.verifying_key().to_bytes().to_vec(),
        settings: Some(state.guild().voice_settings.to_proto()),
    };
    send(sink, Payload::Registered(registered)).await.ok()?;
    let (commands_tx, commands) = mpsc::unbounded_channel();
    let link = NodeLink {
        endpoint: node.endpoint.clone(),
        fingerprint,
    };
    let registration =
        state
            .voice_nodes
            .register(&node.endpoint, link, Box::new(ControlSink(commands_tx)));
    Some((registration, commands))
}

/// The configured node `register` names, if it proved the shared secret.
async fn authenticate<'a>(
    state: &'a AppState,
    nonce: &[u8],
    register: &internal::Register,
) -> Option<&'a ExternalNode> {
    let node = state
        .config
        .voice
        .external_nodes
        .iter()
        .find(|node| node.endpoint == register.endpoint)?;
    let secret = match read_secret(&node.secret_file).await {
        Ok(secret) => secret,
        Err(problem) => {
            tracing::error!(
                endpoint = %node.endpoint,
                file = %node.secret_file.display(),
                "the voice node's shared secret is unusable: {problem}"
            );
            return None;
        }
    };
    control::proof_matches(&secret, nonce, &register.proof).then_some(node)
}

/// The secret in `path`, without surrounding whitespace.
pub async fn read_secret(path: &Path) -> Result<Vec<u8>, String> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|error| error.to_string())?;
    let secret = bytes.trim_ascii();
    if secret.len() < MIN_SECRET_BYTES {
        return Err(format!("it is shorter than {MIN_SECRET_BYTES} characters"));
    }
    Ok(secret.to_vec())
}

/// Serves a registered node until its channel ends; returns the close code
/// to send, if this server is the one closing.
async fn run(
    state: &AppState,
    registration: &Registration,
    sink: &mut Sink,
    stream: &mut Stream,
    commands: &mut mpsc::UnboundedReceiver<internal::ControlEnvelope>,
) -> Option<u16> {
    let mut deadline = Instant::now() + SILENT_FOR;
    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(envelope) => {
                    // A node that stops reading is as good as gone.
                    let sent = tokio::time::timeout(SILENT_FOR, send_envelope(sink, &envelope)).await;
                    if !matches!(sent, Ok(Ok(()))) {
                        return None;
                    }
                }
                // The node registered again on another connection.
                None => return Some(close::SESSION_REPLACED),
            },
            () = tokio::time::sleep_until(deadline) => return Some(close::HEARTBEAT_TIMEOUT),
            next = next(stream) => {
                deadline = Instant::now() + SILENT_FOR;
                match next {
                    Next::Closed => return None,
                    Next::Invalid => return Some(close::INVALID_FRAME),
                    Next::Alive => {}
                    Next::Envelope(envelope) => {
                        if !handle(state, registration, *envelope).await {
                            return Some(close::INVALID_FRAME);
                        }
                    }
                }
            }
        }
    }
}

/// One message from a registered node; `false` if it is not one a node
/// sends.
async fn handle(
    state: &AppState,
    registration: &Registration,
    envelope: internal::ControlEnvelope,
) -> bool {
    match &envelope.payload {
        Some(Payload::LoadReport(load)) => {
            tracing::debug!(
                node = registration.id(),
                channels = load.channels,
                participants = load.participants,
                packets_per_second = load.packets_per_second,
                bitrate_in = load.bitrate_in,
                bitrate_out = load.bitrate_out,
                "voice node load"
            );
            state
                .voice_nodes
                .report_load(registration, load.participants);
        }
        Some(Payload::ParticipantDisconnected(gone)) => {
            // A node speaks only for the channels it serves.
            if state.voice_nodes.serves(registration, gone.channel_id)
                && let Some(event) = control::event_from(&envelope)
            {
                super::on_node_event(state, event).await;
            }
        }
        // AFK moves use speaking activity (V9).
        Some(Payload::ParticipantConnected(_) | Payload::SpeakingActivity(_)) => {}
        Some(Payload::Error(error)) => {
            tracing::warn!(node = registration.id(), message = ?error.message, "a voice node reported an error");
        }
        _ => return false,
    }
    true
}

async fn refuse<T>(sink: &mut Sink, code: u16) -> Option<T> {
    close_with(sink, code).await;
    None
}

async fn close_with(sink: &mut Sink, code: u16) {
    let frame = CloseFrame {
        code,
        reason: "".into(),
    };
    let _ = sink.send(Message::Close(Some(frame))).await;
    let _ = sink.close().await;
}

async fn send(sink: &mut Sink, payload: Payload) -> Result<(), axum::Error> {
    let envelope = internal::ControlEnvelope {
        payload: Some(payload),
    };
    send_envelope(sink, &envelope).await
}

async fn send_envelope(
    sink: &mut Sink,
    envelope: &internal::ControlEnvelope,
) -> Result<(), axum::Error> {
    sink.send(Message::Binary(envelope.encode_to_vec().into()))
        .await
}

enum Next {
    Envelope(Box<internal::ControlEnvelope>),
    /// A ping or pong: the node is there.
    Alive,
    Invalid,
    Closed,
}

async fn next(stream: &mut Stream) -> Next {
    match stream.next().await {
        Some(Ok(Message::Binary(bytes))) => match internal::ControlEnvelope::decode(bytes) {
            Ok(envelope) => Next::Envelope(Box::new(envelope)),
            Err(_) => Next::Invalid,
        },
        Some(Ok(Message::Ping(_) | Message::Pong(_))) => Next::Alive,
        Some(Ok(Message::Text(_))) => Next::Invalid,
        Some(Ok(Message::Close(_)) | Err(_)) | None => Next::Closed,
    }
}
