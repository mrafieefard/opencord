//! Audio forwarding (plan §6): everyone hears everyone else in the channel,
//! except the muted and to the deafened; above [`LOUDEST_ONLY_ABOVE`]
//! people, only the loudest few.

use std::sync::Arc;
use std::time::{Duration, Instant};

use opencord_common::voice::{LOUDEST_HEARD, LOUDEST_ONLY_ABOVE};
use str0m::rtp::{ExtensionValues, RtpPacket, RtpWrite, Ssrc};

use super::{PeerId, Sfu};

/// How often the loudest speakers of a big channel are picked again.
pub(super) const LOUDEST_EVERY: Duration = Duration::from_millis(100);
/// Weight of the newest audio level in the running loudness.
const LEVEL_WEIGHT: f32 = 0.2;
/// Silence, in the audio level extension's -dBov.
pub(super) const SILENT: f32 = -127.0;

impl Sfu {
    /// Sends a participant's audio to everyone else in the channel who
    /// should hear it.
    pub(super) fn forward_audio(&mut self, now: Instant, sender: PeerId, packet: &RtpPacket) {
        let Some(from) = self.peers.get_mut(&sender) else {
            return;
        };
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
            peer.audio_out.add(now, packet.payload.len());
            peer.advance(now);
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
    pub(super) fn among_loudest(&mut self, now: Instant, channel_id: i64, sender: PeerId) -> bool {
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
