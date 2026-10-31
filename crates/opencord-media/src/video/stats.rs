//! Counters for the session log (D82): what a video track sent or
//! received, written as one line every [`EVERY`].

use std::fmt::Write as _;
use std::time::{Duration, Instant};

/// How often a track's counters are written.
pub(crate) const EVERY: Duration = Duration::from_secs(5);

/// A sending track's counters since its last line.
pub(crate) struct ShareStats {
    started: Instant,
    since: Instant,
    first_sent: bool,
    captured: u32,
    sent: u32,
    dropped: Vec<(&'static str, u32)>,
    keyframes: u32,
    keyframe_requests: u32,
    encoding: Duration,
}

impl ShareStats {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            started: now,
            since: now,
            first_sent: false,
            captured: 0,
            sent: 0,
            dropped: Vec::new(),
            keyframes: 0,
            keyframe_requests: 0,
            encoding: Duration::ZERO,
        }
    }

    pub(crate) fn captured(&mut self) {
        self.captured += 1;
    }

    /// A frame encoded in `took` and sent.
    pub(crate) fn sent(&mut self, took: Duration, keyframe: bool) {
        self.sent += 1;
        self.encoding += took;
        self.keyframes += u32::from(keyframe);
    }

    pub(crate) fn dropped(&mut self, reason: &'static str) {
        match self.dropped.iter_mut().find(|(known, _)| *known == reason) {
            Some((_, count)) => *count += 1,
            None => self.dropped.push((reason, 1)),
        }
    }

    pub(crate) fn keyframe_requested(&mut self) {
        self.keyframe_requests += 1;
    }

    /// Milliseconds from the start to the first frame sent, the first
    /// time only.
    pub(crate) fn first_sent(&mut self, now: Instant) -> Option<u128> {
        if self.first_sent {
            return None;
        }
        self.first_sent = true;
        Some(now.saturating_duration_since(self.started).as_millis())
    }

    /// The line for the last [`EVERY`], once it has passed; the counters
    /// start again.
    pub(crate) fn due(&mut self, now: Instant) -> Option<String> {
        if now.saturating_duration_since(self.since) < EVERY {
            return None;
        }
        Some(self.line(now))
    }

    /// The counters since the last line, however short; for a track's end.
    pub(crate) fn line(&mut self, now: Instant) -> String {
        let over = now.saturating_duration_since(self.since);
        let dropped: u32 = self.dropped.iter().map(|(_, count)| count).sum();
        let mut line = format!(
            "captured={} sent={} dropped={dropped}",
            self.captured, self.sent
        );
        if !self.dropped.is_empty() {
            let reasons: Vec<String> = self
                .dropped
                .iter()
                .map(|(reason, count)| format!("{reason} {count}"))
                .collect();
            let _ = write!(line, " ({})", reasons.join(", "));
        }
        let encode_ms = if self.sent == 0 {
            0.0
        } else {
            self.encoding.as_secs_f64() * 1000.0 / f64::from(self.sent)
        };
        let _ = write!(
            line,
            " keyframes={} keyframe_requests={} encode_ms={encode_ms:.1} over_s={:.1}",
            self.keyframes,
            self.keyframe_requests,
            over.as_secs_f64()
        );
        let first_sent = self.first_sent;
        *self = Self::new(now);
        self.first_sent = first_sent;
        line
    }
}

/// A received track's counters since its last line.
pub(crate) struct ReceiveStats {
    since: Instant,
    received: u32,
    decoded: u32,
    decode_errors: u32,
    keyframe_requests: u32,
}

impl ReceiveStats {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            since: now,
            received: 0,
            decoded: 0,
            decode_errors: 0,
            keyframe_requests: 0,
        }
    }

    pub(crate) fn received(&mut self) {
        self.received += 1;
    }

    pub(crate) fn decoded(&mut self) {
        self.decoded += 1;
    }

    pub(crate) fn decode_error(&mut self) {
        self.decode_errors += 1;
    }

    /// A keyframe asked for, since what came before was lost.
    pub(crate) fn lost(&mut self) {
        self.keyframe_requests += 1;
    }

    pub(crate) fn due(&mut self, now: Instant) -> Option<String> {
        let over = now.saturating_duration_since(self.since);
        if over < EVERY {
            return None;
        }
        let line = format!(
            "received={} decoded={} decode_errors={} keyframe_requests={} over_s={:.1}",
            self.received,
            self.decoded,
            self.decode_errors,
            self.keyframe_requests,
            over.as_secs_f64()
        );
        *self = Self::new(now);
        Some(line)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    fn stats_line_counts_and_drops() {
        let start = Instant::now();
        let mut stats = ShareStats::new(start);
        for _ in 0..10 {
            stats.captured();
        }
        for _ in 0..8 {
            stats.sent(Duration::from_millis(4), false);
        }
        stats.dropped("encoder busy");
        stats.dropped("encoder busy");
        stats.sent(Duration::from_millis(4), true);
        stats.keyframe_requested();
        assert_eq!(stats.due(start + Duration::from_secs(1)), None);
        let line = stats.due(start + Duration::from_secs(5)).unwrap();
        assert_eq!(
            line,
            "captured=10 sent=9 dropped=2 (encoder busy 2) keyframes=1 \
             keyframe_requests=1 encode_ms=4.0 over_s=5.0"
        );
        // Counted afresh for the next five seconds.
        stats.captured();
        let next = stats.due(start + Duration::from_secs(10)).unwrap();
        assert!(next.starts_with("captured=1 sent=0 dropped=0 "), "{next}");
    }

    #[test]
    fn the_first_frame_is_reported_once() {
        let start = Instant::now();
        let mut stats = ShareStats::new(start);
        assert_eq!(
            stats.first_sent(start + Duration::from_millis(250)),
            Some(250)
        );
        assert_eq!(stats.first_sent(start + Duration::from_millis(300)), None);
    }

    #[test]
    fn receiving_stats_line() {
        let start = Instant::now();
        let mut stats = ReceiveStats::new(start);
        stats.received();
        stats.received();
        stats.decoded();
        stats.decode_error();
        stats.lost();
        let line = stats.due(start + Duration::from_secs(5)).unwrap();
        assert_eq!(
            line,
            "received=2 decoded=1 decode_errors=1 keyframe_requests=1 over_s=5.0"
        );
    }
}
