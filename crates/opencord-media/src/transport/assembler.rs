//! Puts received video packets back into frames: in sequence order, waiting
//! a little for retransmissions, and after a frame is lost for good, skipping
//! to the next keyframe (nothing before it decodes).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use opencord_common::video::FrameMarking;

use super::h264::depacketize;

/// How long a frame with a hole waits for the retransmission.
pub const LOSS_WAIT: Duration = Duration::from_millis(250);
/// Packets kept at most; beyond that the oldest go and the stream waits for
/// a keyframe.
pub const MAX_PACKETS: usize = 1024;

/// A received video packet.
#[derive(Debug, Clone)]
pub struct VideoPacket {
    /// Extended: never wraps.
    pub seq: u64,
    pub timestamp: u32,
    /// The frame's last packet.
    pub marker: bool,
    pub marking: Option<FrameMarking>,
    pub payload: Arc<[u8]>,
    pub arrived: Instant,
}

/// A whole frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssembledFrame {
    pub timestamp: u32,
    pub keyframe: bool,
    pub layer: u8,
    /// Width and height, from a keyframe.
    pub size: Option<(u16, u16)>,
    pub nal_units: Vec<Vec<u8>>,
    /// When its last packet arrived.
    pub arrived: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Assembled {
    Frame(AssembledFrame),
    /// A frame is gone for good: ask for a keyframe; frames are skipped
    /// until one arrives.
    Lost,
}

#[derive(Debug)]
pub struct FrameAssembler {
    packets: BTreeMap<u64, VideoPacket>,
    /// The first sequence number of the next frame.
    next: Option<u64>,
    /// Nothing decodes until a keyframe begins.
    needs_keyframe: bool,
    /// When the next frame was found to have a hole.
    stalled_since: Option<Instant>,
}

impl Default for FrameAssembler {
    fn default() -> Self {
        Self {
            packets: BTreeMap::new(),
            next: None,
            needs_keyframe: true,
            stalled_since: None,
        }
    }
}

/// How the walk through the next frame ended.
enum Walk {
    Complete(u64),
    /// Another frame begins at this sequence number before this one ended.
    CutShort(u64),
    /// This packet is missing.
    Hole(u64),
}

impl FrameAssembler {
    pub fn push(&mut self, packet: VideoPacket) {
        if self.next.is_some_and(|next| packet.seq < next) {
            return;
        }
        self.packets.entry(packet.seq).or_insert(packet);
        while self.packets.len() > MAX_PACKETS {
            if let Some((seq, _)) = self.packets.pop_first() {
                self.next = Some(seq + 1);
                self.needs_keyframe = true;
            }
        }
    }

    /// Whether frames are being skipped until a keyframe arrives.
    pub fn needs_keyframe(&self) -> bool {
        self.needs_keyframe
    }

    /// Packets held.
    pub fn len(&self) -> usize {
        self.packets.len()
    }

    /// The next frame, or a loss, once there is one.
    pub fn poll(&mut self, now: Instant) -> Option<Assembled> {
        loop {
            if self.needs_keyframe && !self.skip_to_keyframe() {
                return None;
            }
            let start = self.next?;
            match self.walk(start) {
                Walk::Complete(end) => {
                    self.stalled_since = None;
                    self.next = Some(end + 1);
                    return Some(self.take(start, end));
                }
                Walk::CutShort(begins) => {
                    self.drop_before(begins);
                    self.next = Some(begins);
                    let keyframe = self
                        .packets
                        .get(&begins)
                        .and_then(|packet| packet.marking)
                        .is_some_and(|marking| marking.keyframe);
                    if !keyframe {
                        return Some(self.lose());
                    }
                }
                Walk::Hole(missing) => {
                    let later = self.packets.range(missing + 1..).next().is_some();
                    if !later {
                        return None;
                    }
                    let since = *self.stalled_since.get_or_insert(now);
                    if now.saturating_duration_since(since) <= LOSS_WAIT {
                        return None;
                    }
                    return Some(self.lose());
                }
            }
        }
    }

    /// Drops everything before the first packet that begins a keyframe;
    /// false when none has arrived yet.
    fn skip_to_keyframe(&mut self) -> bool {
        let found = self
            .packets
            .iter()
            .find(|(_, packet)| {
                packet
                    .marking
                    .is_some_and(|marking| marking.keyframe && marking.start)
            })
            .map(|(seq, _)| *seq);
        let Some(seq) = found else {
            return false;
        };
        self.drop_before(seq);
        self.next = Some(seq);
        self.needs_keyframe = false;
        self.stalled_since = None;
        true
    }

    fn walk(&self, start: u64) -> Walk {
        let mut seq = start;
        loop {
            let Some(packet) = self.packets.get(&seq) else {
                return Walk::Hole(seq);
            };
            let begins_frame = packet.marking.is_some_and(|marking| marking.start);
            if seq != start && begins_frame {
                return Walk::CutShort(seq);
            }
            if packet.marker {
                return Walk::Complete(seq);
            }
            seq += 1;
        }
    }

    fn take(&mut self, start: u64, end: u64) -> Assembled {
        let packets: Vec<VideoPacket> = (start..=end)
            .filter_map(|seq| self.packets.remove(&seq))
            .collect();
        let units = depacketize(packets.iter().map(|packet| &packet.payload[..]));
        let (Some(first), Some(last), Ok(nal_units)) = (packets.first(), packets.last(), units)
        else {
            return self.lose();
        };
        let marking = first.marking;
        Assembled::Frame(AssembledFrame {
            timestamp: first.timestamp,
            keyframe: marking.is_some_and(|marking| marking.keyframe),
            layer: marking.map_or(0, |marking| marking.layer),
            size: marking.and_then(|marking| marking.size),
            nal_units,
            arrived: last.arrived,
        })
    }

    fn lose(&mut self) -> Assembled {
        self.needs_keyframe = true;
        self.stalled_since = None;
        if let Some(next) = self.next {
            self.drop_before(next + 1);
        }
        Assembled::Lost
    }

    fn drop_before(&mut self, seq: u64) {
        self.packets = self.packets.split_off(&seq);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::h264::packetize;

    const MAX: usize = 1080;

    /// The packets of one frame: a keyframe is SPS, PPS and an IDR slice.
    fn frame(first_seq: u64, timestamp: u32, keyframe: bool, size: usize) -> Vec<VideoPacket> {
        let units: Vec<Vec<u8>> = if keyframe {
            vec![vec![0x67, 1, 2, 3], vec![0x68, 4], body(0x65, size)]
        } else {
            vec![body(0x41, size)]
        };
        let payloads = packetize(&units, MAX);
        let count = payloads.len();
        payloads
            .into_iter()
            .enumerate()
            .map(|(index, payload)| VideoPacket {
                seq: first_seq + index as u64,
                timestamp,
                marker: index + 1 == count,
                marking: Some(FrameMarking {
                    keyframe,
                    start: index == 0,
                    layer: 1,
                    size: (keyframe && index == 0).then_some((640, 360)),
                }),
                payload: payload.into(),
                arrived: Instant::now(),
            })
            .collect()
    }

    fn body(header: u8, size: usize) -> Vec<u8> {
        let mut unit = vec![header];
        unit.extend((1..size).map(|i| (i % 253) as u8));
        unit
    }

    fn frames(assembler: &mut FrameAssembler, now: Instant) -> Vec<Assembled> {
        std::iter::from_fn(|| assembler.poll(now)).collect()
    }

    fn timestamps(out: &[Assembled]) -> Vec<Option<u32>> {
        out.iter()
            .map(|assembled| match assembled {
                Assembled::Frame(frame) => Some(frame.timestamp),
                Assembled::Lost => None,
            })
            .collect()
    }

    #[test]
    fn frames_come_out_whole_and_in_order() {
        let mut assembler = FrameAssembler::default();
        let now = Instant::now();
        let key = frame(100, 9000, true, 2500);
        let next = 100 + key.len() as u64;
        let delta = frame(next, 12000, false, 700);
        for packet in key.into_iter().chain(delta) {
            assembler.push(packet);
        }

        let out = frames(&mut assembler, now);

        assert_eq!(timestamps(&out), vec![Some(9000), Some(12000)]);
        let Assembled::Frame(first) = &out[0] else {
            panic!("expected a frame");
        };
        assert!(first.keyframe);
        assert_eq!(first.layer, 1);
        assert_eq!(first.size, Some((640, 360)));
        assert_eq!(first.nal_units.len(), 3);
        assert_eq!(first.nal_units[2], body(0x65, 2500));
    }

    #[test]
    fn a_frame_waits_for_its_reordered_and_resent_packets() {
        let mut assembler = FrameAssembler::default();
        let now = Instant::now();
        let mut key = frame(1, 0, true, 3000);
        let late = key.remove(2);
        for packet in key.into_iter().rev() {
            assembler.push(packet);
        }

        assert!(frames(&mut assembler, now).is_empty());
        assembler.push(late);
        assert_eq!(timestamps(&frames(&mut assembler, now)), vec![Some(0)]);
    }

    #[test]
    fn frames_before_the_first_keyframe_are_skipped() {
        let mut assembler = FrameAssembler::default();
        let now = Instant::now();
        let delta = frame(10, 0, false, 300);
        let key = frame(11, 3000, true, 300);
        for packet in delta.into_iter().chain(key) {
            assembler.push(packet);
        }

        assert_eq!(timestamps(&frames(&mut assembler, now)), vec![Some(3000)]);
    }

    #[test]
    fn a_frame_lost_for_good_skips_to_the_next_keyframe() {
        let mut assembler = FrameAssembler::default();
        let start = Instant::now();
        let key = frame(1, 0, true, 300);
        let mut lost = frame(4, 3000, false, 2000);
        lost.remove(1);
        let delta = frame(6, 6000, false, 300);
        let next_key = frame(7, 9000, true, 300);
        for packet in key.into_iter().chain(lost).chain(delta) {
            assembler.push(packet);
        }

        assert_eq!(timestamps(&frames(&mut assembler, start)), vec![Some(0)]);
        assert!(!assembler.needs_keyframe());
        let later = start + LOSS_WAIT + Duration::from_millis(1);
        assert_eq!(timestamps(&frames(&mut assembler, later)), vec![None]);
        assert!(assembler.needs_keyframe());
        for packet in next_key {
            assembler.push(packet);
        }
        assert_eq!(timestamps(&frames(&mut assembler, later)), vec![Some(9000)]);
        assert!(!assembler.needs_keyframe());
    }

    #[test]
    fn waiting_for_the_rest_of_a_frame_is_not_a_loss() {
        let mut assembler = FrameAssembler::default();
        let start = Instant::now();
        let mut key = frame(1, 0, true, 3000);
        key.truncate(2);
        for packet in key {
            assembler.push(packet);
        }

        let much_later = start + LOSS_WAIT * 10;
        assert!(frames(&mut assembler, much_later).is_empty());
        assert!(!assembler.needs_keyframe());
    }

    #[test]
    fn a_frame_cut_short_by_a_new_keyframe_is_dropped_quietly() {
        // The node switched layers in the middle of a frame.
        let mut assembler = FrameAssembler::default();
        let now = Instant::now();
        let key = frame(1, 0, true, 300);
        let mut cut = frame(4, 3000, false, 3000);
        cut.truncate(2);
        let switched = frame(6, 3000, true, 300);
        for packet in key.into_iter().chain(cut).chain(switched) {
            assembler.push(packet);
        }

        assert_eq!(
            timestamps(&frames(&mut assembler, now)),
            vec![Some(0), Some(3000)]
        );
    }

    #[test]
    fn a_frame_cut_short_by_a_delta_frame_is_a_loss() {
        let mut assembler = FrameAssembler::default();
        let now = Instant::now();
        let key = frame(1, 0, true, 300);
        let mut cut = frame(4, 3000, false, 3000);
        cut.truncate(2);
        let delta = frame(6, 6000, false, 300);
        for packet in key.into_iter().chain(cut).chain(delta) {
            assembler.push(packet);
        }

        assert_eq!(
            timestamps(&frames(&mut assembler, now)),
            vec![Some(0), None]
        );
        assert!(assembler.needs_keyframe());
    }

    #[test]
    fn old_and_repeated_packets_are_ignored() {
        let mut assembler = FrameAssembler::default();
        let now = Instant::now();
        let key = frame(1, 0, true, 300);
        for packet in key.clone() {
            assembler.push(packet);
        }
        assert_eq!(frames(&mut assembler, now).len(), 1);

        for packet in key {
            assembler.push(packet);
        }

        assert!(frames(&mut assembler, now).is_empty());
    }

    #[test]
    fn the_buffer_stays_bounded() {
        let mut assembler = FrameAssembler::default();
        let now = Instant::now();
        // A frame that never completes, followed by far too many packets.
        let mut stuck = frame(1, 0, true, 3000);
        stuck.remove(0);
        for packet in stuck {
            assembler.push(packet);
        }
        for seq in 10..(10 + MAX_PACKETS as u64 * 2) {
            assembler.push(VideoPacket {
                seq,
                timestamp: 0,
                marker: false,
                marking: None,
                payload: vec![0x41, 0].into(),
                arrived: now,
            });
        }

        assert!(assembler.len() <= MAX_PACKETS);
    }
}
