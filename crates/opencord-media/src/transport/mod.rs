//! The voice transport (Phase 2 plan §7.1, §7.14): the voice gateway
//! WebSocket for signalling, and one str0m connection over UDP for media,
//! in RTP mode. It moves Opus packets and H.264 frames; encoding, decoding
//! and mixing are the engines'.
//!
//! A dropped gateway is resumed while media keeps flowing; only a refused
//! resume or a close the node means ends the connection.

use std::sync::Arc;
use std::time::{Duration, Instant};

use opencord_common::voice::OPUS_PAYLOAD_TYPE;
use opencord_proto::voice::v1 as voice;
use tokio::sync::{mpsc, oneshot};
use voice::envelope::Payload;

use driver::Driver;
use gateway::{GatewayUrl, Next, Socket};
use media::Media;
use transform::{FrameTransform, Identity};

mod assembler;
mod driver;
pub mod gateway;
mod h264;
#[cfg(any(test, feature = "testing"))]
pub mod impairment;
mod media;
pub mod transform;

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

/// How a connection treats frames; the defaults suit the app.
#[derive(Default)]
pub struct ConnectOptions {
    /// Every encoded frame passes through it both ways (plan §13); the
    /// identity when `None`.
    pub transform: Option<Box<dyn FrameTransform>>,
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

/// What a video track shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    Camera,
    Screen,
}

/// One simulcast layer of a video track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layer {
    /// "l", "m" or "h".
    pub rid: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// Bits per second.
    pub max_bitrate: u32,
}

/// A video track to publish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackRequest {
    /// Unique in the channel; pick it at random.
    pub track_id: String,
    pub kind: TrackKind,
    pub layers: Vec<Layer>,
}

/// Why the node refused a track.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TrackError {
    #[error("the voice gateway is not connected")]
    NotConnected,
    #[error("the node did not answer")]
    NoAnswer,
    /// `reason` is an `opencord.v1.ErrorCode`.
    #[error("the node refused the track: {message}")]
    Refused { reason: i32, message: String },
}

/// One encoded picture of one layer, as NAL units without start codes.
#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub track_id: String,
    /// 0 for the lowest layer.
    pub layer: u8,
    pub keyframe: bool,
    pub width: u16,
    pub height: u16,
    /// When the picture was captured; every layer of it shares this.
    pub captured: Instant,
    pub nal_units: Vec<Vec<u8>>,
}

/// Someone's picture, put back together.
#[derive(Debug, Clone)]
pub struct ReceivedVideo {
    pub user_id: i64,
    pub track_id: String,
    pub layer: u8,
    pub keyframe: bool,
    /// Width and height, from a keyframe.
    pub size: Option<(u16, u16)>,
    /// 90 kHz.
    pub timestamp: u32,
    pub nal_units: Vec<Vec<u8>>,
    pub arrived: Instant,
}

/// Someone else's video track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteTrack {
    pub track_id: String,
    pub kind: TrackKind,
    pub layers: Vec<Layer>,
    /// The layers its sender produces now, once the node has said.
    pub available: Option<Vec<String>>,
}

/// A video this client wants, at the height of its tile (plan §6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SinkWant {
    pub track_id: String,
    pub max_height: u32,
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
    /// Someone's track as it is now: just published, or its layers
    /// changed. On joining, one for each track already there.
    Track {
        user_id: i64,
        track: RemoteTrack,
    },
    TrackRemoved {
        user_id: i64,
        track_id: String,
    },
    Video(ReceivedVideo),
    /// The node stopped one of this client's tracks; `reason` is an
    /// `opencord.v1.ErrorCode`.
    TrackStopped {
        track_id: String,
        reason: i32,
        message: String,
    },
    /// The layers of one of this client's tracks anyone needs; the rest
    /// need not be encoded.
    LayerWants {
        track_id: String,
        rids: Vec<String>,
    },
    /// Someone lost a picture: make the next frame of that layer a
    /// keyframe.
    KeyframeRequested {
        track_id: String,
        layer: u8,
    },
    /// What the uplink can carry, in bits per second.
    UplinkEstimate(u64),
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
    Video(VideoFrame),
    Speaking(u32),
    Publish {
        request: TrackRequest,
        reply: oneshot::Sender<Result<(), TrackError>>,
    },
    Unpublish(String),
    SinkWants(Vec<SinkWant>),
    #[cfg(any(test, feature = "testing"))]
    Impair {
        inbound: impairment::Impairment,
        outbound: impairment::Impairment,
    },
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
        Self::connect_with(target, ConnectOptions::default()).await
    }

    pub async fn connect_with(
        target: VoiceTarget,
        options: ConnectOptions,
    ) -> Result<(Self, mpsc::UnboundedReceiver<VoiceEvent>), TransportError> {
        let url = GatewayUrl::parse(&target.gateway_url)?;
        let mut socket = gateway::open(&url, target.certificate_fingerprint).await?;
        let heartbeat = hello(&mut socket).await?;
        let support = |codec: voice::Codec| voice::CodecSupport {
            codec: codec as i32,
            hardware_encode: false,
            hardware_decode: false,
        };
        gateway::send(
            &mut socket,
            Payload::Identify(voice::Identify {
                token: target.token.clone(),
                user_id: target.user_id,
                session_id: target.session_id.clone(),
                channel_id: target.channel_id,
                client_caps: Some(voice::ClientCaps {
                    codecs: vec![support(voice::Codec::Opus), support(voice::Codec::H264)],
                    max_decode_height: 0,
                    simulcast: true,
                }),
                max_e2ee_version: 0,
            }),
        )
        .await?;
        let ready = ready(&mut socket).await?;
        if !opus_offered(&ready) {
            return Err(TransportError::Protocol(
                "the node offers no Opus this client understands".to_owned(),
            ));
        }
        let transform = options.transform.unwrap_or_else(|| Box::new(Identity));
        let media = Media::start(&url.address.host, &ready, transform).await?;
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
            for track in &participant.tracks {
                if let Some(track) = media::remote_track(track, None) {
                    let _ = events.send(VoiceEvent::Track {
                        user_id: participant.user_id,
                        track,
                    });
                }
            }
        }
        let (commands, command_receiver) = mpsc::unbounded_channel();
        let audio_ssrc = ready.audio_ssrc;
        let limits = ready.limits;
        let driver = Driver::new(
            url,
            target.certificate_fingerprint,
            &ready,
            heartbeat,
            media,
            events,
        );
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

    /// The limits the node gave: camera cap, screen share maximum.
    pub fn limits(&self) -> Option<voice::Limits> {
        self.limits
    }

    pub fn send_audio(&self, frame: AudioFrame) {
        let _ = self.commands.send(Command::Audio(frame));
    }

    /// `flags` from [`opencord_common::voice::speaking`].
    pub fn set_speaking(&self, flags: u32) {
        let _ = self.commands.send(Command::Speaking(flags));
    }

    /// Publishes a video track; resolves once the node accepts or refuses
    /// it. Frames go out with [`Self::send_video`] after that.
    pub async fn publish_track(&self, request: TrackRequest) -> Result<(), TrackError> {
        let (reply, answer) = oneshot::channel();
        self.commands
            .send(Command::Publish { request, reply })
            .map_err(|_| TrackError::NotConnected)?;
        answer.await.map_err(|_| TrackError::NoAnswer)?
    }

    pub fn unpublish_track(&self, track_id: &str) {
        let _ = self.commands.send(Command::Unpublish(track_id.to_owned()));
    }

    pub fn send_video(&self, frame: VideoFrame) {
        let _ = self.commands.send(Command::Video(frame));
    }

    /// The videos this client wants and at what size; the rest are not
    /// sent to it (plan §6).
    pub fn set_sink_wants(&self, wants: Vec<SinkWant>) {
        let _ = self.commands.send(Command::SinkWants(wants));
    }

    /// Impairs this connection's packets, for tests (plan §15).
    #[cfg(any(test, feature = "testing"))]
    pub fn set_impairment(
        &self,
        inbound: impairment::Impairment,
        outbound: impairment::Impairment,
    ) {
        let _ = self.commands.send(Command::Impair { inbound, outbound });
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

/// Whether the node's Opus is the one this client speaks.
fn opus_offered(ready: &voice::Ready) -> bool {
    ready.codecs.iter().any(|codec| {
        codec.codec == voice::Codec::Opus as i32
            && codec.payload_type == u32::from(OPUS_PAYLOAD_TYPE)
    })
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
