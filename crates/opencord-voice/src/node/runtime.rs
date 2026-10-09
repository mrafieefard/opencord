//! The node's one task: the UDP sockets, the SFU and the voice sessions.
//! Voice gateway connections and the main server talk to it through
//! commands; it answers through each session's connection and node events.

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ed25519_dalek::VerifyingKey;
use opencord_common::permissions::Permissions;
use opencord_common::video::{
    H264_FMTP, H264_PAYLOAD_TYPE, H264_RTX_PAYLOAD_TYPE, VIDEO_CLOCK_RATE,
};
use opencord_common::voice::{OPUS_PAYLOAD_TYPE, close, speaking};
use opencord_proto::voice::v1 as voice;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, oneshot};
use voice::envelope::Payload;

use super::session::{Connection, NewSession, VoiceSession};
use super::tracks::Refusal;
use super::{Counters, NodeCommand, NodeEvent};
use crate::sfu::{PeerId, PeerSetup, PeerState, Sfu, SfuEvent, Transport};
use video::track_message;

mod video;
use crate::token::{self, Presenter, UsedTokens};

/// How long a voice session waits for its WebSocket to come back.
pub const RESUME_WINDOW: Duration = Duration::from_secs(30);
/// Housekeeping when nothing else wakes the task.
const TICK: Duration = Duration::from_secs(1);
/// Datagrams read in one go, each stamped as it is read, before any is
/// handled: bandwidth estimation needs arrival times as they were.
const READ_BATCH: usize = 32;
const DATAGRAM: usize = 2048;
/// Updates for people not connected yet are kept this long, as long as a
/// voice token lives.
const PENDING_FOR: Duration = token::VOICE_TOKEN_LIFETIME;

/// Why a voice session ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ending {
    /// Its connection did not come back within the resume window: the only
    /// ending the main server does not know about already.
    Expired,
    /// The same user connected again.
    Replaced,
    /// The main server asked.
    Disconnected,
    /// The node is stopping, or lost the main server; the main server sends
    /// everyone elsewhere.
    NodeShutdown,
}

impl Ending {
    fn close_code(self) -> u16 {
        match self {
            Self::Expired => close::SESSION_INVALID,
            Self::Replaced => close::SESSION_REPLACED,
            Self::Disconnected => close::DISCONNECTED,
            Self::NodeShutdown => close::NODE_SHUTDOWN,
        }
    }
}

pub enum Command {
    Identify {
        identify: voice::Identify,
        connection: Connection,
        reply: oneshot::Sender<Result<Identified, IdentifyError>>,
    },
    Resume {
        resume: voice::Resume,
        connection: Connection,
        reply: oneshot::Sender<Option<String>>,
    },
    Transport {
        session: String,
        info: voice::TransportInfo,
    },
    Speaking {
        session: String,
        flags: u32,
    },
    Publish {
        session: String,
        publish: voice::PublishTrack,
    },
    Unpublish {
        session: String,
        track_id: String,
    },
    SinkWants {
        session: String,
        wants: voice::MediaSinkWants,
    },
    Detached {
        session: String,
        connection_id: u64,
    },
    Node(NodeCommand),
    VerifyingKey(VerifyingKey),
    Suspend,
    Shutdown,
}

/// What a connection needs after a successful Identify.
pub struct Identified {
    pub session_id: String,
}

#[derive(Debug, thiserror::Error)]
pub enum IdentifyError {
    #[error("this node does not know the main server's key yet")]
    NoKey,
    #[error(transparent)]
    Token(#[from] token::TokenError),
    #[error("the media connection could not be prepared")]
    Media,
}

pub struct Runtime {
    pub sfu: Sfu,
    pub v4: UdpSocket,
    pub v6: Option<UdpSocket>,
    pub verifying_key: Option<VerifyingKey>,
    /// Candidate ip clients send media to; empty for "the host you reached
    /// the voice gateway with".
    pub public_ip: String,
    pub udp_port: u16,
    pub events: mpsc::UnboundedSender<NodeEvent>,
    pub used_tokens: UsedTokens,
    pub sessions: HashMap<String, VoiceSession>,
    /// What the main server last said about people not connected (yet).
    pub pending: HashMap<(i64, i64), (PeerState, u64, Instant)>,
    pub limits: HashMap<i64, voice::Limits>,
    pub counters: Arc<Counters>,
}

impl Runtime {
    pub async fn run(mut self, mut commands: mpsc::UnboundedReceiver<Command>) {
        let mut v4_batch = Batch::new();
        let mut v6_batch = Batch::new();
        loop {
            let now = Instant::now();
            let wake = self
                .sfu
                .next_timeout()
                .map_or(now + TICK, |at| at.min(now + TICK));
            tokio::select! {
                received = self.v4.recv_from(&mut v4_batch.buffers[0]) => {
                    if let Ok(first) = received {
                        let count = v4_batch.fill(&self.v4, first);
                        self.handle_batch(&v4_batch, count);
                    }
                }
                received = recv_maybe(self.v6.as_ref(), &mut v6_batch.buffers[0]) => {
                    if let (Ok(first), Some(v6)) = (received, self.v6.as_ref()) {
                        let count = v6_batch.fill(v6, first);
                        self.handle_batch(&v6_batch, count);
                    }
                }
                command = commands.recv() => match command {
                    Some(Command::Shutdown) | None => {
                        self.shut_down();
                        return;
                    }
                    Some(command) => self.handle(command),
                },
                () = tokio::time::sleep_until(wake.into()) => {
                    let now = Instant::now();
                    self.sfu.handle_timeout(now);
                    self.expire(now);
                }
            }
            self.flush();
        }
    }

    fn handle_batch(&mut self, batch: &Batch, count: usize) {
        for (datagram, (size, source, at)) in batch.buffers.iter().zip(&batch.read).take(count) {
            self.counters.received(*size);
            self.sfu.handle_receive(*at, *source, &datagram[..*size]);
        }
    }

    fn handle(&mut self, command: Command) {
        let now = Instant::now();
        match command {
            Command::Identify {
                identify,
                connection,
                reply,
            } => {
                let result = self.identify(now, identify, connection);
                let _ = reply.send(result);
            }
            Command::Resume {
                resume,
                connection,
                reply,
            } => {
                let resumed = self
                    .sessions
                    .get_mut(&resume.voice_session_id)
                    .and_then(|session| {
                        session
                            .resume(&resume.resume_token, resume.last_seq, connection, now)
                            .ok()
                            .map(|_| session.id.clone())
                    });
                let _ = reply.send(resumed);
            }
            Command::Transport { session, info } => {
                let Some(peer) = self.sessions.get(&session).map(|s| s.peer) else {
                    return;
                };
                let remote = Transport {
                    ice_ufrag: info.ice_ufrag,
                    ice_pwd: info.ice_pwd,
                    dtls_fingerprint: info.dtls_fingerprint,
                };
                if let Err(error) = self.sfu.start_peer(peer, remote) {
                    tracing::debug!(%error, "a transport was refused");
                    if let Some(session) = self.sessions.get_mut(&session) {
                        session.close(close::INVALID_FRAME);
                    }
                }
            }
            Command::Speaking { session, flags } => self.speaking(now, &session, flags),
            Command::Publish { session, publish } => self.publish(now, &session, publish),
            Command::Unpublish { session, track_id } => self.unpublish(now, &session, &track_id),
            Command::SinkWants { session, wants } => self.sink_wants(now, &session, &wants),
            Command::Detached {
                session,
                connection_id,
            } => {
                if let Some(session) = self.sessions.get_mut(&session) {
                    session.detach(connection_id, now);
                }
            }
            Command::Node(command) => self.node_command(now, command),
            Command::VerifyingKey(key) => self.verifying_key = Some(key),
            Command::Suspend => {
                self.end_all(now);
                self.verifying_key = None;
            }
            Command::Shutdown => {}
        }
    }

    fn identify(
        &mut self,
        now: Instant,
        identify: voice::Identify,
        connection: Connection,
    ) -> Result<Identified, IdentifyError> {
        let presenter = Presenter {
            user_id: identify.user_id,
            session_id: &identify.session_id,
            channel_id: identify.channel_id,
        };
        let key = self.verifying_key.as_ref().ok_or(IdentifyError::NoKey)?;
        let claims = token::verify(key, &identify.token, presenter, unix_ms())?;
        self.used_tokens.redeem(&claims, unix_ms())?;

        // One voice connection per user on a node: a new one replaces it.
        let previous: Vec<String> = self
            .sessions
            .values()
            .filter(|session| session.user_id == claims.user_id)
            .map(|session| session.id.clone())
            .collect();
        for id in previous {
            self.end_session(now, &id, Ending::Replaced);
        }

        let (mut state, mut permissions) = (
            PeerState {
                self_mute: claims.self_mute,
                self_deaf: claims.self_deaf,
                server_mute: claims.server_mute,
                server_deaf: claims.server_deaf,
                suppress: claims.suppress,
            },
            claims.permissions,
        );
        if let Some((pending_state, pending_permissions, _)) =
            self.pending.remove(&(claims.user_id, claims.channel_id))
        {
            state = pending_state;
            permissions = pending_permissions;
        }
        let audio_ssrc = self.free_ssrc(claims.channel_id, &mut Vec::new());
        let setup = PeerSetup {
            user_id: claims.user_id,
            channel_id: claims.channel_id,
            audio_ssrc,
            state,
        };
        let (peer, transport) = self
            .sfu
            .add_peer(now, setup)
            .map_err(|_| IdentifyError::Media)?;
        let others: Vec<voice::Participant> = self
            .sessions
            .values()
            .filter(|session| session.channel_id == claims.channel_id)
            .map(|session| voice::Participant {
                user_id: session.user_id,
                audio_ssrc: session.audio_ssrc,
                tracks: self
                    .sfu
                    .tracks(session.peer)
                    .iter()
                    .map(|track| track_message(track, voice::TrackState::Active))
                    .collect(),
            })
            .collect();
        if let Some(limits) = claims.limits {
            self.limits.entry(claims.channel_id).or_insert(limits);
        }
        let limits = Some(self.channel_limits(claims.channel_id));
        let session_id = hex::encode(random_bytes::<16>());
        let resume_token = random_bytes::<32>().to_vec();
        let mut session = VoiceSession::new(NewSession {
            id: session_id.clone(),
            resume_token: resume_token.clone(),
            user_id: claims.user_id,
            channel_id: claims.channel_id,
            main_session_id: claims.session_id,
            peer,
            audio_ssrc,
            permissions,
            connection,
        });
        session.send(
            Payload::Ready(voice::Ready {
                voice_session_id: session_id.clone(),
                resume_token,
                audio_ssrc,
                ice_ufrag: transport.ice_ufrag,
                ice_pwd: transport.ice_pwd,
                candidates: vec![voice::IceCandidate {
                    ip: self.public_ip.clone(),
                    port: u32::from(self.udp_port),
                }],
                dtls_fingerprint: transport.dtls_fingerprint,
                codecs: vec![
                    voice::CodecParams {
                        codec: voice::Codec::Opus as i32,
                        payload_type: u32::from(OPUS_PAYLOAD_TYPE),
                        clock_rate: 48_000,
                        rtx_payload_type: 0,
                        fmtp: "minptime=10;useinbandfec=1".to_owned(),
                    },
                    voice::CodecParams {
                        codec: voice::Codec::H264 as i32,
                        payload_type: u32::from(H264_PAYLOAD_TYPE),
                        clock_rate: VIDEO_CLOCK_RATE,
                        rtx_payload_type: u32::from(H264_RTX_PAYLOAD_TYPE),
                        fmtp: H264_FMTP.to_owned(),
                    },
                ],
                limits,
                participants: others,
            }),
            now,
        );
        let connected = voice::ClientConnect {
            user_id: claims.user_id,
            audio_ssrc,
            tracks: Vec::new(),
        };
        for other in self
            .sessions
            .values_mut()
            .filter(|other| other.channel_id == claims.channel_id)
        {
            other.send_sequenced(Payload::ClientConnect(connected.clone()), now);
        }
        self.sessions.insert(session_id.clone(), session);
        self.count_sessions();
        Ok(Identified { session_id })
    }

    fn speaking(&mut self, now: Instant, session_id: &str, flags: u32) {
        let Some(session) = self.sessions.get(session_id) else {
            return;
        };
        let may_priority = Permissions::from_bits_truncate(session.permissions)
            .contains(Permissions::PRIORITY_SPEAKER);
        let flags = if may_priority {
            flags
        } else {
            flags & !speaking::PRIORITY
        };
        let relayed = voice::Speaking {
            flags,
            user_id: session.user_id,
        };
        let channel_id = session.channel_id;
        for other in self
            .sessions
            .values_mut()
            .filter(|other| other.channel_id == channel_id && other.id != session_id)
        {
            other.send_sequenced(Payload::Speaking(relayed), now);
        }
    }

    fn node_command(&mut self, now: Instant, command: NodeCommand) {
        match command {
            NodeCommand::Update {
                user_id,
                channel_id,
                state,
                permissions,
            } => {
                let session = self
                    .sessions
                    .values_mut()
                    .find(|s| s.user_id == user_id && s.channel_id == channel_id);
                match session {
                    Some(session) => {
                        session.permissions = permissions;
                        let (peer, id) = (session.peer, session.id.clone());
                        self.sfu.set_state(peer, state);
                        self.recheck_tracks(now, &id);
                    }
                    None => {
                        self.pending
                            .insert((user_id, channel_id), (state, permissions, now));
                    }
                }
            }
            NodeCommand::Disconnect {
                user_id,
                channel_id,
            } => {
                self.pending.remove(&(user_id, channel_id));
                let ids: Vec<String> = self
                    .sessions
                    .values()
                    .filter(|s| s.user_id == user_id && s.channel_id == channel_id)
                    .map(|s| s.id.clone())
                    .collect();
                for id in ids {
                    self.end_session(now, &id, Ending::Disconnected);
                }
            }
            NodeCommand::StreamViewers {
                channel_id,
                user_id,
                viewers,
            } => self
                .sfu
                .set_stream_viewers(now, channel_id, user_id, viewers),
            NodeCommand::Limits { channel_id, limits } => {
                self.limits.insert(channel_id, limits);
                let ids: Vec<String> = self
                    .sessions
                    .values()
                    .filter(|session| session.channel_id == channel_id)
                    .map(|session| session.id.clone())
                    .collect();
                for id in ids {
                    self.recheck_tracks(now, &id);
                }
            }
        }
    }

    /// Removes a session and its participant, and tells the others; an
    /// expired one is reported to the main server.
    fn end_session(&mut self, now: Instant, id: &str, ending: Ending) {
        let Some(mut session) = self.sessions.remove(id) else {
            return;
        };
        session.close(ending.close_code());
        self.sfu.remove_peer(session.peer);
        self.count_sessions();
        let gone = voice::ClientDisconnect {
            user_id: session.user_id,
        };
        for other in self
            .sessions
            .values_mut()
            .filter(|other| other.channel_id == session.channel_id)
        {
            other.send_sequenced(Payload::ClientDisconnect(gone), now);
        }
        if ending == Ending::Expired {
            let _ = self.events.send(NodeEvent::Disconnected {
                user_id: session.user_id,
                channel_id: session.channel_id,
                session_id: session.main_session_id,
            });
        }
    }

    fn expire(&mut self, now: Instant) {
        let expired: Vec<String> = self
            .sessions
            .values()
            .filter(|session| session.is_expired(now, RESUME_WINDOW))
            .map(|session| session.id.clone())
            .collect();
        for id in expired {
            self.end_session(now, &id, Ending::Expired);
        }
        self.pending
            .retain(|_, (_, _, at)| now.saturating_duration_since(*at) < PENDING_FOR);
    }

    /// Sends what the SFU produced and handles its events.
    fn flush(&mut self) {
        while let Some(transmit) = self.sfu.poll_transmit() {
            let socket = if transmit.destination.is_ipv4() {
                Some(&self.v4)
            } else {
                self.v6.as_ref()
            };
            if let Some(socket) = socket {
                // A full send buffer drops the packet, as the network would.
                if socket
                    .try_send_to(&transmit.contents, transmit.destination)
                    .is_ok()
                {
                    self.counters.sent(transmit.contents.len());
                }
            }
        }
        while let Some(event) = self.sfu.poll_event() {
            match event {
                SfuEvent::Connected(peer) => {
                    if let Some(session) = self.session_by_peer(peer) {
                        session.media_connected = true;
                        let event = NodeEvent::Connected {
                            user_id: session.user_id,
                            channel_id: session.channel_id,
                            session_id: session.main_session_id.clone(),
                        };
                        let _ = self.events.send(event);
                    }
                }
                // The client restarts ICE or resumes; the session ends only
                // when its gateway connection does not come back.
                SfuEvent::Disconnected(_) => {}
                SfuEvent::LayerWants {
                    peer,
                    track_id,
                    rids,
                } => {
                    if let Some(session) = self.session_by_peer(peer) {
                        let wants = voice::SenderLayerWants {
                            track_id,
                            active_layers: rids,
                        };
                        session.send_sequenced(Payload::SenderLayerWants(wants), Instant::now());
                    }
                }
                SfuEvent::LayersAvailable {
                    peer,
                    track_id,
                    rids,
                } => self.layers_available(peer, &track_id, rids),
                SfuEvent::TrackStopped { peer, track_id } => {
                    let refusal = Refusal {
                        code: opencord_proto::v1::ErrorCode::QualityLimit,
                        message: "the track stayed over its bitrate ceiling".to_owned(),
                    };
                    self.track_ended(Instant::now(), peer, &track_id, Some(refusal));
                }
            }
        }
    }

    fn session_by_peer(&mut self, peer: PeerId) -> Option<&mut VoiceSession> {
        self.sessions.values_mut().find(|s| s.peer == peer)
    }

    /// An SSRC nobody in the channel uses, and not in `taken`, which it
    /// joins.
    fn free_ssrc(&self, channel_id: i64, taken: &mut Vec<u32>) -> u32 {
        loop {
            let candidate = u32::from_ne_bytes(random_bytes::<4>());
            let used = taken.contains(&candidate)
                || self.sfu.ssrc_in_use(channel_id, candidate)
                || self
                    .sessions
                    .values()
                    .any(|s| s.channel_id == channel_id && s.audio_ssrc == candidate);
            if candidate != 0 && !used {
                taken.push(candidate);
                return candidate;
            }
        }
    }

    fn count_sessions(&self) {
        let channels: HashSet<i64> = self.sessions.values().map(|s| s.channel_id).collect();
        self.counters.set_sessions(
            u32::try_from(channels.len()).unwrap_or(u32::MAX),
            u32::try_from(self.sessions.len()).unwrap_or(u32::MAX),
        );
    }

    fn end_all(&mut self, now: Instant) {
        let ids: Vec<String> = self.sessions.keys().cloned().collect();
        for id in ids {
            self.end_session(now, &id, Ending::NodeShutdown);
        }
        self.pending.clear();
    }

    fn shut_down(&mut self) {
        self.end_all(Instant::now());
        self.flush();
    }
}

/// Datagrams read together: each one's length, sender and arrival.
struct Batch {
    buffers: Vec<[u8; DATAGRAM]>,
    read: Vec<(usize, SocketAddr, Instant)>,
}

impl Batch {
    fn new() -> Self {
        let nowhere = SocketAddr::from((std::net::Ipv4Addr::UNSPECIFIED, 0));
        Self {
            buffers: vec![[0; DATAGRAM]; READ_BATCH],
            read: vec![(0, nowhere, Instant::now()); READ_BATCH],
        }
    }

    /// After the first datagram (already in the first buffer), reads what
    /// else is waiting; returns how many there are.
    fn fill(&mut self, socket: &UdpSocket, first: (usize, SocketAddr)) -> usize {
        self.read[0] = (first.0, first.1, Instant::now());
        let mut count = 1;
        while count < READ_BATCH {
            let Ok((size, source)) = socket.try_recv_from(&mut self.buffers[count]) else {
                break;
            };
            self.read[count] = (size, source, Instant::now());
            count += 1;
        }
        count
    }
}

async fn recv_maybe(
    socket: Option<&UdpSocket>,
    buffer: &mut [u8],
) -> std::io::Result<(usize, SocketAddr)> {
    match socket {
        Some(socket) => socket.recv_from(buffer).await,
        None => std::future::pending().await,
    }
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes).expect("the operating system's random source is unavailable");
    bytes
}

fn unix_ms() -> i64 {
    let since = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
}
