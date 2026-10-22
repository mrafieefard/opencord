//! One person's incoming audio put back in order and played out at a steady
//! delay (plan §7.5): the 95th percentile of their recent jitter plus 10 ms,
//! between 20 and 200 ms. The delay changes between talk spurts; a missing
//! frame comes back from the next packet's FEC, or is concealed.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{FRAME, SAMPLE_RATE};

pub const MIN_DELAY: Duration = Duration::from_millis(20);
pub const MAX_DELAY: Duration = Duration::from_millis(200);
/// Added to the measured jitter.
const MARGIN_MS: f64 = 10.0;
/// Arrivals the jitter is measured over: two seconds of 20 ms packets.
const WINDOW: usize = 100;
/// Packets kept: one second.
const CAPACITY: usize = 50;
/// Frames concealed with nothing newer before the sender counts as paused.
const MAX_CONCEALED: u32 = 3;
/// A gap this long is a pause between talk spurts, not loss.
const MAX_GAP: u64 = 10 * FRAME as u64;
/// A backlog this far beyond the delay (after a network stall) is cut...
const TRIM_ABOVE: Duration = Duration::from_millis(100);
/// ...down to this much beyond it.
const TRIM_TO: Duration = Duration::from_millis(20);

/// What to play for the next frame.
#[derive(Debug, Clone)]
pub enum Playout {
    /// Decode this packet.
    Frame(Arc<[u8]>),
    /// The frame is lost, but this next packet carries it as forward error
    /// correction; the packet itself plays next.
    Recover(Arc<[u8]>),
    /// The frame is lost; conceal it.
    Conceal,
    /// Nothing is playing.
    Silence,
}

#[derive(Debug)]
struct Packet {
    payload: Arc<[u8]>,
    samples: u64,
    marker: bool,
    arrived: Instant,
    /// How late it was against the earliest arrival in the window, in ms.
    delay: f64,
}

#[derive(Debug)]
pub struct JitterBuffer {
    /// By extended RTP timestamp.
    packets: BTreeMap<u64, Packet>,
    /// The next frame to play; `None` between talk spurts.
    next: Option<u64>,
    /// When the buffered talk spurt starts, between talk spurts.
    start_at: Option<Instant>,
    /// Packets before this are too late.
    played_until: Option<u64>,
    concealed: u32,
    highest: Option<u64>,
    /// The first arrival, which delays are measured against.
    reference: Option<(Instant, u64)>,
    delays: VecDeque<f64>,
    min_delay: f64,
    target: Duration,
    scratch: Vec<f64>,
}

impl Default for JitterBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl JitterBuffer {
    pub fn new() -> Self {
        Self {
            packets: BTreeMap::new(),
            next: None,
            start_at: None,
            played_until: None,
            concealed: 0,
            highest: None,
            reference: None,
            delays: VecDeque::with_capacity(WINDOW),
            min_delay: 0.0,
            target: MIN_DELAY,
            scratch: Vec::with_capacity(WINDOW),
        }
    }

    /// A packet arrived. `marker` starts a talk spurt.
    pub fn insert(&mut self, timestamp: u32, marker: bool, payload: Arc<[u8]>, arrived: Instant) {
        let timestamp = self.extend(timestamp);
        let delay = self.record_delay(timestamp, arrived);
        if self.played_until.is_some_and(|until| timestamp < until) {
            return;
        }
        let samples = opus::packet::get_nb_samples(&payload, SAMPLE_RATE)
            .map_or(FRAME as u64, |samples| samples as u64);
        let packet = Packet {
            payload,
            samples,
            marker,
            arrived,
            delay,
        };
        if self.next.is_none() && self.start_at.is_none() {
            self.start_at = Some(self.start_time(&packet));
        }
        self.packets.insert(timestamp, packet);
        while self.packets.len() > CAPACITY {
            self.packets.pop_first();
        }
    }

    /// What plays for the next frame, which is needed at `now`.
    pub fn next_frame(&mut self, now: Instant) -> Playout {
        let Some(next) = self.next else {
            let starts = self.start_at.is_some_and(|start_at| now >= start_at);
            let first = self.packets.first_key_value().map(|(ts, _)| *ts);
            return match (starts, first) {
                (true, Some(first)) => {
                    self.start_at = None;
                    self.play(first)
                }
                _ => Playout::Silence,
            };
        };
        let next = self.trim().unwrap_or(next);
        self.play(next)
    }

    pub fn target_delay(&self) -> Duration {
        self.target
    }

    /// Audio waiting to play; gaps between talk spurts do not count.
    pub fn buffered(&self) -> Duration {
        samples_to_duration(self.buffered_samples())
    }

    fn buffered_samples(&self) -> u64 {
        self.packets.values().map(|packet| packet.samples).sum()
    }

    fn play(&mut self, next: u64) -> Playout {
        if let Some(packet) = self.packets.remove(&next) {
            self.concealed = 0;
            self.advance(next + packet.samples);
            return Playout::Frame(packet.payload);
        }
        let later = self
            .packets
            .range(next..)
            .next()
            .map(|(ts, packet)| (*ts, packet));
        match later {
            Some((later, packet)) if packet.marker || later - next > MAX_GAP => {
                // The talk spurt ended; the next one starts after the delay.
                self.start_at = Some(self.start_time(packet));
                self.next = None;
                Playout::Silence
            }
            Some((later, packet)) => {
                let recover = (later == next + FRAME as u64).then(|| packet.payload.clone());
                self.concealed = 0;
                self.advance(next + FRAME as u64);
                recover.map_or(Playout::Conceal, Playout::Recover)
            }
            None if self.concealed < MAX_CONCEALED => {
                self.concealed += 1;
                self.advance(next + FRAME as u64);
                Playout::Conceal
            }
            None => {
                self.next = None;
                Playout::Silence
            }
        }
    }

    fn advance(&mut self, to: u64) {
        self.next = Some(to);
        self.played_until = Some(to);
    }

    /// Cuts a backlog far beyond the delay, oldest first; returns where
    /// playing goes on.
    fn trim(&mut self) -> Option<u64> {
        if self.buffered() <= self.target + TRIM_ABOVE {
            return None;
        }
        let keep = duration_to_samples(self.target + TRIM_TO);
        let mut buffered = self.buffered_samples();
        while buffered > keep {
            let (_, packet) = self.packets.pop_first()?;
            buffered -= packet.samples;
        }
        let first = *self.packets.first_key_value()?.0;
        self.advance(first);
        Some(first)
    }

    /// When a talk spurt starting with `packet` plays: the delay after its
    /// arrival, less however late it already was.
    fn start_time(&self, packet: &Packet) -> Instant {
        let late_by = Duration::from_secs_f64((packet.delay - self.min_delay).max(0.0) / 1000.0);
        packet.arrived + self.target.saturating_sub(late_by)
    }

    /// The 32-bit RTP timestamp, made 64-bit across wrap-arounds.
    fn extend(&mut self, timestamp: u32) -> u64 {
        let extended = match self.highest {
            None => (1u64 << 32) | u64::from(timestamp),
            Some(highest) => {
                let delta = i64::from(timestamp.wrapping_sub(highest as u32) as i32);
                highest.saturating_add_signed(delta)
            }
        };
        if self.highest.is_none_or(|highest| extended > highest) {
            self.highest = Some(extended);
        }
        extended
    }

    /// Records how late a packet arrived for its timestamp, against the
    /// first one, and updates the target delay; returns the delay in ms.
    fn record_delay(&mut self, timestamp: u64, arrived: Instant) -> f64 {
        let (reference_arrival, reference_timestamp) =
            *self.reference.get_or_insert((arrived, timestamp));
        let since = arrived
            .saturating_duration_since(reference_arrival)
            .as_secs_f64()
            * 1000.0;
        let media =
            (timestamp as f64 - reference_timestamp as f64) * 1000.0 / f64::from(SAMPLE_RATE);
        let delay = since - media;
        if self.delays.len() == WINDOW {
            self.delays.pop_front();
        }
        self.delays.push_back(delay);

        self.scratch.clear();
        self.scratch.extend(self.delays.iter().copied());
        self.scratch.sort_by(f64::total_cmp);
        self.min_delay = self.scratch[0];
        let p95 = self.scratch[(self.scratch.len() * 95).div_ceil(100) - 1];
        let jitter = (p95 - self.min_delay).max(0.0);
        self.target =
            Duration::from_secs_f64((jitter + MARGIN_MS) / 1000.0).clamp(MIN_DELAY, MAX_DELAY);
        delay
    }
}

fn samples_to_duration(samples: u64) -> Duration {
    Duration::from_micros(samples * 1_000_000 / u64::from(SAMPLE_RATE))
}

fn duration_to_samples(duration: Duration) -> u64 {
    (duration.as_micros() as u64) * u64::from(SAMPLE_RATE) / 1_000_000
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::audio::FRAME;

    const BASE: u32 = 1_000_000;

    fn ts(n: u64) -> u32 {
        BASE.wrapping_add((n * FRAME as u64) as u32)
    }

    fn payload(n: u64) -> Arc<[u8]> {
        Arc::from(vec![0xf8, n as u8])
    }

    fn insert(buffer: &mut JitterBuffer, n: u64, at: Instant) {
        buffer.insert(ts(n), n == 0, payload(n), at);
    }

    /// What plays: `Some(n)` for frame n, `None` for silence.
    #[derive(Debug, PartialEq, Eq)]
    enum Heard {
        Frame(u8),
        Recovered(u8),
        Concealed,
        Silence,
    }

    fn heard(playout: Playout) -> Heard {
        match playout {
            Playout::Frame(payload) => Heard::Frame(payload[1]),
            Playout::Recover(next) => Heard::Recovered(next[1]),
            Playout::Conceal => Heard::Concealed,
            Playout::Silence => Heard::Silence,
        }
    }

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    #[test]
    fn packets_play_in_order_once_the_delay_has_passed() {
        let start = Instant::now();
        let mut buffer = JitterBuffer::new();
        for n in [2, 0, 1, 4, 3] {
            insert(&mut buffer, n, start);
        }

        let early = heard(buffer.next_frame(start));
        let played: Vec<Heard> = (0..5)
            .map(|_| heard(buffer.next_frame(start + MIN_DELAY)))
            .collect();

        assert_eq!(early, Heard::Silence);
        assert_eq!(played, (0..5).map(Heard::Frame).collect::<Vec<_>>());
    }

    #[test]
    fn a_lost_frame_comes_back_from_the_next_packet() {
        let start = Instant::now();
        let mut buffer = JitterBuffer::new();
        for n in [0, 1, 3] {
            insert(&mut buffer, n, start);
        }

        let played: Vec<Heard> = (0..4)
            .map(|_| heard(buffer.next_frame(start + MIN_DELAY)))
            .collect();

        assert_eq!(
            played,
            [
                Heard::Frame(0),
                Heard::Frame(1),
                Heard::Recovered(3),
                Heard::Frame(3)
            ]
        );
    }

    #[test]
    fn a_frame_with_nothing_right_after_it_is_concealed() {
        let start = Instant::now();
        let mut buffer = JitterBuffer::new();
        for n in [0, 1, 4] {
            insert(&mut buffer, n, start);
        }

        let played: Vec<Heard> = (0..5)
            .map(|_| heard(buffer.next_frame(start + MIN_DELAY)))
            .collect();

        assert_eq!(
            played,
            [
                Heard::Frame(0),
                Heard::Frame(1),
                Heard::Concealed,
                Heard::Recovered(4),
                Heard::Frame(4)
            ]
        );
    }

    #[test]
    fn packets_too_late_to_play_are_dropped() {
        let start = Instant::now();
        let mut buffer = JitterBuffer::new();
        insert(&mut buffer, 0, start);
        insert(&mut buffer, 1, start);
        let at = start + MIN_DELAY;
        buffer.next_frame(at);
        buffer.next_frame(at);
        buffer.next_frame(at);

        insert(&mut buffer, 2, at);
        insert(&mut buffer, 3, at);

        assert_eq!(heard(buffer.next_frame(at)), Heard::Frame(3));
    }

    #[test]
    fn steady_arrival_keeps_the_shortest_delay_and_jitter_raises_it() {
        let start = Instant::now();
        let mut steady = JitterBuffer::new();
        let mut jittery = JitterBuffer::new();
        for n in 0..100u64 {
            let on_time = start + ms(n * 20);
            steady.insert(ts(n), n == 0, payload(n), on_time);
            // Every fifth packet is 60 ms late.
            let late = if n % 5 == 4 { ms(60) } else { Duration::ZERO };
            jittery.insert(ts(n), n == 0, payload(n), on_time + late);
        }

        assert_eq!(steady.target_delay(), MIN_DELAY);
        let raised = jittery.target_delay();
        assert!(raised >= ms(70) && raised <= MAX_DELAY, "{raised:?}");
    }

    #[test]
    fn the_delay_never_exceeds_the_maximum() {
        let start = Instant::now();
        let mut buffer = JitterBuffer::new();
        for n in 0..100u64 {
            let late = if n % 2 == 1 { ms(500) } else { Duration::ZERO };
            buffer.insert(ts(n), n == 0, payload(n), start + ms(n * 20) + late);
        }

        assert_eq!(buffer.target_delay(), MAX_DELAY);
    }

    #[test]
    fn when_the_sender_stops_a_few_frames_are_concealed_then_silence() {
        let start = Instant::now();
        let mut buffer = JitterBuffer::new();
        insert(&mut buffer, 0, start);
        insert(&mut buffer, 1, start);

        let played: Vec<Heard> = (0..7)
            .map(|_| heard(buffer.next_frame(start + MIN_DELAY)))
            .collect();

        assert_eq!(
            played,
            [
                Heard::Frame(0),
                Heard::Frame(1),
                Heard::Concealed,
                Heard::Concealed,
                Heard::Concealed,
                Heard::Silence,
                Heard::Silence
            ]
        );
    }

    #[test]
    fn a_new_talk_spurt_waits_for_the_delay_again() {
        let start = Instant::now();
        let mut buffer = JitterBuffer::new();
        insert(&mut buffer, 0, start);
        let at = start + MIN_DELAY;
        assert_eq!(heard(buffer.next_frame(at)), Heard::Frame(0));
        for _ in 0..5 {
            buffer.next_frame(at);
        }

        // Two seconds later the sender talks again; its first packet is
        // marked.
        let again = start + ms(2_000);
        buffer.insert(ts(100), true, payload(100), again);
        buffer.insert(ts(101), false, payload(101), again);
        let too_soon = heard(buffer.next_frame(again));
        let on_time: Vec<Heard> = (0..2)
            .map(|_| heard(buffer.next_frame(again + MIN_DELAY)))
            .collect();

        assert_eq!(too_soon, Heard::Silence);
        assert_eq!(on_time, [Heard::Frame(100), Heard::Frame(101)]);
    }

    #[test]
    fn a_new_talk_spurt_ends_the_old_one_at_once() {
        let start = Instant::now();
        let mut buffer = JitterBuffer::new();
        insert(&mut buffer, 0, start);
        buffer.next_frame(start + MIN_DELAY);

        // The sender paused after frame 0 and talks again from frame 5,
        // marked, while frame 1 is being concealed.
        let again = start + ms(100);
        buffer.insert(ts(5), true, payload(5), again);
        let tail = heard(buffer.next_frame(again));
        let next = heard(buffer.next_frame(again + MIN_DELAY));

        assert_eq!(tail, Heard::Silence);
        assert_eq!(next, Heard::Frame(5));
    }

    #[test]
    fn a_long_gap_starts_a_new_talk_spurt_even_without_the_mark() {
        let start = Instant::now();
        let mut buffer = JitterBuffer::new();
        insert(&mut buffer, 0, start);
        buffer.next_frame(start + MIN_DELAY);

        let again = start + ms(1_000);
        buffer.insert(ts(50), false, payload(50), again);
        let tail = heard(buffer.next_frame(again));
        let next = heard(buffer.next_frame(again + MIN_DELAY));

        assert_eq!(tail, Heard::Silence);
        assert_eq!(next, Heard::Frame(50));
    }

    #[test]
    fn timestamps_wrap_around() {
        let start = Instant::now();
        let mut buffer = JitterBuffer::new();
        let first = u32::MAX - 959;
        for n in 0..4u32 {
            buffer.insert(
                first.wrapping_add(n * FRAME as u32),
                n == 0,
                Arc::from(vec![0xf8, n as u8]),
                start,
            );
        }

        let played: Vec<Heard> = (0..4)
            .map(|_| heard(buffer.next_frame(start + MIN_DELAY)))
            .collect();

        assert_eq!(played, (0..4).map(Heard::Frame).collect::<Vec<_>>());
    }

    #[test]
    fn a_backlog_after_a_network_stall_is_cut_short() {
        let start = Instant::now();
        let mut buffer = JitterBuffer::new();
        insert(&mut buffer, 0, start);
        let at = start + MIN_DELAY;
        buffer.next_frame(at);

        // A stall: 30 frames arrive at once.
        for n in 1..=30 {
            insert(&mut buffer, n, at);
        }
        let first = heard(buffer.next_frame(at));

        assert!(
            buffer.buffered() <= buffer.target_delay() + ms(40),
            "{:?}",
            buffer.buffered()
        );
        assert!(matches!(first, Heard::Frame(n) if n > 1), "{first:?}");
    }
}
