//! The selective forwarding unit (Phase 2 plan §6), Sans-IO: it never
//! touches a socket or a clock. The node feeds it packets and timeouts and
//! sends what it hands back. One str0m `Rtc` per participant, in RTP mode:
//! packets are forwarded, never decoded.
//!
//! Each participant sends its microphone on [`AUDIO_MID`] with the SSRC the
//! node gave it, unique in the channel. Every other participant receives it
//! on its own media, [`receive_mid`] of that SSRC, so a receiver tells
//! speakers apart by SSRC. Video is in [`video`].

use std::collections::{HashMap, HashSet, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use hmac::{Hmac, KeyInit, Mac};
use opencord_common::voice::{AUDIO_MID, receive_mid, rtc_config};
use sha1::Sha1;
use str0m::bwe::{Bitrate, BweKind};
use str0m::config::{DtlsCert, Fingerprint};
use str0m::crypto::CryptoProvider;
use str0m::ice::{IceCreds, StunMessage};
use str0m::media::{MediaKind, Mid};
use str0m::net::{Protocol, Receive};
use str0m::rtp::{RtpPacket, Ssrc};
use str0m::{Candidate, Event, IceConnectionState, Input, Output, Rtc};

pub use opencord_common::video::VideoKind;
pub use video::{LayerSetup, TrackSetup, Want};

mod allocation;
mod audio;
mod meter;
mod video;

use meter::{RateMeter, TokenBucket};

/// A participant on this node.
pub type PeerId = u64;

/// What decides a participant's audio, from the main server.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PeerState {
    pub self_mute: bool,
    pub self_deaf: bool,
    pub server_mute: bool,
    pub server_deaf: bool,
    /// No Speak permission, or the AFK channel.
    pub suppress: bool,
}

impl PeerState {
    /// Whether what they say is forwarded.
    pub fn speaks(self) -> bool {
        !(self.self_mute || self.server_mute || self.suppress)
    }

    /// Whether others' audio is sent to them.
    pub fn hears(self) -> bool {
        !(self.self_deaf || self.server_deaf)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerSetup {
    pub user_id: i64,
    pub channel_id: i64,
    /// Unique in the channel.
    pub audio_ssrc: u32,
    pub state: PeerState,
}

/// One side of the transport, exchanged over the voice gateway.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transport {
    pub ice_ufrag: String,
    pub ice_pwd: String,
    /// SHA-256 of the DTLS certificate.
    pub dtls_fingerprint: Vec<u8>,
}

/// A datagram to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transmit {
    pub destination: SocketAddr,
    pub contents: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SfuEvent {
    /// ICE and DTLS are up: media flows.
    Connected(PeerId),
    /// The media connection was lost.
    Disconnected(PeerId),
    /// The layers of a track anyone needs; the sender stops encoding the
    /// rest (plan §6, `SenderLayerWants`).
    LayerWants {
        peer: PeerId,
        track_id: String,
        rids: Vec<String>,
    },
    /// The layers a track's sender is producing changed.
    LayersAvailable {
        peer: PeerId,
        track_id: String,
        rids: Vec<String>,
    },
    /// A track sent far over its ceiling for too long and was stopped
    /// (plan §6, quality enforcement).
    TrackStopped { peer: PeerId, track_id: String },
}

#[derive(Debug, thiserror::Error)]
pub enum SfuError {
    #[error("no such participant")]
    UnknownPeer,
    #[error("the participant already publishes a track with that id")]
    TrackExists,
    #[error("a track needs at least one layer")]
    NoLayers,
    #[error("the crypto backend could not make a DTLS certificate")]
    NoCertificate,
    #[error("the node's address cannot be an ICE candidate: {0}")]
    Candidate(String),
    #[error(transparent)]
    Rtc(#[from] str0m::RtcError),
}

pub struct Sfu {
    /// What str0m is told each packet arrived at, per address family: the
    /// node binds the unspecified address, and the kernel routes.
    candidate_v4: SocketAddr,
    candidate_v6: SocketAddr,
    crypto: Arc<CryptoProvider>,
    /// One certificate for every participant, so the node's fingerprint is
    /// the same for all.
    dtls_cert: DtlsCert,
    next_id: PeerId,
    peers: HashMap<PeerId, Peer>,
    by_source: HashMap<SocketAddr, PeerId>,
    channels: HashMap<i64, Channel>,
    transmits: VecDeque<Transmit>,
    events: VecDeque<SfuEvent>,
    /// Keyframe requests receivers sent, by the media they are for.
    keyframe_requests: Vec<(PeerId, Mid)>,
    /// When video layers are next chosen; far off without video.
    allocate_at: Instant,
}

struct Peer {
    rtc: Rtc,
    setup: PeerSetup,
    /// Signs this peer's connectivity checks.
    ice_password: String,
    /// Where the client last nominated a path from; media goes there.
    address: Option<SocketAddr>,
    timeout: Instant,
    /// Running loudness in -dBov, 0 loudest.
    level: f32,
    /// Per sender: packets not sent to this peer since the last one was,
    /// and how far the sequence numbers it sees lag the sender's.
    forwards: HashMap<PeerId, Forward>,
    /// Video it publishes.
    tracks: Vec<video::Track>,
    /// Others' video it can receive.
    sending: Vec<video::Sending>,
    /// Its downlink, from the bandwidth estimator, in bits per second.
    estimate: u64,
    /// Audio sent to it.
    audio_out: RateMeter,
    /// Media it may send (plan §14).
    inbound: TokenBucket,
}

#[derive(Debug, Default, Clone, Copy)]
struct Forward {
    skipped: u64,
    lag: u64,
}

#[derive(Default)]
struct Channel {
    peers: Vec<PeerId>,
    loudest: HashSet<PeerId>,
    loudest_at: Option<Instant>,
}

impl Sfu {
    pub fn new(candidate_v4: SocketAddr, candidate_v6: SocketAddr) -> Result<Self, SfuError> {
        let crypto = Arc::new(str0m::crypto::from_feature_flags());
        let dtls_cert = crypto
            .dtls_provider
            .generate_certificate()
            .ok_or(SfuError::NoCertificate)?;
        Ok(Self {
            candidate_v4,
            candidate_v6,
            crypto,
            dtls_cert,
            next_id: 1,
            peers: HashMap::new(),
            by_source: HashMap::new(),
            channels: HashMap::new(),
            transmits: VecDeque::new(),
            events: VecDeque::new(),
            keyframe_requests: Vec::new(),
            allocate_at: far_future(Instant::now()),
        })
    }

    /// Adds a participant; it still needs [`Self::start_peer`] with the
    /// client's half of the transport.
    pub fn add_peer(
        &mut self,
        now: Instant,
        setup: PeerSetup,
    ) -> Result<(PeerId, Transport), SfuError> {
        let credentials = IceCreds::new();
        let mut rtc = rtc_config()
            .set_ice_lite(true)
            .set_local_ice_credentials(credentials.clone())
            .set_crypto_provider(Arc::clone(&self.crypto))
            .set_dtls_cert(self.dtls_cert.clone())
            .enable_bwe(Some(Bitrate::bps(video::INITIAL_ESTIMATE)))
            .build(now);
        for address in [self.candidate_v4, self.candidate_v6] {
            let candidate = Candidate::host(address, "udp")
                .map_err(|error| SfuError::Candidate(error.to_string()))?;
            rtc.add_local_candidate(candidate);
        }
        let others: Vec<PeerId> = self
            .channels
            .get(&setup.channel_id)
            .map(|channel| channel.peers.clone())
            .unwrap_or_default();
        let fingerprint = {
            let mut api = rtc.direct_api();
            api.enable_twcc_feedback();
            api.declare_media(AUDIO_MID.into(), MediaKind::Audio);
            api.expect_stream_rx(Ssrc::from(setup.audio_ssrc), None, AUDIO_MID.into(), None);
            for other in &others {
                if let Some(peer) = self.peers.get(other) {
                    declare_receive(&mut api, peer.setup.audio_ssrc);
                }
            }
            api.local_dtls_fingerprint().bytes.clone()
        };
        let id = self.next_id;
        self.next_id += 1;
        self.peers.insert(
            id,
            Peer {
                rtc,
                setup,
                ice_password: credentials.pass.clone(),
                address: None,
                timeout: now,
                level: audio::SILENT,
                forwards: HashMap::new(),
                tracks: Vec::new(),
                sending: Vec::new(),
                estimate: video::INITIAL_ESTIMATE,
                audio_out: RateMeter::default(),
                inbound: TokenBucket::new(now, video::inbound_cap(&[])),
            },
        );
        for other in &others {
            let tracks: Vec<TrackSetup> = self
                .peers
                .get(other)
                .map(|peer| peer.tracks.iter().map(|t| t.setup.clone()).collect())
                .unwrap_or_default();
            for track in &tracks {
                self.declare_sending(id, *other, track);
            }
        }
        self.drain(id);
        for other in &others {
            if let Some(peer) = self.peers.get_mut(other) {
                declare_receive(&mut peer.rtc.direct_api(), setup.audio_ssrc);
            }
            self.drain(*other);
        }
        self.channels
            .entry(setup.channel_id)
            .or_default()
            .peers
            .push(id);
        Ok((
            id,
            Transport {
                ice_ufrag: credentials.ufrag,
                ice_pwd: credentials.pass,
                dtls_fingerprint: fingerprint,
            },
        ))
    }

    /// Starts ICE and DTLS with the client's half of the transport.
    pub fn start_peer(&mut self, id: PeerId, remote: Transport) -> Result<(), SfuError> {
        let peer = self.peers.get_mut(&id).ok_or(SfuError::UnknownPeer)?;
        {
            let mut api = peer.rtc.direct_api();
            api.set_remote_ice_credentials(IceCreds {
                ufrag: remote.ice_ufrag,
                pass: remote.ice_pwd,
            });
            api.set_remote_fingerprint(Fingerprint {
                hash_func: "sha-256".to_owned(),
                bytes: remote.dtls_fingerprint,
            });
            api.set_ice_controlling(false);
            api.start_dtls(false)?;
        }
        self.drain(id);
        Ok(())
    }

    /// The client nominated a path from a new address (a network change,
    /// or the first path): media goes there from now. str0m keeps sending
    /// on the first nominated path while it lives, so the old address is
    /// dropped; to make that possible, each followed address becomes a
    /// host candidate in place of the peer-reflexive one ICE made for it.
    fn follow(&mut self, id: PeerId, source: SocketAddr) {
        let Some(peer) = self.peers.get_mut(&id) else {
            return;
        };
        let Ok(followed) = Candidate::host(source, "udp") else {
            return;
        };
        peer.rtc.add_remote_candidate(followed);
        if let Some(old) = peer.address.replace(source) {
            if let Ok(candidate) = Candidate::host(old, "udp") {
                peer.rtc.direct_api().invalidate_candidate(&candidate);
            }
            self.by_source.remove(&old);
        }
    }

    /// Removes a participant; the others stop receiving its media.
    pub fn remove_peer(&mut self, id: PeerId) {
        let track_ids: Vec<String> = self
            .peers
            .get(&id)
            .map(|peer| {
                peer.tracks
                    .iter()
                    .map(|track| track.setup.track_id.clone())
                    .collect()
            })
            .unwrap_or_default();
        for track_id in track_ids {
            self.unpublish_track(id, &track_id);
        }
        let Some(mut peer) = self.peers.remove(&id) else {
            return;
        };
        peer.rtc.disconnect();
        self.by_source.retain(|_, peer_id| *peer_id != id);
        let channel_id = peer.setup.channel_id;
        let others = match self.channels.get_mut(&channel_id) {
            Some(channel) => {
                channel.peers.retain(|peer_id| *peer_id != id);
                channel.loudest.remove(&id);
                channel.peers.clone()
            }
            None => Vec::new(),
        };
        if others.is_empty() {
            self.channels.remove(&channel_id);
        }
        let mid = receive_mid(peer.setup.audio_ssrc);
        for other in others {
            if let Some(receiver) = self.peers.get_mut(&other) {
                receiver.forwards.remove(&id);
                receiver.rtc.direct_api().remove_media(mid.as_str().into());
            }
            self.drain(other);
        }
    }

    pub fn set_state(&mut self, id: PeerId, state: PeerState) {
        if let Some(peer) = self.peers.get_mut(&id) {
            peer.setup.state = state;
        }
    }

    pub fn setup(&self, id: PeerId) -> Option<PeerSetup> {
        self.peers.get(&id).map(|peer| peer.setup)
    }

    /// Whether anyone in the channel sends on `ssrc`, audio or video.
    pub fn ssrc_in_use(&self, channel_id: i64, ssrc: u32) -> bool {
        let Some(channel) = self.channels.get(&channel_id) else {
            return false;
        };
        channel
            .peers
            .iter()
            .filter_map(|id| self.peers.get(id))
            .any(|peer| peer.setup.audio_ssrc == ssrc || video::uses_ssrc(&peer.tracks, ssrc))
    }

    /// A datagram that arrived from `source`.
    pub fn handle_receive(&mut self, now: Instant, source: SocketAddr, data: &[u8]) {
        let destination = if source.is_ipv4() {
            self.candidate_v4
        } else {
            self.candidate_v6
        };
        let Ok(contents) = data.try_into() else {
            return;
        };
        let input = Input::Receive(
            now,
            Receive {
                proto: Protocol::Udp,
                source,
                destination,
                contents,
            },
        );
        let known = self
            .by_source
            .get(&source)
            .copied()
            .filter(|id| self.peers.get(id).is_some_and(|p| p.rtc.accepts(&input)));
        let id = match known {
            Some(id) => id,
            None => {
                let Some(id) = self
                    .peers
                    .iter()
                    .find(|(_, peer)| peer.rtc.accepts(&input))
                    .map(|(id, _)| *id)
                else {
                    return;
                };
                self.by_source.insert(source, id);
                id
            }
        };
        let moved = self.peers.get(&id).is_some_and(|peer| {
            peer.address != Some(source) && signed_nomination(data, &peer.ice_password)
        });
        let handled = self
            .peers
            .get_mut(&id)
            .map(|peer| peer.rtc.handle_input(input));
        if let Some(Err(error)) = handled {
            tracing::debug!(%error, peer = id, "a packet was refused");
        }
        if moved {
            self.follow(id, source);
        }
        let packets = self.drain(id);
        for packet in packets {
            self.forward(now, id, &packet);
        }
        self.answer_keyframe_requests(now);
    }

    /// Lets every participant whose timer is due move on.
    pub fn handle_timeout(&mut self, now: Instant) {
        let due: Vec<PeerId> = self
            .peers
            .iter()
            .filter(|(_, peer)| peer.timeout <= now)
            .map(|(id, _)| *id)
            .collect();
        for id in due {
            if let Some(peer) = self.peers.get_mut(&id)
                && let Err(error) = peer.rtc.handle_input(Input::Timeout(now))
            {
                tracing::debug!(%error, peer = id, "a timeout failed");
            }
            self.drain(id);
        }
        if self.allocate_at <= now {
            self.video_tick(now);
        }
        self.answer_keyframe_requests(now);
    }

    /// When [`Self::handle_timeout`] is next needed.
    pub fn next_timeout(&self) -> Option<Instant> {
        self.peers
            .values()
            .map(|peer| peer.timeout)
            .min()
            .map(|at| at.min(self.allocate_at))
    }

    pub fn poll_transmit(&mut self) -> Option<Transmit> {
        self.transmits.pop_front()
    }

    pub fn poll_event(&mut self) -> Option<SfuEvent> {
        self.events.pop_front()
    }

    /// Everything the participant's `Rtc` wants to do now; returns the RTP
    /// packets it received, to forward.
    fn drain(&mut self, id: PeerId) -> Vec<RtpPacket> {
        let Self {
            peers,
            transmits,
            events,
            keyframe_requests,
            ..
        } = self;
        let Some(peer) = peers.get_mut(&id) else {
            return Vec::new();
        };
        let mut packets = Vec::new();
        loop {
            match peer.rtc.poll_output() {
                Ok(Output::Timeout(at)) => {
                    peer.timeout = at;
                    break;
                }
                Ok(Output::Transmit(transmit)) => transmits.push_back(Transmit {
                    destination: transmit.destination,
                    contents: transmit.contents.to_vec(),
                }),
                Ok(Output::Event(Event::Connected)) => events.push_back(SfuEvent::Connected(id)),
                Ok(Output::Event(Event::IceConnectionStateChange(
                    IceConnectionState::Disconnected,
                ))) => events.push_back(SfuEvent::Disconnected(id)),
                Ok(Output::Event(Event::RtpPacket(packet))) => packets.push(packet),
                Ok(Output::Event(Event::KeyframeRequest(request))) => {
                    keyframe_requests.push((id, request.mid));
                }
                Ok(Output::Event(Event::EgressBitrateEstimate(BweKind::Twcc {
                    estimate, ..
                }))) => peer.estimate = estimate.as_u64(),
                Ok(Output::Event(_)) => {}
                Err(error) => {
                    tracing::debug!(%error, peer = id, "the connection failed");
                    peer.timeout = far_future(peer.timeout);
                    events.push_back(SfuEvent::Disconnected(id));
                    break;
                }
            }
        }
        packets
    }

    /// Sends what a participant sent on to everyone who should get it.
    fn forward(&mut self, now: Instant, sender: PeerId, packet: &RtpPacket) {
        let Some(from) = self.peers.get_mut(&sender) else {
            return;
        };
        let ssrc = *packet.header.ssrc;
        if ssrc == from.setup.audio_ssrc {
            if from.inbound.take(now, packet.payload.len()) {
                self.forward_audio(now, sender, packet);
            }
            return;
        }
        self.forward_video(now, sender, packet);
    }
}

/// Media to receive `ssrc`'s audio on.
fn declare_receive(api: &mut str0m::change::DirectApi<'_>, ssrc: u32) {
    let mid = receive_mid(ssrc);
    api.declare_media(mid.as_str().into(), MediaKind::Audio);
    api.declare_stream_tx(Ssrc::from(ssrc), None, mid.as_str().into(), None);
}

/// A broken connection waits for removal instead of spinning.
fn far_future(now: Instant) -> Instant {
    now + Duration::from_secs(3600)
}

/// Whether `data` is a binding request nominating a path, signed with the
/// peer's ICE password: proof the client itself chose that path. Anyone
/// can send a packet from any address; only the client can sign it.
fn signed_nomination(data: &[u8], password: &str) -> bool {
    // STUN messages start with two zero bits; media and DTLS never do.
    if data.first().is_none_or(|first| *first > 1) {
        return false;
    }
    let Ok(message) = StunMessage::parse(data) else {
        return false;
    };
    message.is_binding_request()
        && message.use_candidate()
        && message.verify(password.as_bytes(), hmac_sha1)
}

fn hmac_sha1(key: &[u8], parts: &[&[u8]]) -> [u8; 20] {
    let mut mac = Hmac::<Sha1>::new_from_slice(key).expect("HMAC takes keys of any length");
    for part in parts {
        mac.update(part);
    }
    mac.finalize().into_bytes().into()
}

#[cfg(test)]
mod tests;
