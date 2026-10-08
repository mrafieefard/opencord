//! Bitrates over the last second, and a token bucket for caps.

use std::time::{Duration, Instant};

const SLOT: Duration = Duration::from_millis(100);
const SLOTS: usize = 10;

/// Bytes counted in 100 ms slots across the last second.
#[derive(Debug, Clone, Default)]
pub(crate) struct RateMeter {
    /// When slot numbers start.
    origin: Option<Instant>,
    /// (slot number, bytes), indexed by slot number modulo [`SLOTS`].
    slots: [(u64, u64); SLOTS],
}

impl RateMeter {
    pub fn add(&mut self, now: Instant, bytes: usize) {
        let origin = *self.origin.get_or_insert(now);
        let slot = slot(origin, now);
        let entry = &mut self.slots[(slot % SLOTS as u64) as usize];
        if entry.0 != slot {
            *entry = (slot, 0);
        }
        entry.1 += bytes as u64;
    }

    /// Over the second before `now`.
    pub fn bits_per_second(&self, now: Instant) -> u64 {
        let Some(origin) = self.origin else {
            return 0;
        };
        let slot = slot(origin, now);
        let oldest = slot.saturating_sub(SLOTS as u64 - 1);
        let bytes: u64 = self
            .slots
            .iter()
            .filter(|(number, _)| (oldest..=slot).contains(number))
            .map(|(_, bytes)| bytes)
            .sum();
        bytes * 8
    }
}

/// Slot 0 is never used, so the zeroed slots count for nothing.
fn slot(origin: Instant, now: Instant) -> u64 {
    (now.saturating_duration_since(origin).as_millis() / SLOT.as_millis()) as u64 + 1
}

/// At most `rate` bits per second, with a second's worth of burst.
#[derive(Debug, Clone)]
pub(crate) struct TokenBucket {
    /// Bytes per second.
    rate: f64,
    tokens: f64,
    at: Instant,
}

impl TokenBucket {
    /// Starts full.
    pub fn new(now: Instant, bits_per_second: u64) -> Self {
        let rate = bits_per_second as f64 / 8.0;
        Self {
            rate,
            tokens: rate,
            at: now,
        }
    }

    pub fn set_rate(&mut self, bits_per_second: u64) {
        self.rate = bits_per_second as f64 / 8.0;
        self.tokens = self.tokens.min(self.rate);
    }

    /// Whether `bytes` fit now; they are taken if so.
    pub fn take(&mut self, now: Instant, bytes: usize) -> bool {
        let elapsed = now.saturating_duration_since(self.at).as_secs_f64();
        self.at = self.at.max(now);
        self.tokens = (self.tokens + elapsed * self.rate).min(self.rate);
        let bytes = bytes as f64;
        if bytes > self.tokens {
            return false;
        }
        self.tokens -= bytes;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_steady_stream_reads_its_bitrate() {
        let mut meter = RateMeter::default();
        let start = Instant::now();

        // 1250 bytes every 10 ms: 1 Mbit/s.
        for tick in 0..300u64 {
            meter.add(start + Duration::from_millis(tick * 10), 1250);
        }

        let rate = meter.bits_per_second(start + Duration::from_millis(3000));
        assert!((900_000..=1_100_000).contains(&rate), "{rate}");
    }

    #[test]
    fn a_stream_that_stopped_reads_zero_a_second_later() {
        let mut meter = RateMeter::default();
        let start = Instant::now();
        assert_eq!(meter.bits_per_second(start), 0);
        meter.add(start, 100_000);

        assert!(meter.bits_per_second(start + Duration::from_millis(500)) > 0);
        assert_eq!(
            meter.bits_per_second(start + Duration::from_millis(1200)),
            0
        );
    }

    #[test]
    fn a_bucket_allows_a_burst_then_its_rate() {
        // 8 kbit/s: 1000 bytes a second, and a second's worth of burst.
        let start = Instant::now();
        let mut bucket = TokenBucket::new(start, 8_000);

        assert!(bucket.take(start, 600));
        assert!(bucket.take(start, 400));
        assert!(!bucket.take(start, 1));
        assert!(!bucket.take(start + Duration::from_millis(400), 500));
        assert!(bucket.take(start + Duration::from_millis(500), 500));
    }

    #[test]
    fn a_bucket_never_saves_up_more_than_its_burst() {
        let start = Instant::now();
        let mut bucket = TokenBucket::new(start, 8_000);

        let later = start + Duration::from_secs(60);
        assert!(bucket.take(later, 1000));
        assert!(!bucket.take(later, 1));
    }

    #[test]
    fn a_bucket_can_change_its_rate() {
        let start = Instant::now();
        let mut bucket = TokenBucket::new(start, 8_000);
        assert!(bucket.take(start, 1000));

        bucket.set_rate(80_000);

        assert!(bucket.take(start + Duration::from_millis(100), 1000));
    }
}
