//! One task per server. It connects, authenticates, keeps the connection
//! alive, reconnects with backoff (resuming when it can) and turns gateway
//! events into [`CoreEvent`]s. One server failing never affects another.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::{SinkExt, StreamExt};
use opencord_common::address::{Fingerprint, ServerAddress, format_fingerprint};
use opencord_common::auth::{Challenge, Nonce, ServerId};
use opencord_proto::v1 as proto;
use prost::Message as _;
use proto::envelope::Payload;
use rustls::pki_types::ServerName;
use tokio::net::TcpStream;
use tokio::runtime::Handle;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::{Instant, MissedTickBehavior, interval, interval_at, sleep_until, timeout};
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message as Frame;

use crate::api::types::{
    ConnectionState, CoreError, CoreEvent, CoreEventPayload, ErrorCode, FailureReason,
};
use crate::convert;
use crate::identity::Identity;
use crate::mirror::{GuildMirror, PermissionSnapshot};
use crate::store::Store;
use crate::tofu::{TlsTrust, Verdict};
use crate::voice::VoiceServer;

pub(crate) type Socket = WebSocketStream<TlsStream<TcpStream>>;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_BACKOFF: Duration = Duration::from_secs(30);
const COMMAND_CAPACITY: usize = 256;

#[derive(Debug, Clone)]
pub(crate) struct Credentials {
    pub identity: Identity,
    pub display_name: String,
}

#[derive(Debug)]
pub(crate) enum HandshakeError {
    Untrusted(Fingerprint),
    Mismatch {
        expected: Fingerprint,
        presented: Fingerprint,
    },
    Rejected {
        reason: FailureReason,
        message: String,
    },
    /// Worth retrying: network trouble, timeouts.
    Transient(String),
    /// Worth retrying once the server's wait is over.
    RateLimited {
        message: String,
        retry_after: Duration,
    },
}

impl From<HandshakeError> for CoreError {
    fn from(error: HandshakeError) -> Self {
        match error {
            HandshakeError::Untrusted(presented) => Self::Connection {
                message: format!(
                    "the certificate {} is not trusted",
                    format_fingerprint(&presented)
                ),
            },
            HandshakeError::Mismatch {
                expected,
                presented,
            } => Self::FingerprintMismatch {
                expected: format_fingerprint(&expected),
                presented: format_fingerprint(&presented),
            },
            HandshakeError::Rejected { reason, message } => Self::Rejected { reason, message },
            HandshakeError::Transient(message) => Self::Connection { message },
            HandshakeError::RateLimited {
                message,
                retry_after,
            } => Self::Server {
                code: ErrorCode::RateLimited,
                message,
                retry_after_ms: Some(u32::try_from(retry_after.as_millis()).unwrap_or(u32::MAX)),
            },
        }
    }
}

/// A TLS WebSocket that has received `Hello`.
pub(crate) struct Opened {
    pub socket: Socket,
    pub hello: proto::Hello,
    pub presented: Fingerprint,
    /// The name connected to, and whether an authority vouched for it (no
    /// pin was needed).
    pub host: String,
    pub authority: bool,
}

/// Connects and reads `Hello`. With `pinned`, the certificate must match it.
pub(crate) async fn open(
    address: &ServerAddress,
    pinned: Option<Fingerprint>,
) -> Result<Opened, HandshakeError> {
    let trust = TlsTrust::new(pinned);
    let tcp = timeout(
        CONNECT_TIMEOUT,
        TcpStream::connect((address.host.as_str(), address.port)),
    )
    .await
    .map_err(|_| HandshakeError::Transient("the connection timed out".to_owned()))?
    .map_err(|error| HandshakeError::Transient(error.to_string()))?;
    let server_name = ServerName::try_from(address.host.clone())
        .map_err(|_| HandshakeError::Transient(format!("{} is not a valid host", address.host)))?;
    let tls = timeout(
        CONNECT_TIMEOUT,
        TlsConnector::from(Arc::clone(&trust.config)).connect(server_name, tcp),
    )
    .await
    .map_err(|_| HandshakeError::Transient("the TLS handshake timed out".to_owned()));
    let tls = match (tls, trust.verdict()) {
        (Ok(Ok(tls)), _) => tls,
        (_, Some(Verdict::Untrusted(presented))) => {
            return Err(HandshakeError::Untrusted(presented));
        }
        (
            _,
            Some(Verdict::Mismatch {
                expected,
                presented,
            }),
        ) => {
            return Err(HandshakeError::Mismatch {
                expected,
                presented,
            });
        }
        (Ok(Err(error)), _) => return Err(HandshakeError::Transient(error.to_string())),
        (Err(error), _) => return Err(error),
    };
    let authority = matches!(trust.verdict(), Some(Verdict::Authority(_)))
        && address.host.parse::<std::net::IpAddr>().is_err();
    let presented = match trust.verdict() {
        Some(Verdict::Pinned(fingerprint) | Verdict::Authority(fingerprint)) => fingerprint,
        _ => {
            return Err(HandshakeError::Transient(
                "the certificate was not checked".to_owned(),
            ));
        }
    };
    let url = format!("wss://{address}/gateway");
    let (mut socket, _) = timeout(HANDSHAKE_TIMEOUT, tokio_tungstenite::client_async(url, tls))
        .await
        .map_err(|_| HandshakeError::Transient("the WebSocket handshake timed out".to_owned()))?
        .map_err(|error| HandshakeError::Transient(error.to_string()))?;
    match next_payload(&mut socket).await? {
        (_, _, Payload::Hello(hello)) => Ok(Opened {
            socket,
            hello,
            presented,
            host: address.host.clone(),
            authority,
        }),
        _ => Err(HandshakeError::Transient(
            "the server did not say hello".to_owned(),
        )),
    }
}

/// Signs the `Hello` challenge, bound to the certificate this connection
/// verified, and waits for `Ready`.
pub(crate) async fn identify(
    opened: &mut Opened,
    credentials: &Credentials,
    invite_code: Option<String>,
    claim_token: Option<String>,
) -> Result<proto::Ready, HandshakeError> {
    let hello = &opened.hello;
    if hello.protocol_version != opencord_common::PROTOCOL_VERSION {
        return Err(HandshakeError::Rejected {
            reason: FailureReason::Incompatible,
            message: format!(
                "the server speaks protocol version {}; this app speaks {}",
                hello.protocol_version,
                opencord_common::PROTOCOL_VERSION
            ),
        });
    }
    let server_id: ServerId =
        hello.server_id.as_slice().try_into().map_err(|_| {
            HandshakeError::Transient("the server sent a malformed hello".to_owned())
        })?;
    let nonce: Nonce =
        hello.nonce.as_slice().try_into().map_err(|_| {
            HandshakeError::Transient("the server sent a malformed hello".to_owned())
        })?;
    let challenge = Challenge {
        server_id,
        certificate: opened.presented,
        nonce,
        timestamp_ms: unix_ms(),
    };
    // Checked by an authority under a name: bound to the name too, for a
    // server behind a tunnel or proxy whose certificate it never sees.
    let (host, host_signature) = if opened.authority {
        let bound = Challenge {
            certificate: opencord_common::auth::host_binding(&opened.host),
            ..challenge
        };
        (
            opened.host.clone(),
            credentials.identity.sign_identify(&bound).to_vec(),
        )
    } else {
        (String::new(), Vec::new())
    };
    let identify = proto::Identify {
        public_key: credentials.identity.public_key().to_vec(),
        signature: credentials.identity.sign_identify(&challenge).to_vec(),
        timestamp_ms: challenge.timestamp_ms,
        display_name: credentials.display_name.clone(),
        invite_code,
        claim_token,
        protocol_version: opencord_common::PROTOCOL_VERSION,
        host,
        host_signature,
    };
    let socket = &mut opened.socket;
    send_payload(socket, 0, Payload::Identify(identify)).await?;
    loop {
        match next_payload(socket).await? {
            (_, _, Payload::Ready(ready)) => return Ok(*ready),
            (_, _, Payload::Error(error)) => return Err(handshake_error(error)),
            _ => {}
        }
    }
}

pub(crate) struct Session {
    pub session_id: String,
    pub resume_token: Vec<u8>,
    pub last_seq: u64,
}

impl Session {
    pub fn from_ready(ready: &proto::Ready) -> Self {
        Self {
            session_id: ready.session_id.clone(),
            resume_token: ready.resume_token.clone(),
            last_seq: 0,
        }
    }
}

enum ResumeOutcome {
    Resumed(Vec<(u64, proto::Event)>),
    /// The server forgot the session; identify on the same connection.
    Invalid,
}

async fn resume(socket: &mut Socket, session: &Session) -> Result<ResumeOutcome, HandshakeError> {
    let resume = proto::Resume {
        session_id: session.session_id.clone(),
        resume_token: session.resume_token.clone(),
        last_seq: session.last_seq,
    };
    send_payload(socket, 0, Payload::Resume(resume)).await?;
    let mut replayed = Vec::new();
    loop {
        match next_payload(socket).await? {
            (seq, _, Payload::Event(event)) => replayed.push((seq, event)),
            (_, _, Payload::Resumed(_)) => return Ok(ResumeOutcome::Resumed(replayed)),
            (_, _, Payload::Error(error))
                if error.code == proto::ErrorCode::InvalidSession as i32 =>
            {
                return Ok(ResumeOutcome::Invalid);
            }
            (_, _, Payload::Error(error)) => return Err(handshake_error(error)),
            _ => {}
        }
    }
}

/// An authenticated connection, ready to serve.
pub(crate) struct Established {
    pub socket: Socket,
    pub heartbeat_interval: Duration,
    pub session: Session,
    /// Set after a fresh `Identify`; `None` after a resume.
    pub ready: Option<proto::Ready>,
    pub replayed: Vec<(u64, proto::Event)>,
}

impl Established {
    pub fn identified(opened: Opened, ready: proto::Ready) -> Self {
        Self {
            socket: opened.socket,
            heartbeat_interval: heartbeat_interval(&opened.hello),
            session: Session::from_ready(&ready),
            ready: Some(ready),
            replayed: Vec::new(),
        }
    }
}

pub(crate) enum Command {
    Request {
        kind: proto::request::Kind,
        reply: oneshot::Sender<Result<proto::response::Result, CoreError>>,
    },
    /// Skip the wait before the next attempt, or start over after a
    /// failure.
    RetryNow,
    /// The computer woke up: check the connection now with a heartbeat,
    /// and reconnect if it goes unanswered for [`PROBE_TIMEOUT`]; waiting,
    /// try again at once.
    Probe,
    Shutdown,
}

/// How long a probe's heartbeat may go unanswered.
pub(crate) const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Handle to a running server task.
pub(crate) struct Connection {
    pub commands: mpsc::Sender<Command>,
    pub task: JoinHandle<()>,
}

pub(crate) struct Context {
    pub key: String,
    pub address: ServerAddress,
    pub credentials: Credentials,
    pub store: Arc<Mutex<Store>>,
    pub events: mpsc::UnboundedSender<CoreEvent>,
    pub voice_servers: broadcast::Sender<VoiceServer>,
    /// Each server's latest media token, for its HTTP endpoints.
    pub media_tokens: MediaTokens,
    pub routes: Routes,
}

/// Media tokens by server key. They stay in Rust: the soundboard needs
/// them, the app does not.
pub(crate) type MediaTokens = Arc<Mutex<HashMap<String, String>>>;

/// How a server was last reached: its address, and the certificate pinned
/// there, or `None` when an authority vouched for it. Requests besides the
/// gateway (files, voice) go the same way (Phase 3 plan §7.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Route {
    pub address: ServerAddress,
    pub pin: Option<Fingerprint>,
}

impl Route {
    pub fn of(opened: &Opened, address: &ServerAddress) -> Self {
        Self {
            address: address.clone(),
            pin: (!opened.authority).then_some(opened.presented),
        }
    }
}

/// Routes by server key.
pub(crate) type Routes = Arc<Mutex<HashMap<String, Route>>>;

pub(crate) fn spawn(
    runtime: &Handle,
    context: Context,
    initial: Option<Established>,
) -> Connection {
    let (commands, receiver) = mpsc::channel(COMMAND_CAPACITY);
    let task = Task {
        context,
        commands: receiver,
        session: None,
        self_id: None,
        mirror: None,
        permissions: None,
        next_request_id: 1,
    };
    Connection {
        commands,
        task: runtime.spawn(task.run(initial)),
    }
}

/// Delay before reconnect attempt `attempt` (1-based): doubling from 1 s up
/// to 30 s, then scaled by `jitter` (0 to 1) into 75 % to 125 %.
pub(crate) fn backoff_delay(attempt: u32, jitter: f64) -> Duration {
    let doublings = attempt.saturating_sub(1).min(5);
    let base = Duration::from_secs(1u64 << doublings).min(MAX_BACKOFF);
    base.mul_f64(0.75 + jitter.clamp(0.0, 1.0) * 0.5)
        .min(MAX_BACKOFF)
}

enum Ended {
    Shutdown,
    /// The connection dropped; the session may resume.
    Lost,
    /// The connection dropped and the session cannot resume.
    LostSession,
    Fatal {
        reason: FailureReason,
        message: String,
        /// Fingerprints, as lowercase hex, when the certificate changed.
        expected: Option<String>,
        presented: Option<String>,
    },
}

impl Ended {
    fn fatal(reason: FailureReason, message: &str) -> Self {
        Self::Fatal {
            reason,
            message: message.to_owned(),
            expected: None,
            presented: None,
        }
    }

    fn changed_certificate(expected: Option<Fingerprint>, presented: Fingerprint) -> Self {
        Self::Fatal {
            reason: FailureReason::FingerprintChanged,
            message: "the server's certificate no longer matches the trusted one".to_owned(),
            expected: expected.as_ref().map(format_fingerprint),
            presented: Some(format_fingerprint(&presented)),
        }
    }
}

struct Pending {
    reply: oneshot::Sender<Result<proto::response::Result, CoreError>>,
    deadline: Instant,
}

struct Task {
    context: Context,
    commands: mpsc::Receiver<Command>,
    session: Option<Session>,
    /// This user's id on the server, from the last Ready.
    self_id: Option<i64>,
    mirror: Option<GuildMirror>,
    permissions: Option<PermissionSnapshot>,
    next_request_id: u64,
}

impl Task {
    async fn run(mut self, mut initial: Option<Established>) {
        let mut attempt = 0;
        loop {
            // The server's own wait, when it refused a sign-in for coming
            // too often: retrying sooner is refused again.
            let mut not_before = Duration::ZERO;
            let established = match initial.take() {
                Some(established) => Ok(established),
                None => {
                    if attempt == 0 {
                        self.emit_state(ConnectionState::Connecting);
                    }
                    self.establish().await
                }
            };
            let ended = match established {
                Ok(established) => {
                    attempt = 0;
                    self.emit_state(ConnectionState::Connected);
                    self.serve(established).await
                }
                Err(HandshakeError::Transient(_)) => Ended::Lost,
                Err(HandshakeError::RateLimited { retry_after, .. }) => {
                    not_before = retry_after;
                    Ended::Lost
                }
                Err(HandshakeError::Rejected { reason, message }) => Ended::fatal(reason, &message),
                Err(HandshakeError::Untrusted(presented)) => {
                    Ended::changed_certificate(None, presented)
                }
                Err(HandshakeError::Mismatch {
                    expected,
                    presented,
                }) => Ended::changed_certificate(Some(expected), presented),
            };
            match ended {
                Ended::Shutdown => return,
                Ended::Fatal {
                    reason,
                    message,
                    expected,
                    presented,
                } => {
                    self.session = None;
                    self.emit_state(ConnectionState::Failed {
                        reason,
                        message,
                        expected_fingerprint: expected,
                        presented_fingerprint: presented,
                    });
                    if !self.park().await {
                        return;
                    }
                    // Retry now: start over.
                    attempt = 0;
                    continue;
                }
                Ended::LostSession => self.session = None,
                Ended::Lost => {}
            }
            attempt += 1;
            if !self.back_off(attempt, not_before).await {
                return;
            }
        }
    }

    async fn establish(&mut self) -> Result<Established, HandshakeError> {
        let mut opened = self.open_any().await?;
        if let Some(session) = &self.session {
            match resume(&mut opened.socket, session).await? {
                ResumeOutcome::Resumed(replayed) => {
                    return Ok(Established {
                        heartbeat_interval: heartbeat_interval(&opened.hello),
                        socket: opened.socket,
                        session: Session {
                            session_id: session.session_id.clone(),
                            resume_token: session.resume_token.clone(),
                            last_seq: session.last_seq,
                        },
                        ready: None,
                        replayed,
                    });
                }
                ResumeOutcome::Invalid => self.session = None,
            }
        }
        let ready = identify(&mut opened, &self.context.credentials, None, None).await?;
        Ok(Established::identified(opened, ready))
    }

    async fn serve(&mut self, established: Established) -> Ended {
        let Established {
            mut socket,
            heartbeat_interval,
            session,
            ready,
            replayed,
        } = established;
        self.session = Some(session);
        if let Some(ready) = ready {
            self.on_ready(ready);
        }
        for (seq, event) in replayed {
            self.on_event(seq, event);
        }

        let mut pending: HashMap<u64, Pending> = HashMap::new();
        let mut heartbeat = interval_at(Instant::now() + heartbeat_interval, heartbeat_interval);
        heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut sweep = interval(Duration::from_secs(1));
        let mut awaiting_ack = false;
        // A probe's deadline for its heartbeat's ack.
        let mut probe: Option<Instant> = None;
        let ended = loop {
            tokio::select! {
                frame = socket.next() => match frame {
                    Some(Ok(Frame::Binary(bytes))) => {
                        let Ok(envelope) = proto::Envelope::decode(bytes) else {
                            break Ended::Lost;
                        };
                        match envelope.payload {
                            Some(Payload::Event(event)) => self.on_event(envelope.seq, event),
                            Some(Payload::Response(response)) => {
                                if let Some(waiting) = pending.remove(&envelope.request_id) {
                                    let result = response.result.ok_or_else(|| CoreError::Connection {
                                        message: "the server sent an empty response".to_owned(),
                                    });
                                    let _ = waiting.reply.send(result);
                                }
                            }
                            Some(Payload::HeartbeatAck(_)) => {
                                awaiting_ack = false;
                                probe = None;
                            }
                            _ => {}
                        }
                    }
                    Some(Ok(Frame::Close(frame))) => {
                        break closed(frame.map(|frame| u16::from(frame.code)));
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) | None => break Ended::Lost,
                },
                _ = heartbeat.tick() => {
                    if awaiting_ack {
                        break Ended::Lost;
                    }
                    let last_seq = self.session.as_ref().map_or(0, |session| session.last_seq);
                    let beat = Payload::Heartbeat(proto::Heartbeat { last_seq });
                    if send_payload(&mut socket, 0, beat).await.is_err() {
                        break Ended::Lost;
                    }
                    awaiting_ack = true;
                }
                () = sleep_until(probe.unwrap_or_else(Instant::now)), if probe.is_some() => {
                    break Ended::Lost;
                }
                _ = sweep.tick() => {
                    let now = Instant::now();
                    let expired: Vec<u64> = pending
                        .iter()
                        .filter(|(_, waiting)| waiting.deadline <= now)
                        .map(|(id, _)| *id)
                        .collect();
                    for id in expired {
                        if let Some(waiting) = pending.remove(&id) {
                            let _ = waiting.reply.send(Err(CoreError::Timeout));
                        }
                    }
                }
                command = self.commands.recv() => match command {
                    Some(Command::Request { kind, reply }) => {
                        let request_id = self.next_request_id;
                        self.next_request_id += 1;
                        let request = Payload::Request(proto::Request { kind: Some(kind) });
                        if send_payload(&mut socket, request_id, request).await.is_err() {
                            let _ = reply.send(Err(CoreError::NotConnected));
                            break Ended::Lost;
                        }
                        pending.insert(request_id, Pending {
                            reply,
                            deadline: Instant::now() + REQUEST_TIMEOUT,
                        });
                    }
                    Some(Command::RetryNow) => {}
                    Some(Command::Probe) => {
                        let last_seq = self.session.as_ref().map_or(0, |session| session.last_seq);
                        let beat = Payload::Heartbeat(proto::Heartbeat { last_seq });
                        if send_payload(&mut socket, 0, beat).await.is_err() {
                            break Ended::Lost;
                        }
                        awaiting_ack = true;
                        probe = Some(Instant::now() + PROBE_TIMEOUT);
                    }
                    Some(Command::Shutdown) | None => {
                        let _ = socket.close(None).await;
                        break Ended::Shutdown;
                    }
                },
            }
        };
        for (_, waiting) in pending {
            let _ = waiting.reply.send(Err(CoreError::NotConnected));
        }
        ended
    }

    /// Connects to the first of the server's addresses that answers as
    /// this server: its domains, the primary first, then the address it
    /// was added with, then its IP address (Phase 3 plan §7.5). Only the
    /// address it was added with can fail with a certificate the user must
    /// look at; the others are skipped.
    async fn open_any(&mut self) -> Result<Opened, HandshakeError> {
        let (pinned, saved) = {
            let store = self.lock_store();
            (
                store.pin(&self.context.key),
                store.server(&self.context.key).cloned(),
            )
        };
        let own = &self.context.address;
        let mut candidates: Vec<(ServerAddress, bool)> = Vec::new();
        if let Some(saved) = &saved {
            for address in saved.addresses.iter().chain(&saved.ip) {
                let candidate = ServerAddress {
                    host: address.host.to_ascii_lowercase(),
                    port: address.port,
                };
                if candidate != *own && !candidates.iter().any(|(known, _)| *known == candidate) {
                    candidates.push((candidate, address.trusted));
                }
            }
        }
        let ip = saved
            .as_ref()
            .and_then(|saved| saved.ip.as_ref())
            .map(|ip| ServerAddress {
                host: ip.host.clone(),
                port: ip.port,
            });
        // The domains, then the address it was added with, then the IP.
        let (mut ordered, ips): (Vec<_>, Vec<_>) = candidates
            .into_iter()
            .partition(|(address, _)| Some(address) != ip.as_ref());
        ordered.push((own.clone(), false));
        ordered.extend(ips);
        let server_id = saved.as_ref().and_then(|saved| saved.server_id.clone());
        let mut last = None;
        for (address, trusted) in ordered {
            let own_address = address == *own;
            let pin = if trusted { None } else { pinned };
            match open(&address, pin).await {
                Ok(opened) => {
                    // Another server at a domain that moved on: not ours.
                    let id = hex::encode(&opened.hello.server_id);
                    if server_id.as_ref().is_some_and(|known| *known != id) {
                        last = Some(HandshakeError::Transient(format!(
                            "{address} is another server now"
                        )));
                        continue;
                    }
                    if !own_address {
                        tracing::debug!(%address, "connected through another of the server's addresses");
                    }
                    self.remember_server_id(&id);
                    self.context
                        .routes
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .insert(self.context.key.clone(), Route::of(&opened, &address));
                    return Ok(opened);
                }
                Err(error @ (HandshakeError::Untrusted(_) | HandshakeError::Mismatch { .. }))
                    if own_address =>
                {
                    return Err(error);
                }
                Err(error) => last = Some(error),
            }
        }
        Err(last.unwrap_or_else(|| {
            HandshakeError::Transient("the server has no address to connect to".to_owned())
        }))
    }

    fn remember_server_id(&self, id: &str) {
        let mut store = self.lock_store();
        let Some(saved) = store.server(&self.context.key).cloned() else {
            return;
        };
        if saved.server_id.as_deref() != Some(id) {
            let _ = store.upsert_server(crate::store::SavedServer {
                server_id: Some(id.to_owned()),
                ..saved
            });
        }
    }

    /// Keeps where the server is reached, for the next connection.
    fn save_addresses(&self, addresses: &proto::ServerAddresses) {
        let saved_address = |address: &proto::ConnectAddress| crate::store::SavedAddress {
            host: address.host.to_ascii_lowercase(),
            port: u16::try_from(address.port).unwrap_or(0),
            trusted: address.trusted_certificate,
        };
        let mut store = self.lock_store();
        let Some(saved) = store.server(&self.context.key).cloned() else {
            return;
        };
        let updated = crate::store::SavedServer {
            addresses: addresses
                .domains
                .iter()
                .filter(|address| address.port != 0)
                .map(saved_address)
                .collect(),
            ip: addresses.ip.as_ref().map(saved_address),
            ..saved.clone()
        };
        if updated != saved {
            let _ = store.upsert_server(updated);
        }
    }

    fn on_ready(&mut self, ready: proto::Ready) {
        if let Some(addresses) = &ready.server_addresses {
            self.save_addresses(addresses);
        }
        if let Some(token) = &ready.media_token {
            self.context
                .media_tokens
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(self.context.key.clone(), token.token.clone());
        }
        let mirror = GuildMirror::from_ready(&ready);
        self.permissions = Some(mirror.permissions());
        self.mirror = Some(mirror);
        let name = ready.server.as_ref().map(|server| server.name.clone());
        let user_id = ready.self_user.as_ref().map(|user| user.id);
        self.self_id = user_id;
        self.update_saved_server(name, user_id);
        self.emit(CoreEventPayload::Ready(convert::ready(ready)));
    }

    fn on_event(&mut self, seq: u64, event: proto::Event) {
        if let Some(session) = &mut self.session {
            session.last_seq = session.last_seq.max(seq);
        }
        let Some(kind) = event.kind else {
            return;
        };
        let affects_permissions = self
            .mirror
            .as_mut()
            .is_some_and(|mirror| mirror.apply(&kind));
        if let proto::event::Kind::ServerUpdate(update) = &kind {
            let name = update.server.as_ref().map(|server| server.name.clone());
            self.update_saved_server(name, None);
        }
        if let proto::event::Kind::VoiceServerUpdate(update) = &kind {
            self.on_voice_server(update);
        }
        if let proto::event::Kind::ServerAddressesUpdate(update) = &kind
            && let Some(addresses) = &update.addresses
        {
            self.save_addresses(addresses);
        }
        let session_id = self
            .session
            .as_ref()
            .map_or("", |session| session.session_id.as_str());
        if let Some(payload) = convert::event(kind, session_id) {
            self.emit(payload);
        }
        if affects_permissions {
            self.emit_permissions_if_changed();
        }
    }

    /// For the media engine; nobody listening is fine.
    fn on_voice_server(&self, update: &proto::VoiceServerUpdate) {
        let (Some(session), Some(user_id)) = (&self.session, self.self_id) else {
            return;
        };
        // The server's own node is at the address this connection uses,
        // checked the same way.
        let route = self
            .context
            .routes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&self.context.key)
            .cloned();
        let (endpoint, certificate_fingerprint) = match &route {
            Some(route) if update.endpoint.is_empty() => (
                format!("wss://{}/voice", route.address),
                route.pin.map(|pin| pin.to_vec()).unwrap_or_default(),
            ),
            _ => (
                update.endpoint.clone(),
                update.certificate_fingerprint.clone(),
            ),
        };
        let _ = self.context.voice_servers.send(VoiceServer {
            server_key: self.context.key.clone(),
            user_id,
            session_id: session.session_id.clone(),
            channel_id: update.channel_id,
            endpoint,
            certificate_fingerprint,
            token: update.token.clone(),
        });
    }

    fn emit_permissions_if_changed(&mut self) {
        let Some(snapshot) = self.mirror.as_ref().map(GuildMirror::permissions) else {
            return;
        };
        if self.permissions.as_ref() == Some(&snapshot) {
            return;
        }
        self.emit(CoreEventPayload::PermissionsUpdate {
            server_permissions: convert::bits_to_api(snapshot.server),
            channel_permissions: convert::channel_permissions(snapshot.channels.clone()),
        });
        self.permissions = Some(snapshot);
    }

    fn update_saved_server(&self, name: Option<String>, user_id: Option<i64>) {
        let mut store = self.lock_store();
        let Some(saved) = store.server(&self.context.key).cloned() else {
            return;
        };
        let updated = crate::store::SavedServer {
            name: name.unwrap_or(saved.name.clone()),
            user_id: user_id.or(saved.user_id),
            ..saved.clone()
        };
        if updated != saved {
            let _ = store.upsert_server(updated);
        }
    }

    /// Waits before the next attempt, at least `not_before`, refusing
    /// requests meanwhile. Returns `false` when asked to shut down.
    async fn back_off(&mut self, attempt: u32, not_before: Duration) -> bool {
        let delay = backoff_delay(attempt, jitter()).max(not_before);
        self.emit_state(ConnectionState::Reconnecting {
            attempt,
            retry_in_ms: u32::try_from(delay.as_millis()).unwrap_or(u32::MAX),
        });
        let deadline = Instant::now() + delay;
        loop {
            tokio::select! {
                () = sleep_until(deadline) => return true,
                command = self.commands.recv() => match command {
                    Some(Command::Request { reply, .. }) => {
                        let _ = reply.send(Err(CoreError::NotConnected));
                    }
                    Some(Command::RetryNow | Command::Probe) => {
                        // No more waiting: says so, so a countdown stops.
                        self.emit_state(ConnectionState::Reconnecting {
                            attempt,
                            retry_in_ms: 0,
                        });
                        return true;
                    }
                    Some(Command::Shutdown) | None => return false,
                },
            }
        }
    }

    /// After a fatal failure: refuse requests until asked to retry
    /// (`true`) or to shut down (`false`).
    async fn park(&mut self) -> bool {
        while let Some(command) = self.commands.recv().await {
            match command {
                Command::Request { reply, .. } => {
                    let _ = reply.send(Err(CoreError::NotConnected));
                }
                Command::RetryNow => return true,
                // A failure that needs the user, not the network.
                Command::Probe => {}
                Command::Shutdown => return false,
            }
        }
        false
    }

    fn emit_state(&self, state: ConnectionState) {
        self.emit(CoreEventPayload::ConnectionState(state));
    }

    fn emit(&self, payload: CoreEventPayload) {
        let _ = self.context.events.send(CoreEvent {
            server_key: self.context.key.clone(),
            payload,
        });
    }

    fn lock_store(&self) -> std::sync::MutexGuard<'_, Store> {
        self.context
            .store
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// How a close code from the server ends the connection.
fn closed(code: Option<u16>) -> Ended {
    match code {
        Some(4010) => Ended::fatal(FailureReason::Kicked, "you were kicked from this server"),
        Some(4011) => Ended::fatal(FailureReason::Banned, "you were banned from this server"),
        Some(4003) => Ended::fatal(FailureReason::Rejected, "the server refused this identity"),
        Some(4002 | 4004 | 4009) => Ended::LostSession,
        _ => Ended::Lost,
    }
}

fn handshake_error(error: proto::Error) -> HandshakeError {
    match proto::ErrorCode::try_from(error.code) {
        Ok(proto::ErrorCode::RateLimited) => HandshakeError::RateLimited {
            message: error.message,
            retry_after: Duration::from_millis(error.retry_after_ms.unwrap_or_default()),
        },
        Ok(proto::ErrorCode::Unauthorized | proto::ErrorCode::InvalidArgument) => {
            HandshakeError::Rejected {
                reason: FailureReason::Rejected,
                message: error.message,
            }
        }
        _ => HandshakeError::Transient(error.message),
    }
}

async fn send_payload(
    socket: &mut Socket,
    request_id: u64,
    payload: Payload,
) -> Result<(), HandshakeError> {
    let envelope = proto::Envelope {
        seq: 0,
        request_id,
        payload: Some(payload),
    };
    socket
        .send(Frame::Binary(envelope.encode_to_vec().into()))
        .await
        .map_err(|error| HandshakeError::Transient(error.to_string()))
}

/// Next envelope during a handshake: `(seq, request_id, payload)`.
async fn next_payload(socket: &mut Socket) -> Result<(u64, u64, Payload), HandshakeError> {
    let deadline = Instant::now() + HANDSHAKE_TIMEOUT;
    loop {
        let frame = tokio::time::timeout_at(deadline, socket.next())
            .await
            .map_err(|_| HandshakeError::Transient("the server stopped responding".to_owned()))?;
        match frame {
            Some(Ok(Frame::Binary(bytes))) => {
                let envelope = proto::Envelope::decode(bytes).map_err(|_| {
                    HandshakeError::Transient("the server sent an invalid frame".to_owned())
                })?;
                if let Some(payload) = envelope.payload {
                    return Ok((envelope.seq, envelope.request_id, payload));
                }
            }
            Some(Ok(Frame::Close(frame))) => {
                let code = frame.map(|frame| u16::from(frame.code));
                return Err(match closed(code) {
                    Ended::Fatal {
                        reason, message, ..
                    } => HandshakeError::Rejected { reason, message },
                    _ => HandshakeError::Transient("the server closed the connection".to_owned()),
                });
            }
            Some(Ok(_)) => {}
            Some(Err(error)) => return Err(HandshakeError::Transient(error.to_string())),
            None => {
                return Err(HandshakeError::Transient(
                    "the server closed the connection".to_owned(),
                ));
            }
        }
    }
}

fn heartbeat_interval(hello: &proto::Hello) -> Duration {
    Duration::from_millis(u64::from(hello.heartbeat_interval_ms.max(1_000)))
}

fn unix_ms() -> u64 {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    u64::try_from(since_epoch.as_millis()).unwrap_or(u64::MAX)
}

/// A random number between 0 and 1.
fn jitter() -> f64 {
    let mut bytes = [0; 4];
    if getrandom::fill(&mut bytes).is_err() {
        return 0.5;
    }
    f64::from(u32::from_ne_bytes(bytes)) / f64::from(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_starts_near_one_second_and_caps_at_thirty() {
        assert_eq!(backoff_delay(1, 0.0), Duration::from_millis(750));
        assert_eq!(backoff_delay(1, 1.0), Duration::from_millis(1_250));
        assert_eq!(backoff_delay(2, 0.5), Duration::from_secs(2));
        assert_eq!(backoff_delay(4, 0.5), Duration::from_secs(8));
        assert_eq!(backoff_delay(6, 0.5), Duration::from_secs(30));
        assert_eq!(backoff_delay(60, 1.0), Duration::from_secs(30));
        assert!(backoff_delay(60, 0.0) >= Duration::from_millis(22_500));
    }

    #[test]
    fn a_refused_sign_in_waits_as_long_as_the_server_says() {
        let refused = handshake_error(proto::Error {
            code: proto::ErrorCode::RateLimited as i32,
            message: "slow down".to_owned(),
            retry_after_ms: Some(40_000),
        });
        let HandshakeError::RateLimited { retry_after, .. } = refused else {
            panic!("a rate limit is retried later, got {refused:?}");
        };
        assert_eq!(retry_after, Duration::from_secs(40));
        // Longer than the first backoff, so that is what the retry waits.
        assert!(backoff_delay(1, 1.0).max(retry_after) == retry_after);
        // The app's join flow hears the wait too.
        assert!(matches!(
            CoreError::from(HandshakeError::RateLimited {
                message: "slow down".to_owned(),
                retry_after,
            }),
            CoreError::Server {
                code: ErrorCode::RateLimited,
                retry_after_ms: Some(40_000),
                ..
            }
        ));
    }

    #[test]
    fn close_codes_decide_whether_to_retry() {
        assert!(matches!(
            closed(Some(4010)),
            Ended::Fatal {
                reason: FailureReason::Kicked,
                ..
            }
        ));
        assert!(matches!(
            closed(Some(4011)),
            Ended::Fatal {
                reason: FailureReason::Banned,
                ..
            }
        ));
        assert!(matches!(closed(Some(4009)), Ended::LostSession));
        assert!(matches!(closed(Some(4005)), Ended::Lost));
        assert!(matches!(closed(None), Ended::Lost));
    }
}
