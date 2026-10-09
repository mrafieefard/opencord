//! Video forwarding (plan §6, §13, §14). A publisher sends each track as up
//! to three simulcast layers, each its own RTP stream. Every receiver gets
//! at most one layer of it, rewritten into one continuous stream on the
//! track's own SSRC: sequence numbers and timestamps carry on across layer
//! switches, which happen only where the new layer starts a keyframe. Layer
//! and keyframe information comes from the frame-marking header extension,
//! never from payloads.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use opencord_common::video::{FrameMarking, VideoKind, video_mid};
use opencord_common::voice::receive_mid;
use str0m::bwe::Bitrate;
use str0m::media::{KeyframeRequestKind, MediaKind, Mid};
use str0m::rtp::{ExtensionValues, RtpPacket, RtpWrite, Ssrc};

use super::allocation::{self, Candidate, LayerOption};
use super::meter::RateMeter;
use super::{PeerId, Sfu, SfuError, SfuEvent, far_future};

/// How often layers are chosen again.
const ALLOCATE_EVERY: Duration = Duration::from_millis(200);
/// A layer with no packet for this long is not being produced.
const LAYER_IDLE: Duration = Duration::from_secs(1);
/// At most one keyframe request a second for each layer (plan §6).
const KEYFRAME_EVERY: Duration = Duration::from_secs(1);
/// A layer nobody has needed for this long is no longer asked for.
const LAYER_LINGER: Duration = Duration::from_secs(1);
/// Plan §6: a track over 1.25 times its ceiling for more than 5 s is
/// stopped.
const QUALITY_TOLERANCE: f64 = 1.25;
const QUALITY_GRACE: Duration = Duration::from_secs(5);
/// Plan §14: a participant may send its tracks' ceilings and its voice,
/// plus 20 %.
const INBOUND_MARGIN: f64 = 1.2;
/// Voice at the highest bitrate a server allows (256 kbps), with packet
/// overhead.
const VOICE_ALLOWANCE: u64 = 300_000;
/// The share of a receiver's downlink estimate video may use.
const VIDEO_SHARE: f64 = 0.9;
/// A receiver's downlink before the first estimate, in bits per second.
pub(super) const INITIAL_ESTIMATE: u64 = 1_000_000;
/// A receiver that wants more than its estimate allows gets its downlink
/// probed this often, once it has shown no congestion for [`CALM`]. The
/// estimator alone only probes when the sender is application-limited, or
/// after 15 s of no change: after congestion clears, its estimate stays at
/// 1.5 times what is being sent.
const PROBE_EVERY: Duration = Duration::from_secs(5);
const CALM: Duration = Duration::from_secs(2);
/// How often str0m reports each stream, receivers' loss included
/// (receivers report video once a second).
pub(super) const STATS_EVERY: Duration = Duration::from_millis(250);
/// Reported video loss at or above this means the downlink is overloaded
/// (random loss on a poor link stays well under it; a link carrying three
/// times its capacity loses two thirds): the budget drops to what got
/// through at once, without waiting for the estimator, which lowers its
/// estimate a few percent at a time under loss.
pub(super) const LOSSY: f32 = 0.2;
/// Of what got through, the share the budget keeps.
const LOSS_MARGIN: f64 = 0.9;

/// A track as the node accepted it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackSetup {
    /// Unique in the channel.
    pub track_id: String,
    pub kind: VideoKind,
    /// Lowest first.
    pub layers: Vec<LayerSetup>,
    /// What receivers get it on, whichever layer is forwarded.
    pub ssrc: u32,
    pub rtx_ssrc: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerSetup {
    pub rid: String,
    pub ssrc: u32,
    pub rtx_ssrc: u32,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// Bits per second, within the server's ceiling.
    pub max_bitrate: u32,
}

/// A track a receiver wants, at the height of its tile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Want {
    pub track_id: String,
    pub max_height: u32,
}

/// A track on its publisher.
pub(super) struct Track {
    pub setup: TrackSetup,
    layers: Vec<LayerState>,
    meter: RateMeter,
    /// Since when it has been over its ceiling.
    over_since: Option<Instant>,
    /// The layers the sender was last asked for, since when, and when
    /// anyone last needed each.
    asked: Vec<bool>,
    asked_since: Vec<Instant>,
    needed_at: Vec<Instant>,
    /// The layers last reported as produced.
    reported: Vec<bool>,
}

#[derive(Default)]
struct LayerState {
    last_packet: Option<Instant>,
    /// What it sends.
    meter: RateMeter,
    /// From the latest keyframe.
    size: Option<(u16, u16)>,
    keyframe_asked: Option<Instant>,
    /// A keyframe request waits for the next allowed moment.
    keyframe_wanted: bool,
}

impl Track {
    fn new(now: Instant, setup: TrackSetup) -> Self {
        let count = setup.layers.len();
        Self {
            layers: (0..count).map(|_| LayerState::default()).collect(),
            meter: RateMeter::default(),
            over_since: None,
            // The sender starts with every layer; unneeded ones are let go.
            asked: vec![true; count],
            asked_since: vec![now; count],
            needed_at: vec![now; count],
            reported: vec![false; count],
            setup,
        }
    }

    fn layer_of(&self, ssrc: u32) -> Option<usize> {
        self.setup
            .layers
            .iter()
            .position(|layer| layer.ssrc == ssrc)
    }

    fn producing(&self, now: Instant, index: usize) -> bool {
        self.layers[index]
            .last_packet
            .is_some_and(|at| now.saturating_duration_since(at) < LAYER_IDLE)
    }

    /// Asked for long enough to have come, and not coming: the sender's
    /// uplink cannot carry it (plan §7.10).
    fn missing(&self, now: Instant, index: usize) -> bool {
        self.asked[index]
            && now.saturating_duration_since(self.asked_since[index]) >= LAYER_IDLE
            && !self.producing(now, index)
    }

    /// Bits per second it may send.
    fn ceiling(&self) -> u64 {
        self.setup
            .layers
            .iter()
            .map(|layer| u64::from(layer.max_bitrate))
            .sum()
    }

    /// The layers as receivers could get them. A layer costs its declared
    /// maximum, and never less than the layer below it; a layer the sender
    /// was told to stop is gone at once.
    fn options(&self, now: Instant) -> Vec<LayerOption> {
        let mut floor = 0;
        self.setup
            .layers
            .iter()
            .zip(&self.layers)
            .enumerate()
            .map(|(index, (setup, state))| {
                floor = u64::from(setup.max_bitrate).max(floor);
                LayerOption {
                    index: index as u8,
                    height: state
                        .size
                        .map_or(setup.height, |(_, height)| u32::from(height)),
                    cost: floor,
                    available: self.asked[index] && self.producing(now, index),
                    sent: state.meter.bits_per_second(now),
                }
            })
            .collect()
    }

    fn rids(&self, flags: &[bool]) -> Vec<String> {
        self.setup
            .layers
            .iter()
            .zip(flags)
            .filter(|(_, on)| **on)
            .map(|(layer, _)| layer.rid.clone())
            .collect()
    }
}

/// One receiver's copy of someone's track: what it wants, which layer it
/// gets, and how that layer's packets are rewritten for it.
pub(super) struct Sending {
    publisher: PeerId,
    track_id: String,
    ssrc: u32,
    /// The height of its tile, while it wants the track.
    want: Option<u32>,
    /// The layer being forwarded.
    current: Option<u8>,
    /// The layer to switch to at its next keyframe.
    target: Option<u8>,
    /// The layer it asked for, bandwidth aside.
    desired: Option<u8>,
    /// The layer it would get if the sender produced every layer.
    ideal: Option<u8>,
    /// Earlier packets of the current layer are not forwarded.
    from_seq: u64,
    seq_offset: u64,
    ts_offset: u32,
    /// The newest packet it got.
    last_seq: Option<u64>,
    last_ts: u32,
    last_at: Option<Instant>,
    /// The receiver's last report: highest sequence number and packets
    /// lost so far.
    last_report: Option<(u64, u64)>,
}

impl Sending {
    /// Whether its stream is the media `mid`.
    pub(super) fn receives_on(&self, mid: &Mid) -> bool {
        Mid::from(receive_mid(self.ssrc).as_str()) == *mid
    }

    fn new(publisher: PeerId, setup: &TrackSetup) -> Self {
        Self {
            publisher,
            track_id: setup.track_id.clone(),
            ssrc: setup.ssrc,
            want: None,
            current: None,
            target: None,
            desired: None,
            ideal: None,
            from_seq: 0,
            seq_offset: 0,
            ts_offset: 0,
            last_seq: None,
            last_ts: 0,
            last_at: None,
            last_report: None,
        }
    }

    /// The share of packets lost between the receiver's last two reports,
    /// from their running totals. A resent packet that arrives late lowers
    /// the total, which counts as no loss (RFC 3550 §6.4.1); str0m's own
    /// fraction wraps such an interval to nearly all lost.
    pub(super) fn loss_since_last_report(&mut self, highest: u64, lost: u64) -> Option<f32> {
        let previous = self.last_report.replace((highest, lost));
        let (highest_before, lost_before) = previous?;
        let expected = highest
            .checked_sub(highest_before)
            .filter(|count| *count > 0)?;
        let lost = lost.saturating_sub(lost_before).min(expected);
        Some(lost as f32 / expected as f32)
    }

    /// Starts forwarding `layer` at the packet `seq`, a keyframe's first:
    /// its numbers continue where the last layer's stopped.
    fn switch(&mut self, now: Instant, layer: u8, seq: u64, timestamp: u32) {
        self.current = Some(layer);
        self.from_seq = seq;
        // A new copy keeps the number's low 16 bits but no rollovers:
        // SRTP takes a new stream's first packet to have none.
        let next_seq = self.last_seq.map_or(seq % super::ROLLOVER, |last| last + 1);
        self.seq_offset = next_seq.wrapping_sub(seq);
        let next_ts = match self.last_at {
            Some(at) => {
                let elapsed = now.saturating_duration_since(at);
                let ticks = (elapsed.as_micros() * 9 / 100).max(1);
                self.last_ts
                    .wrapping_add(u32::try_from(ticks).unwrap_or(u32::MAX))
            }
            None => timestamp,
        };
        self.ts_offset = next_ts.wrapping_sub(timestamp);
    }

    /// The sequence number and timestamp this receiver sees.
    fn rewrite(&mut self, now: Instant, seq: u64, timestamp: u32) -> (u64, u32) {
        let out_seq = seq.wrapping_add(self.seq_offset);
        let out_ts = timestamp.wrapping_add(self.ts_offset);
        if self.last_seq.is_none_or(|last| out_seq > last) {
            self.last_seq = Some(out_seq);
            self.last_ts = out_ts;
            self.last_at = Some(now);
        }
        (out_seq, out_ts)
    }
}

/// Bits per second a participant with these tracks may send.
pub(super) fn inbound_cap(tracks: &[Track]) -> u64 {
    let ceilings: u64 = tracks.iter().map(Track::ceiling).sum();
    ((VOICE_ALLOWANCE + ceilings) as f64 * INBOUND_MARGIN) as u64
}

pub(super) fn uses_ssrc(tracks: &[Track], ssrc: u32) -> bool {
    tracks.iter().any(|track| {
        track.setup.ssrc == ssrc
            || track.setup.rtx_ssrc == ssrc
            || track
                .setup
                .layers
                .iter()
                .any(|layer| layer.ssrc == ssrc || layer.rtx_ssrc == ssrc)
    })
}

impl Sfu {
    /// Starts taking a track from `id`; everyone else in its channel can ask
    /// for it from now.
    pub fn publish_track(
        &mut self,
        now: Instant,
        id: PeerId,
        setup: TrackSetup,
    ) -> Result<(), SfuError> {
        let first = setup.layers.first().ok_or(SfuError::NoLayers)?.ssrc;
        let peer = self.peers.get_mut(&id).ok_or(SfuError::UnknownPeer)?;
        if peer
            .tracks
            .iter()
            .any(|track| track.setup.track_id == setup.track_id)
        {
            return Err(SfuError::TrackExists);
        }
        {
            let mid = video_mid(first);
            let mut api = peer.rtc.direct_api();
            api.declare_media(mid.as_str().into(), MediaKind::Video);
            for layer in &setup.layers {
                api.expect_stream_rx(
                    Ssrc::from(layer.ssrc),
                    Some(Ssrc::from(layer.rtx_ssrc)),
                    mid.as_str().into(),
                    Some(layer.rid.as_str().into()),
                );
            }
        }
        peer.tracks.push(Track::new(now, setup.clone()));
        let cap = inbound_cap(&peer.tracks);
        peer.inbound.set_rate(cap);
        let channel_id = peer.setup.channel_id;
        self.drain(id);
        for receiver in self.others(channel_id, id) {
            self.declare_sending(receiver, id, &setup);
            self.drain(receiver);
        }
        self.allocate_at = self.allocate_at.min(now);
        Ok(())
    }

    /// Stops a track; its receivers get nothing more of it.
    pub fn unpublish_track(&mut self, id: PeerId, track_id: &str) -> bool {
        let Some(peer) = self.peers.get_mut(&id) else {
            return false;
        };
        let Some(index) = peer
            .tracks
            .iter()
            .position(|track| track.setup.track_id == track_id)
        else {
            return false;
        };
        let track = peer.tracks.remove(index);
        if let Some(first) = track.setup.layers.first() {
            peer.rtc
                .direct_api()
                .remove_media(video_mid(first.ssrc).as_str().into());
        }
        let cap = inbound_cap(&peer.tracks);
        peer.inbound.set_rate(cap);
        let channel_id = peer.setup.channel_id;
        self.drain(id);
        let mid = receive_mid(track.setup.ssrc);
        for receiver in self.others(channel_id, id) {
            if let Some(peer) = self.peers.get_mut(&receiver) {
                peer.sending
                    .retain(|sending| !(sending.publisher == id && sending.track_id == track_id));
                peer.rtc.direct_api().remove_media(mid.as_str().into());
            }
            self.drain(receiver);
        }
        true
    }

    /// What `id` wants of others' video; tracks it does not list are not
    /// sent to it (plan §6, `MediaSinkWants`). Takes effect at once.
    pub fn set_wants(&mut self, now: Instant, id: PeerId, wants: &[Want]) {
        let Some(peer) = self.peers.get_mut(&id) else {
            return;
        };
        for sending in &mut peer.sending {
            sending.want = wants
                .iter()
                .find(|want| want.track_id == sending.track_id)
                .map(|want| want.max_height);
            if sending.want.is_none() {
                sending.current = None;
                sending.target = None;
            }
        }
        if !peer.sending.is_empty() {
            self.allocate_at = self.allocate_at.min(now);
        }
    }

    /// Who may receive `user_id`'s screen share in `channel_id`: those
    /// who chose to watch it (plan §6). Nobody, until the main server says.
    pub fn set_stream_viewers(
        &mut self,
        now: Instant,
        channel_id: i64,
        user_id: i64,
        viewers: Vec<i64>,
    ) {
        if viewers.is_empty() {
            self.stream_viewers.remove(&(channel_id, user_id));
        } else {
            self.stream_viewers
                .insert((channel_id, user_id), viewers.into_iter().collect());
        }
        self.allocate_at = self.allocate_at.min(now);
    }

    /// The tracks `id` publishes.
    pub fn tracks(&self, id: PeerId) -> Vec<TrackSetup> {
        self.peers
            .get(&id)
            .map(|peer| {
                peer.tracks
                    .iter()
                    .map(|track| track.setup.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The layers of a track its sender was last seen producing, by rid.
    pub fn available_layers(&self, id: PeerId, track_id: &str) -> Vec<String> {
        self.peers
            .get(&id)
            .and_then(|peer| {
                peer.tracks
                    .iter()
                    .find(|track| track.setup.track_id == track_id)
            })
            .map(|track| track.rids(&track.reported))
            .unwrap_or_default()
    }

    /// Lets `receiver` receive a track `publisher` sends.
    pub(super) fn declare_sending(
        &mut self,
        receiver: PeerId,
        publisher: PeerId,
        setup: &TrackSetup,
    ) {
        let Some(peer) = self.peers.get_mut(&receiver) else {
            return;
        };
        let mid = receive_mid(setup.ssrc);
        let mut api = peer.rtc.direct_api();
        api.declare_media(mid.as_str().into(), MediaKind::Video);
        // Forwarded as it arrives: layer choice holds the rate, and a pacer
        // queue would make the next layer's keyframe wait behind the last
        // layer's backlog.
        api.declare_stream_tx(
            Ssrc::from(setup.ssrc),
            Some(Ssrc::from(setup.rtx_ssrc)),
            mid.as_str().into(),
            None,
        )
        .set_unpaced(true);
        peer.sending.push(Sending::new(publisher, setup));
    }

    fn others(&self, channel_id: i64, id: PeerId) -> Vec<PeerId> {
        self.channels
            .get(&channel_id)
            .map(|channel| {
                channel
                    .peers
                    .iter()
                    .copied()
                    .filter(|other| *other != id)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Sends a video packet to everyone getting its layer.
    pub(super) fn forward_video(&mut self, now: Instant, sender: PeerId, packet: &RtpPacket) {
        let ssrc = *packet.header.ssrc;
        let bytes = packet.payload.len();
        let marking = packet
            .header
            .ext_vals
            .user_values
            .get::<FrameMarking>()
            .copied();
        let Some(from) = self.peers.get_mut(&sender) else {
            return;
        };
        let Some((track_index, layer_index)) = from
            .tracks
            .iter()
            .enumerate()
            .find_map(|(index, track)| track.layer_of(ssrc).map(|layer| (index, layer)))
        else {
            return;
        };
        let track = &mut from.tracks[track_index];
        track.meter.add(now, bytes);
        let started = !track.producing(now, layer_index);
        let state = &mut track.layers[layer_index];
        state.last_packet = Some(now);
        state.meter.add(now, bytes);
        if let Some(size) = marking.and_then(|marking| marking.size) {
            state.size = Some(size);
        }
        let track_id = track.setup.track_id.clone();
        if started {
            // A layer the sender (re)started: choose layers again now.
            self.allocate_at = self.allocate_at.min(now);
        }
        if !from.inbound.take(now, bytes) {
            return;
        }
        let channel_id = from.setup.channel_id;
        let layer = layer_index as u8;
        let keyframe_start = marking.is_some_and(|marking| marking.keyframe && marking.start);
        let seq = *packet.seq_no;
        let mut ext_vals = ExtensionValues {
            video_orientation: packet.header.ext_vals.video_orientation,
            ..ExtensionValues::default()
        };
        ext_vals.user_values = packet.header.ext_vals.user_values.clone();
        for receiver in self.others(channel_id, sender) {
            let Some(peer) = self.peers.get_mut(&receiver) else {
                continue;
            };
            let Some(sending) = peer
                .sending
                .iter_mut()
                .find(|sending| sending.publisher == sender && sending.track_id == track_id)
            else {
                continue;
            };
            if sending.target == Some(layer) && sending.current != Some(layer) && keyframe_start {
                sending.switch(now, layer, seq, packet.header.timestamp);
            }
            if sending.current != Some(layer) || seq < sending.from_seq {
                continue;
            }
            let (out_seq, out_ts) = sending.rewrite(now, seq, packet.header.timestamp);
            let out_ssrc = sending.ssrc;
            peer.advance(now);
            let mut api = peer.rtc.direct_api();
            let Some(stream) = api.stream_tx(&Ssrc::from(out_ssrc)) else {
                continue;
            };
            stream.write_rtp(
                RtpWrite::new(
                    packet.header.payload_type,
                    out_seq.into(),
                    out_ts,
                    packet.timestamp,
                    Arc::clone(&packet.payload),
                )
                .marker(packet.header.marker)
                .nackable(true)
                .ext_vals(ext_vals.clone()),
            );
            self.drain(receiver);
        }
    }

    /// Chooses layers, asks senders for what is needed, and enforces
    /// ceilings.
    pub(super) fn video_tick(&mut self, now: Instant) {
        if self.peers.values().all(|peer| peer.tracks.is_empty()) {
            self.allocate_at = far_future(now);
            return;
        }
        self.allocate_at = now + ALLOCATE_EVERY;
        self.enforce_quality(now);
        self.report_layers(now);
        let channels: Vec<i64> = self.channels.keys().copied().collect();
        for channel_id in channels {
            self.allocate_channel(now, channel_id);
        }
        self.ask_for_layers(now);
        self.retry_keyframe_requests(now);
    }

    /// Picks each receiver's layers within its downlink.
    fn allocate_channel(&mut self, now: Instant, channel_id: i64) {
        let Some(channel) = self.channels.get(&channel_id) else {
            return;
        };
        let members = channel.peers.clone();
        let offers: Vec<Offer> = members
            .iter()
            .filter_map(|id| self.peers.get(id).map(|peer| (*id, peer)))
            .flat_map(|(id, peer)| {
                let viewers = self
                    .stream_viewers
                    .get(&(peer.setup.channel_id, peer.setup.user_id))
                    .cloned()
                    .unwrap_or_default();
                peer.tracks.iter().map(move |track| {
                    let screen = track.setup.kind == VideoKind::Screen;
                    Offer {
                        publisher: id,
                        track_id: track.setup.track_id.clone(),
                        screen,
                        layers: track.options(now),
                        viewers: screen.then(|| viewers.clone()),
                    }
                })
            })
            .collect();
        let mut keyframes: Vec<(PeerId, String, u8)> = Vec::new();
        for id in &members {
            let Some(peer) = self.peers.get_mut(id) else {
                continue;
            };
            let audio = peer.audio_out.bits_per_second(now);
            let estimated = ((peer.estimate as f64 * VIDEO_SHARE) as u64).saturating_sub(audio);
            let budget = within_loss_cap(peer, estimated, &offers);
            let offered: Vec<Option<&Offer>> = peer
                .sending
                .iter()
                .map(|sending| {
                    offers.iter().find(|offer| {
                        offer.publisher == sending.publisher && offer.track_id == sending.track_id
                    })
                })
                .collect();
            let user_id = peer.setup.user_id;
            let wanted: Vec<usize> = (0..peer.sending.len())
                .filter(|&i| {
                    peer.sending[i].want.is_some()
                        && offered[i].is_some_and(|offer| offer.for_user(user_id))
                })
                .collect();
            let candidates: Vec<Candidate<'_>> = wanted
                .iter()
                .filter_map(|&i| {
                    let offer = offered[i]?;
                    Some(Candidate {
                        screen: offer.screen,
                        want_height: peer.sending[i].want.unwrap_or(0),
                        layers: &offer.layers,
                        current: peer.sending[i].current,
                    })
                })
                .collect();
            let targets = allocation::allocate(budget, &candidates);
            // What it would get with every layer on: the sender is asked
            // for those, so a layer it stopped comes back when needed.
            let everything: Vec<Vec<LayerOption>> = candidates
                .iter()
                .map(|candidate| {
                    candidate
                        .layers
                        .iter()
                        .map(|layer| LayerOption {
                            available: true,
                            ..*layer
                        })
                        .collect()
                })
                .collect();
            let unrestricted: Vec<Candidate<'_>> = candidates
                .iter()
                .zip(&everything)
                .map(|(candidate, layers)| Candidate {
                    layers,
                    ..*candidate
                })
                .collect();
            let ideals = allocation::allocate(budget, &unrestricted);
            let mut desired_video = 0;
            for (position, sending) in peer.sending.iter_mut().enumerate() {
                let slot = wanted.iter().position(|&i| i == position);
                let (target, ideal, desired) = match (slot, offered[position]) {
                    (Some(slot), Some(offer)) => (
                        targets.get(slot).copied().flatten(),
                        ideals.get(slot).copied().flatten(),
                        allocation::desired(sending.want.unwrap_or(0), &offer.layers),
                    ),
                    _ => (None, None, None),
                };
                if let (Some(desired), Some(offer)) = (desired, offered[position]) {
                    desired_video += offer
                        .layers
                        .iter()
                        .find(|layer| layer.index == desired)
                        .map_or(0, |layer| layer.cost);
                }
                sending.desired = desired;
                sending.ideal = ideal;
                sending.target = target;
                match target {
                    None => sending.current = None,
                    Some(target) if sending.current != Some(target) => {
                        keyframes.push((sending.publisher, sending.track_id.clone(), target));
                    }
                    Some(_) => {}
                }
            }
            // The estimator probes up to what the allocation needs to give
            // everyone what they asked for: headroom and audio included.
            let desired = desired_video as f64 * (1.0 + allocation::UPGRADE_HEADROOM) / VIDEO_SHARE;
            let desired_bitrate = (desired as u64 + audio) * u64::from(desired_video > 0);
            peer.rtc
                .bwe()
                .set_desired_bitrate(Bitrate::bps(desired_bitrate));
            let limited = desired_bitrate > peer.estimate;
            let calm = peer
                .congested
                .is_none_or(|at| now.saturating_duration_since(at) >= CALM);
            let due = peer
                .probed
                .is_none_or(|at| now.saturating_duration_since(at) >= PROBE_EVERY);
            if limited && calm && due {
                // A fresh estimator probes at 3x and 6x of where it starts:
                // from half the target at least, so one probe passes it
                // however far below the estimate is (a screen's main layer
                // is several times its low one, and its low layer often
                // sends well under its maximum).
                peer.probed = Some(now);
                let start = peer.estimate.max(desired_bitrate / 2);
                peer.rtc.bwe().reset(Bitrate::bps(start));
            }
        }
        for (publisher, track_id, layer) in keyframes {
            self.request_keyframe(now, publisher, &track_id, layer);
        }
        for id in &members {
            self.drain(*id);
        }
    }

    /// Asks a layer's sender for a keyframe, at most once a second.
    fn request_keyframe(&mut self, now: Instant, publisher: PeerId, track_id: &str, layer: u8) {
        let Some(peer) = self.peers.get_mut(&publisher) else {
            return;
        };
        let Some(track) = peer
            .tracks
            .iter_mut()
            .find(|track| track.setup.track_id == track_id)
        else {
            return;
        };
        let index = usize::from(layer);
        let (Some(state), Some(setup)) =
            (track.layers.get_mut(index), track.setup.layers.get(index))
        else {
            return;
        };
        let recently = state
            .keyframe_asked
            .is_some_and(|at| now.saturating_duration_since(at) < KEYFRAME_EVERY);
        if recently {
            state.keyframe_wanted = true;
            return;
        }
        state.keyframe_asked = Some(now);
        state.keyframe_wanted = false;
        if let Some(stream) = peer.rtc.direct_api().stream_rx(&Ssrc::from(setup.ssrc)) {
            stream.request_keyframe(KeyframeRequestKind::Pli);
        }
        self.drain(publisher);
    }

    /// Passes receivers' keyframe requests to the layer they are getting.
    pub(super) fn answer_keyframe_requests(&mut self, now: Instant) {
        let requests = std::mem::take(&mut self.keyframe_requests);
        for (receiver, mid) in requests {
            let Some(peer) = self.peers.get(&receiver) else {
                continue;
            };
            let found = peer.sending.iter().find_map(|sending| {
                let layer = sending.current.or(sending.target)?;
                (Mid::from(receive_mid(sending.ssrc).as_str()) == mid)
                    .then(|| (sending.publisher, sending.track_id.clone(), layer))
            });
            if let Some((publisher, track_id, layer)) = found {
                self.request_keyframe(now, publisher, &track_id, layer);
            }
        }
    }

    fn retry_keyframe_requests(&mut self, now: Instant) {
        let mut due: Vec<(PeerId, String, u8)> = Vec::new();
        for (id, peer) in &self.peers {
            for track in &peer.tracks {
                for (index, state) in track.layers.iter().enumerate() {
                    let allowed = state
                        .keyframe_asked
                        .is_none_or(|at| now.saturating_duration_since(at) >= KEYFRAME_EVERY);
                    if state.keyframe_wanted && allowed {
                        due.push((*id, track.setup.track_id.clone(), index as u8));
                    }
                }
            }
        }
        for (publisher, track_id, layer) in due {
            self.request_keyframe(now, publisher, &track_id, layer);
        }
    }

    /// Tells senders which layers anyone needs: what receivers would get
    /// with every layer on, are switching to, or are getting; and below a
    /// needed layer the sender is not sending, the layer under it, so
    /// receivers have something until it comes.
    fn ask_for_layers(&mut self, now: Instant) {
        let mut needed: HashMap<(PeerId, &str), Vec<u8>> = HashMap::new();
        for peer in self.peers.values() {
            for sending in &peer.sending {
                let layers = [sending.ideal, sending.target, sending.current];
                needed
                    .entry((sending.publisher, sending.track_id.as_str()))
                    .or_default()
                    .extend(layers.into_iter().flatten());
            }
        }
        let needed: HashMap<(PeerId, String), Vec<u8>> = needed
            .into_iter()
            .map(|((publisher, track_id), layers)| ((publisher, track_id.to_owned()), layers))
            .collect();
        let Self { peers, events, .. } = self;
        for (id, peer) in peers.iter_mut() {
            for track in &mut peer.tracks {
                let marks = needed.get(&(*id, track.setup.track_id.clone()));
                let mut wanted: Vec<bool> = (0..track.layers.len())
                    .map(|index| marks.is_some_and(|layers| layers.contains(&(index as u8))))
                    .collect();
                for index in (1..wanted.len()).rev() {
                    if wanted[index] && track.missing(now, index) {
                        wanted[index - 1] = true;
                    }
                }
                let mut asked = Vec::with_capacity(track.layers.len());
                for (index, needed_at) in track.needed_at.iter_mut().enumerate() {
                    if wanted[index] {
                        *needed_at = now;
                    }
                    asked.push(now.saturating_duration_since(*needed_at) < LAYER_LINGER);
                }
                if asked != track.asked {
                    for (index, (now_asked, was_asked)) in
                        asked.iter().zip(&track.asked).enumerate()
                    {
                        if *now_asked && !*was_asked {
                            track.asked_since[index] = now;
                        }
                    }
                    track.asked = asked;
                    events.push_back(SfuEvent::LayerWants {
                        peer: *id,
                        track_id: track.setup.track_id.clone(),
                        rids: track.rids(&track.asked),
                    });
                }
            }
        }
    }

    /// Reports which layers each sender produces, when that changes.
    fn report_layers(&mut self, now: Instant) {
        let Self { peers, events, .. } = self;
        for (id, peer) in peers.iter_mut() {
            for track in &mut peer.tracks {
                let producing: Vec<bool> = (0..track.layers.len())
                    .map(|index| track.producing(now, index))
                    .collect();
                if producing != track.reported {
                    track.reported = producing;
                    events.push_back(SfuEvent::LayersAvailable {
                        peer: *id,
                        track_id: track.setup.track_id.clone(),
                        rids: track.rids(&track.reported),
                    });
                }
            }
        }
    }

    /// Stops tracks that stayed far over their ceiling (plan §6).
    fn enforce_quality(&mut self, now: Instant) {
        let mut stopped: Vec<(PeerId, String)> = Vec::new();
        for (id, peer) in &mut self.peers {
            for track in &mut peer.tracks {
                let rate = track.meter.bits_per_second(now) as f64;
                if rate > track.ceiling() as f64 * QUALITY_TOLERANCE {
                    let since = *track.over_since.get_or_insert(now);
                    if now.saturating_duration_since(since) > QUALITY_GRACE {
                        stopped.push((*id, track.setup.track_id.clone()));
                    }
                } else {
                    track.over_since = None;
                }
            }
        }
        for (peer, track_id) in stopped {
            tracing::info!(peer, track_id, "a track stayed over its bitrate ceiling");
            self.unpublish_track(peer, &track_id);
            self.events
                .push_back(SfuEvent::TrackStopped { peer, track_id });
        }
    }
}

/// The budget, capped at what got through when the receiver last reported
/// heavy loss. The cap holds while the loss lasts and goes once the
/// receiver's reports are clean again; it never takes anyone below the
/// lowest layer of what they want (only the estimate stops video).
fn within_loss_cap(peer: &mut super::Peer, estimated: u64, offers: &[Offer]) -> u64 {
    let offer_of = |sending: &Sending| {
        offers.iter().find(|offer| {
            offer.publisher == sending.publisher && offer.track_id == sending.track_id
        })
    };
    if peer.smoothed_loss < LOSSY / 2.0 {
        peer.loss_cap = None;
    }
    if let Some(loss) = peer.reported_loss.take() {
        let sent: u64 = peer
            .sending
            .iter()
            .filter_map(|sending| {
                let layer = sending.current?;
                offer_of(sending)?
                    .layers
                    .iter()
                    .find(|option| option.index == layer)
                    .map(|option| option.cost)
            })
            .sum();
        let floor: u64 = peer
            .sending
            .iter()
            .filter(|sending| sending.want.is_some())
            .filter_map(|sending| offer_of(sending)?.layers.first().map(|option| option.cost))
            .sum();
        let through = (sent as f64 * f64::from(1.0 - loss) * LOSS_MARGIN) as u64;
        let cap = peer.loss_cap.map_or(through, |cap| cap.min(through));
        peer.loss_cap = Some(cap.max(floor));
    }
    peer.loss_cap.map_or(estimated, |cap| cap.min(estimated))
}

/// A track as receivers could get it.
struct Offer {
    publisher: PeerId,
    track_id: String,
    screen: bool,
    layers: Vec<LayerOption>,
    /// Who may receive it; everyone when `None` (cameras).
    viewers: Option<HashSet<i64>>,
}

impl Offer {
    fn for_user(&self, user_id: i64) -> bool {
        self.viewers
            .as_ref()
            .is_none_or(|viewers| viewers.contains(&user_id))
    }
}

#[cfg(test)]
mod sequence_tests {
    use super::*;

    fn setup() -> TrackSetup {
        TrackSetup {
            track_id: "cam".to_owned(),
            kind: VideoKind::Camera,
            layers: Vec::new(),
            ssrc: 1,
            rtx_ssrc: 2,
        }
    }

    #[test]
    fn a_receivers_copy_starts_without_rollovers_and_runs_on_across_layers() {
        let now = Instant::now();
        let mut sending = Sending::new(7, &setup());
        // The publisher's layer has wrapped twice before this receiver came:
        // SRTP takes a new stream to start at rollover count 0.
        sending.switch(now, 0, 0x2_fff0, 1000);

        assert_eq!(sending.rewrite(now, 0x2_fff0, 1000).0, 0xfff0);
        assert_eq!(sending.rewrite(now, 0x2_fff1, 4000).0, 0xfff1);
        // Another layer, numbered on from where this one was.
        sending.switch(now, 1, 0x5_0010, 7000);
        assert_eq!(sending.rewrite(now, 0x5_0010, 7000).0, 0xfff2);
    }
}
