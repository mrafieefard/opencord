//! A channel's music stream (Phase 3 plan §13.3, §18.4): Opus from the
//! audio bridge, sent to everyone in the channel who hears and has not
//! muted music. It is not a participant: it has an SSRC of its own in the
//! channel and nothing else. Its numbering runs on across Lavalink
//! connections, so a failover is a short gap, not a new stream.

use std::sync::Arc;
use std::time::Instant;

use opencord_common::voice::{OPUS_PAYLOAD_TYPE, receive_mid};
use str0m::rtp::{RtpWrite, Ssrc};

use super::{Forward, PeerId, ROLLOVER, Sfu, declare_receive};

/// 20 ms of Opus at 48 kHz.
const FRAME_TICKS: u32 = 960;
const TICKS_PER_MS: u32 = 48;

/// The stream of one channel.
pub(super) struct Music {
    pub ssrc: u32,
    /// The connection the numbering below follows, and its last packet.
    connection: u64,
    last_in: Option<(u16, u32)>,
    /// What went out last: an extended sequence number and a timestamp.
    last_out: Option<(u64, u32, Instant)>,
}

/// A frame of music, as the bridge hands it on.
#[derive(Debug, Clone)]
pub struct MusicPacket {
    pub connection: u64,
    pub sequence: u16,
    pub timestamp: u32,
    pub opus: Arc<[u8]>,
    pub received_at: Instant,
}

impl Music {
    /// Where `packet` sits in the channel's stream, or `None` for a
    /// duplicate or one too late to matter.
    fn place(&mut self, packet: &MusicPacket) -> Option<(u64, u32)> {
        let same = self.connection == packet.connection;
        let placed = match (self.last_in, self.last_out) {
            (Some((seq, timestamp)), Some((out_seq, out_ts, _))) if same => {
                let step = i64::from(packet.sequence.wrapping_sub(seq) as i16);
                let ticks = packet.timestamp.wrapping_sub(timestamp) as i32;
                let out_seq = out_seq.checked_add_signed(step)?;
                (out_seq, out_ts.wrapping_add_signed(ticks), step > 0)
            }
            // A new connection, after a failover: on from where the last
            // one stopped, as much later as time went by.
            (_, Some((out_seq, out_ts, at))) => {
                let elapsed = packet.received_at.saturating_duration_since(at);
                let ms = u32::try_from(elapsed.as_millis()).unwrap_or(u32::MAX);
                let ticks = ms.saturating_mul(TICKS_PER_MS).max(FRAME_TICKS);
                (out_seq + 1, out_ts.wrapping_add(ticks), true)
            }
            // The first packet: the stream starts at its numbers.
            (_, None) => (
                u64::from(packet.sequence) + ROLLOVER,
                packet.timestamp,
                true,
            ),
        };
        let (out_seq, out_ts, newest) = placed;
        if !same {
            self.connection = packet.connection;
        }
        if newest {
            self.last_in = Some((packet.sequence, packet.timestamp));
            self.last_out = Some((out_seq, out_ts, packet.received_at));
        } else if self.last_out.is_some_and(|(last, _, _)| out_seq == last) {
            return None;
        }
        Some((out_seq, out_ts))
    }
}

impl Sfu {
    /// Gives `channel_id` a music stream, if it has none; returns its SSRC.
    /// Everyone in the channel is ready to receive it.
    pub fn start_music(&mut self, channel_id: i64, ssrc: u32) -> u32 {
        if let Some(music) = self.music.get(&channel_id) {
            return music.ssrc;
        }
        self.music.insert(
            channel_id,
            Music {
                ssrc,
                connection: 0,
                last_in: None,
                last_out: None,
            },
        );
        for id in self.channel_peers(channel_id) {
            if let Some(peer) = self.peers.get_mut(&id) {
                declare_receive(&mut peer.rtc.direct_api(), ssrc);
                peer.music_forward = Forward::default();
            }
            self.drain(id);
        }
        ssrc
    }

    /// Ends a channel's music stream.
    pub fn stop_music(&mut self, channel_id: i64) {
        let Some(music) = self.music.remove(&channel_id) else {
            return;
        };
        let mid = receive_mid(music.ssrc);
        for id in self.channel_peers(channel_id) {
            if let Some(peer) = self.peers.get_mut(&id) {
                peer.rtc.direct_api().remove_media(mid.as_str().into());
            }
            self.drain(id);
        }
    }

    /// The SSRC of a channel's music stream, while it has one.
    pub fn music_ssrc(&self, channel_id: i64) -> Option<u32> {
        self.music.get(&channel_id).map(|music| music.ssrc)
    }

    /// A participant muted music, or unmuted it: it is not sent to them
    /// while muted.
    pub fn set_music_muted(&mut self, id: PeerId, muted: bool) {
        if let Some(peer) = self.peers.get_mut(&id) {
            peer.music_muted = muted;
        }
    }

    /// Sends a frame of the channel's music to everyone who should hear it.
    pub fn forward_music(&mut self, now: Instant, channel_id: i64, packet: &MusicPacket) {
        let Some(music) = self.music.get_mut(&channel_id) else {
            return;
        };
        let ssrc = music.ssrc;
        let Some((seq, timestamp)) = music.place(packet) else {
            return;
        };
        for id in self.channel_peers(channel_id) {
            let Some(peer) = self.peers.get_mut(&id) else {
                continue;
            };
            if peer.music_muted || !peer.setup.state.hears() {
                peer.music_forward.skipped += 1;
                continue;
            }
            let seq_no = peer.music_forward.seq(seq).into();
            peer.audio_out.add(now, packet.opus.len());
            peer.advance(now);
            let mut api = peer.rtc.direct_api();
            let Some(stream) = api.stream_tx(&Ssrc::from(ssrc)) else {
                continue;
            };
            stream.write_rtp(RtpWrite::new(
                OPUS_PAYLOAD_TYPE.into(),
                seq_no,
                timestamp,
                packet.received_at,
                Arc::clone(&packet.opus),
            ));
            self.drain(id);
        }
    }

    fn channel_peers(&self, channel_id: i64) -> Vec<PeerId> {
        self.channels
            .get(&channel_id)
            .map(|channel| channel.peers.clone())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn packet(connection: u64, sequence: u16, timestamp: u32, at: Instant) -> MusicPacket {
        MusicPacket {
            connection,
            sequence,
            timestamp,
            opus: Arc::from(&[1u8][..]),
            received_at: at,
        }
    }

    fn music() -> Music {
        Music {
            ssrc: 5,
            connection: 0,
            last_in: None,
            last_out: None,
        }
    }

    #[test]
    fn numbering_follows_the_source_across_a_wrap() {
        let start = Instant::now();
        let mut music = music();
        let first = music.place(&packet(1, 65535, 1000, start)).unwrap();
        let second = music.place(&packet(1, 0, 1960, start)).unwrap();
        assert_eq!(second.0, first.0 + 1);
        assert_eq!(second.1, 1960);
        // A late packet keeps its place; a duplicate is dropped.
        let late = music.place(&packet(1, 65534, 40, start)).unwrap();
        assert_eq!(late.0, first.0 - 1);
        assert!(music.place(&packet(1, 0, 1960, start)).is_none());
    }

    #[test]
    fn a_new_connection_continues_the_stream_after_the_gap() {
        let start = Instant::now();
        let mut music = music();
        let (seq, ts) = music.place(&packet(1, 100, 50_000, start)).unwrap();

        let later = start + Duration::from_millis(500);
        let (next_seq, next_ts) = music.place(&packet(2, 9, 7, later)).unwrap();

        assert_eq!(next_seq, seq + 1);
        assert_eq!(next_ts, ts + 500 * 48);
        let (then_seq, then_ts) = music.place(&packet(2, 10, 967, later)).unwrap();
        assert_eq!((then_seq, then_ts), (next_seq + 1, next_ts + 960));
    }
}
