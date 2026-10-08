//! The client's str0m connection and its UDP socket: Opus packets and
//! H.264 frames out, the same back in, encoded frames passing through the
//! frame transform both ways (plan §13).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use opencord_common::video::{FrameMarking, H264_PAYLOAD_TYPE, layer_index, video_mid};
use opencord_common::voice::{AUDIO_MID, OPUS_PAYLOAD_TYPE, receive_mid, rtc_config};
use opencord_proto::voice::v1 as voice;
use str0m::bwe::{Bitrate, BweKind};
use str0m::config::Fingerprint;
use str0m::ice::IceCreds;
use str0m::media::{KeyframeRequestKind, MediaKind, Mid, Pt};
use str0m::net::{Protocol, Receive};
use str0m::rtp::{ExtensionValues, RtpPacket, RtpWrite, Ssrc};
use str0m::{Candidate, Event, IceConnectionState, Input, Output, Rtc};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

use super::assembler::{Assembled, FrameAssembler, VideoPacket};
use super::h264::packetize;
use super::transform::FrameTransform;
use super::{
    AudioFrame, Layer, ReceivedAudio, ReceivedVideo, RemoteTrack, TrackKind, TransportError,
    VideoFrame, VoiceEvent, random,
};

/// The most payload bytes in a video packet: room, under str0m's
/// 1150-byte datagram target, for the RTP header, its extensions, SRTP's
/// tag and a retransmission's sequence number.
const MAX_VIDEO_PAYLOAD: usize = 1080;
/// Where the uplink estimate starts.
const INITIAL_UPLINK: u64 = 1_000_000;
/// A stream still waiting for a keyframe asks again this often.
const KEYFRAME_AGAIN: Duration = Duration::from_secs(1);

/// The client's str0m connection and its UDP socket.
pub(super) struct Media {
    pub rtc: Rtc,
    pub socket: UdpSocket,
    local_address: SocketAddr,
    remote_address: SocketAddr,
    pub local: IceCreds,
    pub fingerprint: Vec<u8>,
    audio_ssrc: u32,
    next_seq: u64,
    /// The RTP timestamp of capture position 0.
    timestamp_base: u32,
    users_by_ssrc: HashMap<u32, i64>,
    pub timeout: Instant,
    /// Sending failed in a way that means the network went away.
    network_lost: bool,
    transform: Box<dyn FrameTransform>,
    /// Tracks this client publishes.
    outgoing: Vec<Outgoing>,
    /// Others' tracks, by the SSRC they arrive on.
    incoming: HashMap<u32, Incoming>,
    /// The 90 kHz video clock: its value when the connection started.
    video_base: u32,
    video_start: Instant,
    #[cfg(any(test, feature = "testing"))]
    shims: Option<Shims>,
}

struct Outgoing {
    track_id: String,
    mid: String,
    /// In the node's order: lowest first.
    layers: Vec<OutLayer>,
}

struct OutLayer {
    rid: String,
    ssrc: u32,
    next_seq: u64,
}

struct Incoming {
    user_id: i64,
    track_id: String,
    assembler: FrameAssembler,
    keyframe_asked: Option<Instant>,
}

#[cfg(any(test, feature = "testing"))]
struct Shims {
    inbound: super::impairment::Shim,
    outbound: super::impairment::Shim,
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

/// A track as the node describes it, if this client can show it.
pub(super) fn remote_track(
    track: &voice::Track,
    available: Option<Vec<String>>,
) -> Option<RemoteTrack> {
    let kind = match voice::TrackKind::try_from(track.kind) {
        Ok(voice::TrackKind::Camera) => TrackKind::Camera,
        Ok(voice::TrackKind::Screen) => TrackKind::Screen,
        _ => return None,
    };
    Some(RemoteTrack {
        track_id: track.track_id.clone(),
        kind,
        layers: track
            .layers
            .iter()
            .map(|layer| Layer {
                rid: layer.rid.clone(),
                width: layer.width,
                height: layer.height,
                fps: layer.fps,
                max_bitrate: layer.max_bitrate,
            })
            .collect(),
        available,
    })
}

impl Media {
    /// Media goes to the node's candidate, or to `gateway_host` when the
    /// candidate leaves its address empty ("the host you reached the voice
    /// gateway with").
    pub async fn start(
        gateway_host: &str,
        ready: &voice::Ready,
        transform: Box<dyn FrameTransform>,
    ) -> Result<Self, TransportError> {
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
        let mut rtc = rtc_config()
            .set_local_ice_credentials(local.clone())
            .enable_bwe(Some(Bitrate::bps(INITIAL_UPLINK)))
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
            api.enable_twcc_feedback();
            api.declare_media(AUDIO_MID.into(), MediaKind::Audio);
            api.declare_stream_tx(Ssrc::from(ready.audio_ssrc), None, AUDIO_MID.into(), None);
            for participant in &ready.participants {
                hear(&mut api, participant.audio_ssrc);
            }
            api.start_dtls(true)
                .map_err(|error| TransportError::Connect(error.to_string()))?;
            api.local_dtls_fingerprint().bytes.clone()
        };
        let mut media = Self {
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
            transform,
            outgoing: Vec::new(),
            incoming: HashMap::new(),
            video_base: u32::from_ne_bytes(random::<4>()),
            video_start: now,
            #[cfg(any(test, feature = "testing"))]
            shims: None,
        };
        for participant in &ready.participants {
            for track in &participant.tracks {
                media.watch(participant.user_id, track);
            }
        }
        Ok(media)
    }

    pub fn hear(&mut self, user_id: i64, ssrc: u32) {
        self.users_by_ssrc.insert(ssrc, user_id);
        hear(&mut self.rtc.direct_api(), ssrc);
    }

    /// Someone left: their audio and video go.
    pub fn forget(&mut self, user_id: i64) {
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
        let tracks: Vec<u32> = self
            .incoming
            .iter()
            .filter(|(_, incoming)| incoming.user_id == user_id)
            .map(|(ssrc, _)| *ssrc)
            .collect();
        for ssrc in tracks {
            self.incoming.remove(&ssrc);
            self.rtc
                .direct_api()
                .remove_media(receive_mid(ssrc).as_str().into());
        }
    }

    /// Gets ready to receive someone's track.
    pub fn watch(&mut self, user_id: i64, track: &voice::Track) {
        if track.ssrc == 0 || self.incoming.contains_key(&track.ssrc) {
            return;
        }
        let mid = receive_mid(track.ssrc);
        let rtx = (track.rtx_ssrc != 0).then(|| Ssrc::from(track.rtx_ssrc));
        let mut api = self.rtc.direct_api();
        api.declare_media(mid.as_str().into(), MediaKind::Video);
        api.expect_stream_rx(Ssrc::from(track.ssrc), rtx, mid.as_str().into(), None);
        self.incoming.insert(
            track.ssrc,
            Incoming {
                user_id,
                track_id: track.track_id.clone(),
                assembler: FrameAssembler::default(),
                keyframe_asked: None,
            },
        );
    }

    pub fn unwatch(&mut self, user_id: i64, track_id: &str) {
        let found = self
            .incoming
            .iter()
            .find(|(_, incoming)| incoming.user_id == user_id && incoming.track_id == track_id)
            .map(|(ssrc, _)| *ssrc);
        if let Some(ssrc) = found {
            self.incoming.remove(&ssrc);
            self.rtc
                .direct_api()
                .remove_media(receive_mid(ssrc).as_str().into());
        }
    }

    /// Starts sending a track the node accepted, on the SSRCs it gave.
    pub fn publish(&mut self, track_id: &str, layers: &[voice::LayerSsrc]) {
        let Some(first) = layers.first() else {
            return;
        };
        let mid = video_mid(first.ssrc);
        let mut api = self.rtc.direct_api();
        api.declare_media(mid.as_str().into(), MediaKind::Video);
        for layer in layers {
            let rtx = (layer.rtx_ssrc != 0).then(|| Ssrc::from(layer.rtx_ssrc));
            api.declare_stream_tx(
                Ssrc::from(layer.ssrc),
                rtx,
                mid.as_str().into(),
                Some(layer.rid.as_str().into()),
            );
        }
        self.outgoing.push(Outgoing {
            track_id: track_id.to_owned(),
            mid,
            layers: layers
                .iter()
                .map(|layer| OutLayer {
                    rid: layer.rid.clone(),
                    ssrc: layer.ssrc,
                    next_seq: u64::from(u16::from_ne_bytes(random::<2>())),
                })
                .collect(),
        });
    }

    pub fn unpublish(&mut self, track_id: &str) {
        let Some(index) = self
            .outgoing
            .iter()
            .position(|track| track.track_id == track_id)
        else {
            return;
        };
        let track = self.outgoing.remove(index);
        self.rtc
            .direct_api()
            .remove_media(track.mid.as_str().into());
    }

    /// Whether the network this connection was on is gone: sending failed,
    /// or the system now reaches the node from another address.
    pub fn network_changed(&mut self) -> bool {
        std::mem::take(&mut self.network_lost)
            || route_to(self.remote_address).is_some_and(|ip| ip != self.local_address.ip())
    }

    /// Moves media to a new socket after a network change: its address
    /// becomes a new local candidate and the old one is dropped, so ICE
    /// checks the new path and moves over (the node learns the new address
    /// from the checks). DTLS and the voice session carry on.
    pub async fn rebind(&mut self) -> Result<(), TransportError> {
        let (socket, local_address) = connected_socket(self.remote_address).await?;
        let new = Candidate::host(local_address, "udp").map_err(candidate_error)?;
        let old = Candidate::host(self.local_address, "udp").map_err(candidate_error)?;
        self.rtc.add_local_candidate(new);
        self.rtc.direct_api().invalidate_candidate(&old);
        self.socket = socket;
        self.local_address = local_address;
        Ok(())
    }

    pub fn send_audio(&mut self, frame: AudioFrame) {
        self.timeout();
        let seq = self.next_seq;
        self.next_seq += 1;
        // RTP timestamps are the capture clock, wrapped to 32 bits.
        let timestamp = self.timestamp_base.wrapping_add(frame.position as u32);
        let payload = self.transform.outbound_audio(frame.payload);
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
                payload,
            )
            .marker(frame.marker)
            .ext_vals(ExtensionValues {
                audio_level: Some(frame.audio_level),
                voice_activity: Some(frame.voice_activity),
                ..ExtensionValues::default()
            }),
        );
    }

    /// Cuts a picture into packets on its layer's stream.
    pub fn send_video(&mut self, frame: VideoFrame) {
        // str0m stamps packets with the time it last saw; the bandwidth
        // estimator needs the real send time.
        self.timeout();
        let Some(track) = self
            .outgoing
            .iter_mut()
            .find(|track| track.track_id == frame.track_id)
        else {
            return;
        };
        let Some(layer) = track
            .layers
            .iter_mut()
            .find(|layer| layer_index(&layer.rid) == Some(frame.layer))
        else {
            return;
        };
        let elapsed = frame.captured.saturating_duration_since(self.video_start);
        let ticks = u32::try_from(elapsed.as_micros() * 9 / 100).unwrap_or(u32::MAX);
        let timestamp = self.video_base.wrapping_add(ticks);
        let nal_units = self.transform.outbound_video(frame.nal_units);
        let packets = packetize(&nal_units, MAX_VIDEO_PAYLOAD);
        let count = packets.len();
        let mut api = self.rtc.direct_api();
        let Some(stream) = api.stream_tx(&Ssrc::from(layer.ssrc)) else {
            return;
        };
        for (index, payload) in packets.into_iter().enumerate() {
            let seq = layer.next_seq;
            layer.next_seq += 1;
            let first = index == 0;
            let mut ext_vals = ExtensionValues::default();
            ext_vals.user_values.set(FrameMarking {
                keyframe: frame.keyframe,
                start: first,
                layer: frame.layer,
                size: (frame.keyframe && first).then_some((frame.width, frame.height)),
            });
            stream.write_rtp(
                RtpWrite::new(
                    Pt::from(H264_PAYLOAD_TYPE),
                    seq.into(),
                    timestamp,
                    frame.captured,
                    payload,
                )
                .marker(index + 1 == count)
                .nackable(true)
                .ext_vals(ext_vals),
            );
        }
    }

    /// A datagram read at `at`. The bandwidth estimator needs arrival
    /// times as they were, not when the packet got its turn.
    pub fn receive(
        &mut self,
        at: Instant,
        data: &[u8],
        events: &mpsc::UnboundedSender<VoiceEvent>,
    ) {
        #[cfg(any(test, feature = "testing"))]
        if let Some(shims) = self.shims.as_mut() {
            shims.inbound.push(at, data.to_vec());
            return;
        }
        self.handle_datagram(at, data);
        // In RTP mode str0m holds one received packet until polled.
        self.poll(events);
    }

    fn handle_datagram(&mut self, at: Instant, data: &[u8]) {
        let Ok(contents) = data.try_into() else {
            return;
        };
        let input = Input::Receive(
            at,
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

    pub fn timeout(&mut self) {
        if let Err(error) = self.rtc.handle_input(Input::Timeout(Instant::now())) {
            tracing::debug!(%error, "a media timeout failed");
        }
    }

    /// Sends what str0m wants sent and reports what happened.
    pub fn drain(&mut self, events: &mpsc::UnboundedSender<VoiceEvent>) {
        loop {
            if !self.poll(events) {
                return;
            }
            // Frames that came together, and losses that need a keyframe;
            // asking for one gives str0m more to send.
            if !self.collect_frames(events) {
                return;
            }
        }
    }

    /// Everything str0m has now; false when the connection failed.
    fn poll(&mut self, events: &mpsc::UnboundedSender<VoiceEvent>) -> bool {
        loop {
            match self.rtc.poll_output() {
                Ok(Output::Timeout(at)) => {
                    self.timeout = at;
                    return true;
                }
                Ok(Output::Transmit(transmit)) => self.transmit(&transmit.contents),
                Ok(Output::Event(Event::Connected)) => {
                    let _ = events.send(VoiceEvent::MediaConnected);
                }
                Ok(Output::Event(Event::IceConnectionStateChange(
                    IceConnectionState::Disconnected,
                ))) => {
                    let _ = events.send(VoiceEvent::MediaDisconnected);
                }
                Ok(Output::Event(Event::RtpPacket(packet))) => self.on_rtp(packet, events),
                Ok(Output::Event(Event::KeyframeRequest(request))) => {
                    self.on_keyframe_request(&request.mid, request.rid.as_deref(), events);
                }
                Ok(Output::Event(Event::EgressBitrateEstimate(BweKind::Twcc {
                    estimate, ..
                }))) => {
                    let _ = events.send(VoiceEvent::UplinkEstimate(estimate.as_u64()));
                }
                Ok(Output::Event(_)) => {}
                Err(error) => {
                    tracing::debug!(%error, "the media connection failed");
                    let _ = events.send(VoiceEvent::MediaDisconnected);
                    self.timeout = Instant::now() + Duration::from_secs(3600);
                    return false;
                }
            }
        }
    }

    fn transmit(&mut self, contents: &[u8]) {
        #[cfg(any(test, feature = "testing"))]
        if let Some(shims) = self.shims.as_mut() {
            shims.outbound.push(Instant::now(), contents.to_vec());
            return;
        }
        self.send_datagram(contents);
    }

    fn send_datagram(&mut self, contents: &[u8]) {
        if let Err(error) = self.socket.try_send(contents) {
            // A full buffer drops the packet, as the network would;
            // anything else means the network is gone.
            if error.kind() != std::io::ErrorKind::WouldBlock {
                self.network_lost = true;
            }
        }
    }

    fn on_rtp(&mut self, packet: RtpPacket, events: &mpsc::UnboundedSender<VoiceEvent>) {
        let ssrc = *packet.header.ssrc;
        if *packet.header.payload_type == H264_PAYLOAD_TYPE {
            if let Some(incoming) = self.incoming.get_mut(&ssrc) {
                incoming.assembler.push(VideoPacket {
                    seq: *packet.seq_no,
                    timestamp: packet.header.timestamp,
                    marker: packet.header.marker,
                    marking: packet.header.ext_vals.user_values.get().copied(),
                    payload: packet.payload,
                    arrived: packet.timestamp,
                });
            }
            return;
        }
        let Some(user_id) = self.users_by_ssrc.get(&ssrc).copied() else {
            return;
        };
        let Some(payload) = self.transform.inbound_audio(user_id, packet.payload) else {
            return;
        };
        let _ = events.send(VoiceEvent::Audio(ReceivedAudio {
            user_id,
            ssrc,
            seq: *packet.seq_no,
            timestamp: packet.header.timestamp,
            marker: packet.header.marker,
            payload,
            audio_level: packet.header.ext_vals.audio_level,
            arrived: packet.timestamp,
        }));
    }

    fn on_keyframe_request(
        &self,
        mid: &Mid,
        rid: Option<&str>,
        events: &mpsc::UnboundedSender<VoiceEvent>,
    ) {
        let Some(track) = self
            .outgoing
            .iter()
            .find(|track| Mid::from(track.mid.as_str()) == *mid)
        else {
            return;
        };
        let Some(layer) = rid.and_then(layer_index) else {
            return;
        };
        let _ = events.send(VoiceEvent::KeyframeRequested {
            track_id: track.track_id.clone(),
            layer,
        });
    }

    /// Hands over finished frames; asks for keyframes where pictures were
    /// lost. Returns whether it asked for any.
    fn collect_frames(&mut self, events: &mpsc::UnboundedSender<VoiceEvent>) -> bool {
        let now = Instant::now();
        let Self {
            incoming,
            transform,
            rtc,
            ..
        } = self;
        let mut asked = false;
        for (ssrc, stream) in incoming.iter_mut() {
            let mut lost = false;
            while let Some(assembled) = stream.assembler.poll(now) {
                match assembled {
                    Assembled::Frame(frame) => {
                        let Some(nal_units) =
                            transform.inbound_video(stream.user_id, frame.nal_units)
                        else {
                            continue;
                        };
                        let _ = events.send(VoiceEvent::Video(ReceivedVideo {
                            user_id: stream.user_id,
                            track_id: stream.track_id.clone(),
                            layer: frame.layer,
                            keyframe: frame.keyframe,
                            size: frame.size,
                            timestamp: frame.timestamp,
                            nal_units,
                            arrived: frame.arrived,
                        }));
                    }
                    Assembled::Lost => lost = true,
                }
            }
            let waiting = stream.assembler.needs_keyframe() && stream.assembler.len() > 0;
            let allowed = stream
                .keyframe_asked
                .is_none_or(|at| now.saturating_duration_since(at) >= KEYFRAME_AGAIN);
            if (lost || waiting)
                && allowed
                && let Some(rx) = rtc.direct_api().stream_rx(&Ssrc::from(*ssrc))
            {
                rx.request_keyframe(KeyframeRequestKind::Pli);
                stream.keyframe_asked = Some(now);
                asked = true;
            }
        }
        asked
    }

    /// Impairs packets both ways from now (plan §15).
    #[cfg(any(test, feature = "testing"))]
    pub fn set_impairment(
        &mut self,
        inbound: &super::impairment::Impairment,
        outbound: &super::impairment::Impairment,
    ) {
        use super::impairment::{Impairment, Shim};
        if *inbound == Impairment::default() && *outbound == Impairment::default() {
            // Straight through again. What the shims still hold is lost, as
            // packets in flight on a link that went away would be.
            self.shims = None;
            return;
        }
        match self.shims.as_mut() {
            Some(shims) => {
                shims.inbound.set(inbound);
                shims.outbound.set(outbound);
            }
            None => {
                let seed = u64::from_ne_bytes(random::<8>());
                self.shims = Some(Shims {
                    inbound: Shim::new(inbound, seed),
                    outbound: Shim::new(outbound, seed.wrapping_add(1)),
                });
            }
        }
    }

    /// When impaired packets are next due.
    pub fn shims_due(&self) -> Option<Instant> {
        #[cfg(any(test, feature = "testing"))]
        if let Some(shims) = &self.shims {
            return match (shims.inbound.next_due(), shims.outbound.next_due()) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
        }
        None
    }

    /// Lets impaired packets through once they are due.
    pub fn release_shims(&mut self, events: &mpsc::UnboundedSender<VoiceEvent>) {
        #[cfg(any(test, feature = "testing"))]
        {
            let now = Instant::now();
            let (mut inbound, mut outbound) = (Vec::new(), Vec::new());
            if let Some(shims) = self.shims.as_mut() {
                while let Some(datagram) = shims.inbound.pop(now) {
                    inbound.push(datagram);
                }
                while let Some(datagram) = shims.outbound.pop(now) {
                    outbound.push(datagram);
                }
            }
            for datagram in inbound {
                self.handle_datagram(now, &datagram);
                self.poll(events);
            }
            for datagram in outbound {
                self.send_datagram(&datagram);
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
