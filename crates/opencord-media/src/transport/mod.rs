//! The voice transport (Phase 2 plan §7.1, §7.14): the voice gateway
//! WebSocket for signalling, and one str0m connection over UDP for media,
//! in RTP mode. It moves Opus packets; encoding, decoding and mixing are the
//! audio engine's.
//!
//! A dropped gateway is resumed while media keeps flowing; only a refused
//! resume or a close the node means ends the connection.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use opencord_common::voice::{AUDIO_MID, OPUS_PAYLOAD_TYPE, close, receive_mid};
use opencord_proto::voice::v1 as voice;
use str0m::config::Fingerprint;
use str0m::ice::IceCreds;
use str0m::media::{MediaKind, Pt};
use str0m::net::{Protocol, Receive};
use str0m::rtp::{ExtensionValues, RtpWrite, Ssrc};
use str0m::{Candidate, Event, IceConnectionState, Input, Output, Rtc, RtcConfig};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use voice::envelope::Payload;

use gateway::{GatewayUrl, Next, Socket};

pub mod gateway;

/// How long a dropped gateway is tried again, as long as the node keeps
/// the voice session.
const RESUME_FOR: Duration = Duration::from_secs(30);
const RESUME_PAUSE: Duration = Duration::from_millis(500);
/// How often the network is checked for a change (plan §7.14: back in
/// under 3 s after switching Wi-Fi).
const NETWORK_CHECK: Duration = Duration::from_secs(1);

/// Where to connect, from the main server's `VoiceServerUpdate`.
#[derive(Debug, Clone)]
pub struct VoiceTarget {
    /// The voice gateway, `wss://host:port/voice`.
    pub gateway_url: String,
    /// SHA-256 of the node's certificate. Needed for `wss://`.
    pub certificate_fingerprint: Option<[u8; 32]>,
    pub token: Vec<u8>,
    pub user_id: i64,
    pub session_id: String,
    pub channel_id: i64,
}

/// One Opus frame to send.
#[derive(Debug, Clone)]
pub struct AudioFrame {
    pub payload: Vec<u8>,
    /// The frame's first sample on the sender's 48 kHz capture clock. It
    /// keeps counting through silence, so the RTP timestamps show pauses.
    pub position: u64,
    /// The first frame of a talk spurt.
    pub marker: bool,
    /// -dBov: 0 is loudest, -127 silent.
    pub audio_level: i8,
    pub voice_activity: bool,
}

/// Someone's Opus packet, as it arrived.
#[derive(Debug, Clone)]
pub struct ReceivedAudio {
    pub user_id: i64,
    pub ssrc: u32,
    pub seq: u64,
    pub timestamp: u32,
    /// The first packet of a talk spurt.
    pub marker: bool,
    pub payload: Arc<[u8]>,
    pub audio_level: Option<i8>,
    pub arrived: Instant,
}

#[derive(Debug, Clone)]
pub enum VoiceEvent {
    /// ICE and DTLS are up: audio can flow.
    MediaConnected,
    /// The media path broke; the gateway may still be up.
    MediaDisconnected,
    /// Joined, or already there when this connection began.
    ClientConnected {
        user_id: i64,
        audio_ssrc: u32,
    },
    ClientDisconnected {
        user_id: i64,
    },
    Audio(ReceivedAudio),
    Speaking {
        user_id: i64,
        flags: u32,
    },
    /// The gateway dropped and came back without losing anything.
    Resumed,
    /// Over for good. Join again through the main server for another one.
    Closed {
        code: Option<u16>,
    },
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum TransportError {
    #[error("not a voice gateway URL: {0}")]
    Url(String),
    #[error("a secure voice gateway needs its certificate fingerprint")]
    NoFingerprint,
    #[error("could not connect: {0}")]
    Connect(String),
    #[error("the voice node did not answer in time")]
    Timeout,
    #[error("the voice node refused to connect (close {0:?})")]
    Refused(Option<u16>),
    #[error("the voice node's answer was not usable: {0}")]
    Protocol(String),
}

enum Command {
    Audio(AudioFrame),
    Speaking(u32),
    DropGateway,
    Rebind,
    Close,
}

/// A connected voice session. Dropping it closes it.
pub struct VoiceConnection {
    commands: mpsc::UnboundedSender<Command>,
    audio_ssrc: u32,
    limits: Option<voice::Limits>,
}

impl VoiceConnection {
    /// Identifies with the voice node and starts media. Returns once the
    /// node is ready; [`VoiceEvent::MediaConnected`] follows.
    pub async fn connect(
        target: VoiceTarget,
    ) -> Result<(Self, mpsc::UnboundedReceiver<VoiceEvent>), TransportError> {
        let url = GatewayUrl::parse(&target.gateway_url)?;
        let mut socket = gateway::open(&url, target.certificate_fingerprint).await?;
        let heartbeat = hello(&mut socket).await?;
        gateway::send(
            &mut socket,
            Payload::Identify(voice::Identify {
                token: target.token.clone(),
                user_id: target.user_id,
                session_id: target.session_id.clone(),
                channel_id: target.channel_id,
                client_caps: Some(voice::ClientCaps {
                    codecs: vec![voice::CodecSupport {
                        codec: voice::Codec::Opus as i32,
                        hardware_encode: false,
                        hardware_decode: false,
                    }],
                    max_decode_height: 0,
                    simulcast: false,
                }),
                max_e2ee_version: 0,
            }),
        )
        .await?;
        let ready = ready(&mut socket).await?;
        let media = Media::start(&url.address.host, &ready).await?;
        gateway::send(
            &mut socket,
            Payload::TransportInfo(voice::TransportInfo {
                ice_ufrag: media.local.ufrag.clone(),
                ice_pwd: media.local.pass.clone(),
                dtls_fingerprint: media.fingerprint.clone(),
            }),
        )
        .await?;
        let (events, receiver) = mpsc::unbounded_channel();
        for participant in &ready.participants {
            let _ = events.send(VoiceEvent::ClientConnected {
                user_id: participant.user_id,
                audio_ssrc: participant.audio_ssrc,
            });
        }
        let (commands, command_receiver) = mpsc::unbounded_channel();
        let audio_ssrc = ready.audio_ssrc;
        let limits = ready.limits;
        let driver = Driver {
            url,
            fingerprint: target.certificate_fingerprint,
            voice_session_id: ready.voice_session_id,
            resume_token: ready.resume_token,
            heartbeat,
            awaiting_ack: false,
            last_seq: 0,
            media,
            events,
        };
        tokio::spawn(driver.run(socket, command_receiver));
        Ok((
            Self {
                commands,
                audio_ssrc,
                limits,
            },
            receiver,
        ))
    }

    /// The SSRC this connection's audio goes out with.
    pub fn audio_ssrc(&self) -> u32 {
        self.audio_ssrc
    }

    /// The channel's voice bitrate in bits per second, as the node said.
    pub fn voice_bitrate(&self) -> Option<u32> {
        self.limits
            .map(|limits| limits.voice_bitrate)
            .filter(|bitrate| *bitrate > 0)
    }

    pub fn send_audio(&self, frame: AudioFrame) {
        let _ = self.commands.send(Command::Audio(frame));
    }

    /// `flags` from [`opencord_common::voice::speaking`].
    pub fn set_speaking(&self, flags: u32) {
        let _ = self.commands.send(Command::Speaking(flags));
    }

    /// Drops the gateway as a network failure would, to exercise resuming.
    #[doc(hidden)]
    pub fn drop_gateway(&self) {
        let _ = self.commands.send(Command::DropGateway);
    }

    /// Moves media to a new socket as a network change would, to exercise
    /// recovering from one.
    #[doc(hidden)]
    pub fn simulate_network_change(&self) {
        let _ = self.commands.send(Command::Rebind);
    }

    pub fn close(&self) {
        let _ = self.commands.send(Command::Close);
    }
}

impl Drop for VoiceConnection {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Close);
    }
}

async fn hello(socket: &mut Socket) -> Result<Duration, TransportError> {
    match tokio::time::timeout(gateway::CONNECT_TIMEOUT, gateway::next(socket)).await {
        Ok(Next::Envelope(envelope)) => match envelope.payload {
            Some(Payload::Hello(hello)) => Ok(Duration::from_millis(u64::from(
                hello.heartbeat_interval_ms.max(1_000),
            ))),
            _ => Err(TransportError::Protocol("expected Hello".to_owned())),
        },
        Ok(Next::Closed(code)) => Err(TransportError::Refused(code)),
        Err(_) => Err(TransportError::Timeout),
    }
}

async fn ready(socket: &mut Socket) -> Result<voice::Ready, TransportError> {
    match tokio::time::timeout(gateway::CONNECT_TIMEOUT, gateway::next(socket)).await {
        Ok(Next::Envelope(envelope)) => match envelope.payload {
            Some(Payload::Ready(ready)) => Ok(ready),
            _ => Err(TransportError::Protocol("expected Ready".to_owned())),
        },
        Ok(Next::Closed(code)) => Err(TransportError::Refused(code)),
        Err(_) => Err(TransportError::Timeout),
    }
}

/// The client's str0m connection and its UDP socket.
struct Media {
    rtc: Rtc,
    socket: UdpSocket,
    local_address: SocketAddr,
    remote_address: SocketAddr,
    local: IceCreds,
    fingerprint: Vec<u8>,
    audio_ssrc: u32,
    next_seq: u64,
    /// The RTP timestamp of capture position 0.
    timestamp_base: u32,
    users_by_ssrc: HashMap<u32, i64>,
    timeout: Instant,
    /// Sending failed in a way that means the network went away.
    network_lost: bool,
}

/// A UDP socket connected to the node, and the local address the system
/// picked for it.
async fn connected_socket(remote: SocketAddr) -> Result<(UdpSocket, SocketAddr), TransportError> {
    let error = |error: std::io::Error| TransportError::Connect(error.to_string());
    let any: SocketAddr = if remote.is_ipv4() {
        (std::net::Ipv4Addr::UNSPECIFIED, 0).into()
    } else {
        (std::net::Ipv6Addr::UNSPECIFIED, 0).into()
    };
    let socket = UdpSocket::bind(any).await.map_err(error)?;
    socket.connect(remote).await.map_err(error)?;
    let local = socket.local_addr().map_err(error)?;
    Ok((socket, local))
}

/// The local address the system would send to `remote` from now. Asking
/// sends nothing.
fn route_to(remote: SocketAddr) -> Option<std::net::IpAddr> {
    let any: SocketAddr = if remote.is_ipv4() {
        (std::net::Ipv4Addr::UNSPECIFIED, 0).into()
    } else {
        (std::net::Ipv6Addr::UNSPECIFIED, 0).into()
    };
    let probe = std::net::UdpSocket::bind(any).ok()?;
    probe.connect(remote).ok()?;
    probe.local_addr().ok().map(|address| address.ip())
}

impl Media {
    /// Media goes to the node's candidate, or to `gateway_host` when the
    /// candidate leaves its address empty ("the host you reached the voice
    /// gateway with").
    async fn start(gateway_host: &str, ready: &voice::Ready) -> Result<Self, TransportError> {
        let codec_ok = ready.codecs.iter().any(|codec| {
            codec.codec == voice::Codec::Opus as i32
                && codec.payload_type == u32::from(OPUS_PAYLOAD_TYPE)
        });
        if !codec_ok {
            return Err(TransportError::Protocol(
                "the node offers no Opus this client understands".to_owned(),
            ));
        }
        let candidate = ready
            .candidates
            .first()
            .ok_or_else(|| TransportError::Protocol("no media address".to_owned()))?;
        let host = if candidate.ip.is_empty() {
            gateway_host
        } else {
            candidate.ip.as_str()
        };
        let port = u16::try_from(candidate.port)
            .map_err(|_| TransportError::Protocol("a media port out of range".to_owned()))?;
        let remote_address = tokio::net::lookup_host((host, port))
            .await
            .map_err(|error| TransportError::Connect(error.to_string()))?
            .next()
            .ok_or_else(|| TransportError::Connect(format!("{host} has no address")))?;
        let (socket, local_address) = connected_socket(remote_address).await?;

        let now = Instant::now();
        let local = IceCreds::new();
        let mut rtc = RtcConfig::new()
            .set_rtp_mode(true)
            .clear_codecs()
            .enable_opus(true, false)
            .set_local_ice_credentials(local.clone())
            .build(now);
        rtc.add_local_candidate(Candidate::host(local_address, "udp").map_err(candidate_error)?);
        rtc.add_remote_candidate(Candidate::host(remote_address, "udp").map_err(candidate_error)?);
        let fingerprint = {
            let mut api = rtc.direct_api();
            api.set_remote_ice_credentials(IceCreds {
                ufrag: ready.ice_ufrag.clone(),
                pass: ready.ice_pwd.clone(),
            });
            api.set_remote_fingerprint(Fingerprint {
                hash_func: "sha-256".to_owned(),
                bytes: ready.dtls_fingerprint.clone(),
            });
            api.set_ice_controlling(true);
            api.declare_media(AUDIO_MID.into(), MediaKind::Audio);
            api.declare_stream_tx(Ssrc::from(ready.audio_ssrc), None, AUDIO_MID.into(), None);
            for participant in &ready.participants {
                hear(&mut api, participant.audio_ssrc);
            }
            api.start_dtls(true)
                .map_err(|error| TransportError::Connect(error.to_string()))?;
            api.local_dtls_fingerprint().bytes.clone()
        };
        Ok(Self {
            rtc,
            socket,
            local_address,
            remote_address,
            local,
            fingerprint,
            audio_ssrc: ready.audio_ssrc,
            next_seq: u64::from(u16::from_ne_bytes(random::<2>())),
            timestamp_base: u32::from_ne_bytes(random::<4>()),
            users_by_ssrc: ready
                .participants
                .iter()
                .map(|p| (p.audio_ssrc, p.user_id))
                .collect(),
            timeout: now,
            network_lost: false,
        })
    }

    fn hear(&mut self, user_id: i64, ssrc: u32) {
        self.users_by_ssrc.insert(ssrc, user_id);
        hear(&mut self.rtc.direct_api(), ssrc);
    }

    fn forget(&mut self, user_id: i64) {
        let gone: Vec<u32> = self
            .users_by_ssrc
            .iter()
            .filter(|(_, user)| **user == user_id)
            .map(|(ssrc, _)| *ssrc)
            .collect();
        for ssrc in gone {
            self.users_by_ssrc.remove(&ssrc);
            self.rtc
                .direct_api()
                .remove_media(receive_mid(ssrc).as_str().into());
        }
    }

    /// Whether the network this connection was on is gone: sending failed,
    /// or the system now reaches the node from another address.
    fn network_changed(&mut self) -> bool {
        std::mem::take(&mut self.network_lost)
            || route_to(self.remote_address).is_some_and(|ip| ip != self.local_address.ip())
    }

    /// Moves media to a new socket after a network change: its address
    /// becomes a new local candidate and the old one is dropped, so ICE
    /// checks the new path and moves over (the node learns the new address
    /// from the checks). DTLS and the voice session carry on.
    async fn rebind(&mut self) -> Result<(), TransportError> {
        let (socket, local_address) = connected_socket(self.remote_address).await?;
        let new = Candidate::host(local_address, "udp").map_err(candidate_error)?;
        let old = Candidate::host(self.local_address, "udp").map_err(candidate_error)?;
        self.rtc.add_local_candidate(new);
        self.rtc.direct_api().invalidate_candidate(&old);
        self.socket = socket;
        self.local_address = local_address;
        Ok(())
    }

    fn send_audio(&mut self, frame: &AudioFrame) {
        let seq = self.next_seq;
        self.next_seq += 1;
        // RTP timestamps are the capture clock, wrapped to 32 bits.
        let timestamp = self.timestamp_base.wrapping_add(frame.position as u32);
        let mut api = self.rtc.direct_api();
        let Some(stream) = api.stream_tx(&Ssrc::from(self.audio_ssrc)) else {
            return;
        };
        stream.write_rtp(
            RtpWrite::new(
                Pt::from(OPUS_PAYLOAD_TYPE),
                seq.into(),
                timestamp,
                Instant::now(),
                frame.payload.clone(),
            )
            .marker(frame.marker)
            .ext_vals(ExtensionValues {
                audio_level: Some(frame.audio_level),
                voice_activity: Some(frame.voice_activity),
                ..ExtensionValues::default()
            }),
        );
    }

    fn receive(&mut self, data: &[u8]) {
        let Ok(contents) = data.try_into() else {
            return;
        };
        let input = Input::Receive(
            Instant::now(),
            Receive {
                proto: Protocol::Udp,
                source: self.remote_address,
                destination: self.local_address,
                contents,
            },
        );
        if let Err(error) = self.rtc.handle_input(input) {
            tracing::debug!(%error, "a media packet was refused");
        }
    }

    fn timeout(&mut self) {
        if let Err(error) = self.rtc.handle_input(Input::Timeout(Instant::now())) {
            tracing::debug!(%error, "a media timeout failed");
        }
    }

    /// Sends what str0m wants sent and reports what happened.
    fn drain(&mut self, events: &mpsc::UnboundedSender<VoiceEvent>) {
        loop {
            match self.rtc.poll_output() {
                Ok(Output::Timeout(at)) => {
                    self.timeout = at;
                    return;
                }
                Ok(Output::Transmit(transmit)) => {
                    if let Err(error) = self.socket.try_send(&transmit.contents) {
                        // A full buffer drops the packet, as the network
                        // would; anything else means the network is gone.
                        if error.kind() != std::io::ErrorKind::WouldBlock {
                            self.network_lost = true;
                        }
                    }
                }
                Ok(Output::Event(Event::Connected)) => {
                    let _ = events.send(VoiceEvent::MediaConnected);
                }
                Ok(Output::Event(Event::IceConnectionStateChange(
                    IceConnectionState::Disconnected,
                ))) => {
                    let _ = events.send(VoiceEvent::MediaDisconnected);
                }
                Ok(Output::Event(Event::RtpPacket(packet))) => {
                    let ssrc = *packet.header.ssrc;
                    if let Some(user_id) = self.users_by_ssrc.get(&ssrc) {
                        let _ = events.send(VoiceEvent::Audio(ReceivedAudio {
                            user_id: *user_id,
                            ssrc,
                            seq: *packet.seq_no,
                            timestamp: packet.header.timestamp,
                            marker: packet.header.marker,
                            payload: packet.payload,
                            audio_level: packet.header.ext_vals.audio_level,
                            arrived: packet.timestamp,
                        }));
                    }
                }
                Ok(Output::Event(_)) => {}
                Err(error) => {
                    tracing::debug!(%error, "the media connection failed");
                    let _ = events.send(VoiceEvent::MediaDisconnected);
                    self.timeout = Instant::now() + Duration::from_secs(3600);
                    return;
                }
            }
        }
    }
}

fn candidate_error(error: impl std::fmt::Display) -> TransportError {
    TransportError::Connect(error.to_string())
}

fn hear(api: &mut str0m::change::DirectApi<'_>, ssrc: u32) {
    let mid = receive_mid(ssrc);
    api.declare_media(mid.as_str().into(), MediaKind::Audio);
    api.expect_stream_rx(Ssrc::from(ssrc), None, mid.as_str().into(), None);
}

/// The task behind a [`VoiceConnection`].
struct Driver {
    url: GatewayUrl,
    fingerprint: Option<[u8; 32]>,
    voice_session_id: String,
    resume_token: Vec<u8>,
    heartbeat: Duration,
    /// A heartbeat went out and its ack has not come back.
    awaiting_ack: bool,
    last_seq: u64,
    media: Media,
    events: mpsc::UnboundedSender<VoiceEvent>,
}

type Reconnect = JoinHandle<Result<Socket, Option<u16>>>;

impl Driver {
    async fn run(mut self, socket: Socket, mut commands: mpsc::UnboundedReceiver<Command>) {
        let mut socket = Some(socket);
        let mut reconnect: Option<Reconnect> = None;
        let mut buffer = vec![0u8; 2048];
        let mut beat = tokio::time::interval(self.heartbeat);
        beat.tick().await;
        let mut network = tokio::time::interval(NETWORK_CHECK);
        network.tick().await;
        let mut nonce = 0u64;
        self.media.drain(&self.events);
        loop {
            let wake = self.media.timeout.max(Instant::now());
            tokio::select! {
                received = self.media.socket.recv(&mut buffer) => {
                    if let Ok(size) = received {
                        self.media.receive(&buffer[..size]);
                    }
                }
                () = tokio::time::sleep_until(wake.into()) => self.media.timeout(),
                next = next_or_pending(socket.as_mut()) => match next {
                    Next::Envelope(envelope) => self.on_envelope(envelope),
                    Next::Closed(code) => {
                        socket = None;
                        if !close::is_resumable(code) {
                            self.finish(code);
                            return;
                        }
                        reconnect = Some(self.resume_later());
                    }
                },
                done = join_or_pending(reconnect.as_mut()) => {
                    reconnect = None;
                    match done {
                        Ok(Ok(resumed)) => {
                            socket = Some(resumed);
                            self.awaiting_ack = false;
                        }
                        Ok(Err(code)) => {
                            self.finish(code.or(Some(close::SESSION_INVALID)));
                            return;
                        }
                        Err(_) => {
                            self.finish(None);
                            return;
                        }
                    }
                }
                _ = network.tick() => {
                    if self.media.network_changed() {
                        self.rebind().await;
                    }
                }
                _ = beat.tick() => {
                    if let Some(open) = socket.as_mut() {
                        if self.awaiting_ack {
                            // The gateway went quiet: treat it as dropped.
                            socket = None;
                            reconnect = Some(self.resume_later());
                        } else {
                            nonce += 1;
                            self.awaiting_ack = true;
                            let beat = Payload::Heartbeat(voice::Heartbeat {
                                nonce,
                                client_ts_ms: unix_ms(),
                            });
                            if gateway::send(open, beat).await.is_err() {
                                socket = None;
                                reconnect = Some(self.resume_later());
                            }
                        }
                    }
                }
                command = commands.recv() => match command {
                    Some(Command::Audio(frame)) => self.media.send_audio(&frame),
                    Some(Command::Speaking(flags)) => {
                        if let Some(open) = socket.as_mut() {
                            let speaking = Payload::Speaking(voice::Speaking { flags, user_id: 0 });
                            let _ = gateway::send(open, speaking).await;
                        }
                    }
                    Some(Command::DropGateway) => {
                        socket = None;
                        reconnect = Some(self.resume_later());
                    }
                    Some(Command::Rebind) => self.rebind().await,
                    Some(Command::Close) | None => {
                        self.media.rtc.disconnect();
                        self.media.drain(&self.events);
                        if let Some(mut open) = socket.take() {
                            let _ = open.close(None).await;
                        }
                        if let Some(pending) = reconnect.take() {
                            pending.abort();
                        }
                        self.finish(None);
                        return;
                    }
                },
            }
            self.media.drain(&self.events);
        }
    }

    async fn rebind(&mut self) {
        if let Err(error) = self.media.rebind().await {
            tracing::debug!(%error, "could not move media to the new network yet");
        }
    }

    fn on_envelope(&mut self, envelope: Box<voice::Envelope>) {
        if envelope.seq > 0 {
            self.last_seq = self.last_seq.max(envelope.seq);
        }
        match envelope.payload {
            Some(Payload::HeartbeatAck(_)) => self.awaiting_ack = false,
            Some(Payload::ClientConnect(connect)) => {
                self.media.hear(connect.user_id, connect.audio_ssrc);
                let _ = self.events.send(VoiceEvent::ClientConnected {
                    user_id: connect.user_id,
                    audio_ssrc: connect.audio_ssrc,
                });
            }
            Some(Payload::ClientDisconnect(gone)) => {
                self.media.forget(gone.user_id);
                let _ = self.events.send(VoiceEvent::ClientDisconnected {
                    user_id: gone.user_id,
                });
            }
            Some(Payload::Speaking(speaking)) => {
                let _ = self.events.send(VoiceEvent::Speaking {
                    user_id: speaking.user_id,
                    flags: speaking.flags,
                });
            }
            Some(Payload::Resumed(_)) => {
                let _ = self.events.send(VoiceEvent::Resumed);
            }
            _ => {}
        }
    }

    /// Opens the gateway again and resumes, in the background so media
    /// keeps flowing meanwhile.
    fn resume_later(&self) -> Reconnect {
        let url = self.url.clone();
        let fingerprint = self.fingerprint;
        let resume = voice::Resume {
            voice_session_id: self.voice_session_id.clone(),
            resume_token: self.resume_token.clone(),
            last_seq: self.last_seq,
        };
        tokio::spawn(async move {
            let give_up = Instant::now() + RESUME_FOR;
            loop {
                match resume_once(&url, fingerprint, resume.clone()).await {
                    Ok(socket) => return Ok(socket),
                    Err(Some(code)) if !close::is_resumable(Some(code)) => return Err(Some(code)),
                    Err(_) if Instant::now() >= give_up => return Err(None),
                    Err(_) => tokio::time::sleep(RESUME_PAUSE).await,
                }
            }
        })
    }

    fn finish(&self, code: Option<u16>) {
        let _ = self.events.send(VoiceEvent::Closed { code });
    }
}

async fn resume_once(
    url: &GatewayUrl,
    fingerprint: Option<[u8; 32]>,
    resume: voice::Resume,
) -> Result<Socket, Option<u16>> {
    let mut socket = gateway::open(url, fingerprint).await.map_err(|_| None)?;
    hello(&mut socket).await.map_err(|error| match error {
        TransportError::Refused(code) => code,
        _ => None,
    })?;
    gateway::send(&mut socket, Payload::Resume(resume))
        .await
        .map_err(|_| None)?;
    Ok(socket)
}

async fn next_or_pending(socket: Option<&mut Socket>) -> Next {
    match socket {
        Some(socket) => gateway::next(socket).await,
        None => std::future::pending().await,
    }
}

async fn join_or_pending(
    handle: Option<&mut Reconnect>,
) -> Result<Result<Socket, Option<u16>>, tokio::task::JoinError> {
    match handle {
        Some(handle) => handle.await,
        None => std::future::pending().await,
    }
}

fn random<const N: usize>() -> [u8; N] {
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
