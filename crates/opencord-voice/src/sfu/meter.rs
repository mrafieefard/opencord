//! Bitrates over the last second (shared with clients), and a token
//! bucket for caps.

use std::time::Instant;

pub(crate) use opencord_common::rate::RateMeter;

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
    use std::time::Duration;

    use super::*;

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
