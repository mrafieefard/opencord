//! The selective forwarding unit (Phase 2 plan §6), Sans-IO: it never
//! touches a socket or a clock. The node feeds it packets and timeouts and
//! sends what it hands back. One str0m `Rtc` per participant, in RTP mode:
//! packets are forwarded, never decoded.
//!
//! Each participant sends its microphone on [`AUDIO_MID`] with the SSRC the
//! node gave it, unique in the channel. Every other participant receives it
//! on its own media, [`receive_mid`] of that SSRC, so a receiver tells
//! speakers apart by SSRC.

use std::collections::{HashMap, HashSet, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use hmac::{Hmac, KeyInit, Mac};
use opencord_common::voice::{AUDIO_MID, LOUDEST_HEARD, LOUDEST_ONLY_ABOVE, receive_mid};
use sha1::Sha1;
use str0m::config::{DtlsCert, Fingerprint};
use str0m::crypto::CryptoProvider;
use str0m::ice::{IceCreds, StunMessage};
use str0m::media::MediaKind;
use str0m::net::{Protocol, Receive};
use str0m::rtp::{ExtensionValues, RtpPacket, RtpWrite, Ssrc};
use str0m::{Candidate, Event, IceConnectionState, Input, Output, Rtc, RtcConfig};

/// A participant on this node.
pub type PeerId = u64;

/// How often the loudest speakers of a big channel are picked again.
const LOUDEST_EVERY: Duration = Duration::from_millis(100);
/// Weight of the newest audio level in the running loudness.
const LEVEL_WEIGHT: f32 = 0.2;
/// Silence, in the audio level extension's -dBov.
const SILENT: f32 = -127.0;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SfuEvent {
    /// ICE and DTLS are up: media flows.
    Connected(PeerId),
    /// The media connection was lost.
    Disconnected(PeerId),
}

#[derive(Debug, thiserror::Error)]
pub enum SfuError {
    #[error("no such participant")]
    UnknownPeer,
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
        let mut rtc = RtcConfig::new()
            .set_rtp_mode(true)
            .set_ice_lite(true)
            .clear_codecs()
            .enable_opus(true, false)
            .set_local_ice_credentials(credentials.clone())
            .set_crypto_provider(Arc::clone(&self.crypto))
            .set_dtls_cert(self.dtls_cert.clone())
            .build(now);
        for address in [self.candidate_v4, self.candidate_v6] {
            let candidate = Candidate::host(address, "udp")
                .map_err(|error| SfuError::Candidate(error.to_string()))?;
            rtc.add_local_candidate(candidate);
        }
        let others: Vec<(PeerId, u32)> = self
            .channels
            .get(&setup.channel_id)
            .map(|channel| {
                channel
                    .peers
                    .iter()
                    .filter_map(|id| self.peers.get(id).map(|p| (*id, p.setup.audio_ssrc)))
                    .collect()
            })
            .unwrap_or_default();
        let fingerprint = {
            let mut api = rtc.direct_api();
            api.declare_media(AUDIO_MID.into(), MediaKind::Audio);
            api.expect_stream_rx(Ssrc::from(setup.audio_ssrc), None, AUDIO_MID.into(), None);
            for (_, ssrc) in &others {
                declare_receive(&mut api, *ssrc);
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
                level: SILENT,
                forwards: HashMap::new(),
            },
        );
        self.drain(id);
        for (other, _) in &others {
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

    /// Removes a participant; the others stop receiving its media.
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

    pub fn remove_peer(&mut self, id: PeerId) {
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
    }

    /// When [`Self::handle_timeout`] is next needed.
    pub fn next_timeout(&self) -> Option<Instant> {
        self.peers.values().map(|peer| peer.timeout).min()
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

    /// Sends a participant's audio to everyone else in the channel who
    /// should hear it.
    fn forward(&mut self, now: Instant, sender: PeerId, packet: &RtpPacket) {
        let Some(from) = self.peers.get_mut(&sender) else {
            return;
        };
        if packet.header.ssrc != Ssrc::from(from.setup.audio_ssrc) {
            return;
        }
        let level = packet.header.ext_vals.audio_level.map_or(SILENT, f32::from);
        from.level += (level - from.level) * LEVEL_WEIGHT;
        let setup = from.setup;
        let receivers: Vec<PeerId> = match self.channels.get(&setup.channel_id) {
            Some(channel) => channel
                .peers
                .iter()
                .copied()
                .filter(|id| *id != sender)
                .collect(),
            None => return,
        };
        let heard = setup.state.speaks() && self.among_loudest(now, setup.channel_id, sender);
        let ext_vals = ExtensionValues {
            audio_level: packet.header.ext_vals.audio_level,
            voice_activity: packet.header.ext_vals.voice_activity,
            ..ExtensionValues::default()
        };
        for receiver in receivers {
            let Some(peer) = self.peers.get_mut(&receiver) else {
                continue;
            };
            let forward = peer.forwards.entry(sender).or_default();
            if !heard || !peer.setup.state.hears() {
                forward.skipped += 1;
                continue;
            }
            forward.lag += forward.skipped;
            forward.skipped = 0;
            let seq_no = (*packet.seq_no).saturating_sub(forward.lag).into();
            let mut api = peer.rtc.direct_api();
            let Some(stream) = api.stream_tx(&Ssrc::from(setup.audio_ssrc)) else {
                continue;
            };
            stream.write_rtp(
                RtpWrite::new(
                    packet.header.payload_type,
                    seq_no,
                    packet.header.timestamp,
                    packet.timestamp,
                    Arc::clone(&packet.payload),
                )
                .marker(packet.header.marker)
                .ext_vals(ext_vals.clone()),
            );
            self.drain(receiver);
        }
    }

    /// In channels over [`LOUDEST_ONLY_ABOVE`] people, only the
    /// [`LOUDEST_HEARD`] loudest are forwarded.
    fn among_loudest(&mut self, now: Instant, channel_id: i64, sender: PeerId) -> bool {
        let Some(channel) = self.channels.get_mut(&channel_id) else {
            return false;
        };
        if channel.peers.len() <= LOUDEST_ONLY_ABOVE {
            return true;
        }
        let stale = channel
            .loudest_at
            .is_none_or(|at| now.saturating_duration_since(at) >= LOUDEST_EVERY);
        if stale {
            let mut ranked: Vec<(PeerId, f32)> = channel
                .peers
                .iter()
                .filter_map(|id| {
                    let peer = self.peers.get(id)?;
                    peer.setup.state.speaks().then_some((*id, peer.level))
                })
                .collect();
            ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
            channel.loudest = ranked
                .into_iter()
                .take(LOUDEST_HEARD)
                .map(|(id, _)| id)
                .collect();
            channel.loudest_at = Some(now);
        }
        channel.loudest.contains(&sender)
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
