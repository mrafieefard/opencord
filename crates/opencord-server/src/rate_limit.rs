//! Per-user and per-IP rate limits (see `opencord_common::limits`).

use std::net::IpAddr;
use std::num::NonZeroU32;
use std::time::Duration;

use governor::clock::{Clock, DefaultClock};
use governor::{DefaultKeyedRateLimiter, Quota};
use opencord_common::limits::{
    IDENTIFY_RATE, MESSAGE_RATE, REQUEST_RATE, RateLimit, VOICE_NODE_RATE, VOICE_STATE_RATE,
};

#[derive(Debug)]
pub struct RateLimits {
    identify: DefaultKeyedRateLimiter<IpAddr>,
    requests: DefaultKeyedRateLimiter<i64>,
    messages: DefaultKeyedRateLimiter<(i64, i64)>,
    voice_states: DefaultKeyedRateLimiter<i64>,
    voice_nodes: DefaultKeyedRateLimiter<IpAddr>,
    clock: DefaultClock,
}

impl Default for RateLimits {
    fn default() -> Self {
        Self {
            identify: DefaultKeyedRateLimiter::keyed(quota(IDENTIFY_RATE)),
            requests: DefaultKeyedRateLimiter::keyed(quota(REQUEST_RATE)),
            messages: DefaultKeyedRateLimiter::keyed(quota(MESSAGE_RATE)),
            voice_states: DefaultKeyedRateLimiter::keyed(quota(VOICE_STATE_RATE)),
            voice_nodes: DefaultKeyedRateLimiter::keyed(quota(VOICE_NODE_RATE)),
            clock: DefaultClock::default(),
        }
    }
}

impl RateLimits {
    /// `Err` holds how long to wait.
    pub fn check_identify(&self, ip: IpAddr) -> Result<(), Duration> {
        self.identify
            .check_key(&ip)
            .map_err(|not_until| not_until.wait_time_from(self.clock.now()))
    }

    pub fn check_request(&self, user_id: i64) -> Result<(), Duration> {
        self.requests
            .check_key(&user_id)
            .map_err(|not_until| not_until.wait_time_from(self.clock.now()))
    }

    pub fn check_message(&self, user_id: i64, channel_id: i64) -> Result<(), Duration> {
        self.messages
            .check_key(&(user_id, channel_id))
            .map_err(|not_until| not_until.wait_time_from(self.clock.now()))
    }

    pub fn check_voice_state(&self, user_id: i64) -> Result<(), Duration> {
        self.voice_states
            .check_key(&user_id)
            .map_err(|not_until| not_until.wait_time_from(self.clock.now()))
    }

    pub fn check_voice_node(&self, ip: IpAddr) -> Result<(), Duration> {
        self.voice_nodes
            .check_key(&ip)
            .map_err(|not_until| not_until.wait_time_from(self.clock.now()))
    }

    /// Forgets keys whose limits have fully recovered.
    pub fn prune(&self) {
        self.identify.retain_recent();
        self.requests.retain_recent();
        self.messages.retain_recent();
        self.voice_states.retain_recent();
        self.voice_nodes.retain_recent();
    }
}

fn quota(limit: RateLimit) -> Quota {
    let burst = NonZeroU32::new(limit.burst).expect("rate limits allow at least one event");
    Quota::with_period(limit.period / limit.burst)
        .expect("rate limit periods are non-zero")
        .allow_burst(burst)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identify_allows_a_burst_then_limits() {
        let limits = RateLimits::default();
        let ip = IpAddr::from([10, 0, 0, 1]);
        let other = IpAddr::from([10, 0, 0, 2]);

        for _ in 0..IDENTIFY_RATE.burst {
            assert!(limits.check_identify(ip).is_ok());
        }

        let wait = limits.check_identify(ip).unwrap_err();
        assert!(wait > Duration::ZERO && wait <= IDENTIFY_RATE.period);
        assert!(limits.check_identify(other).is_ok());
    }

    #[test]
    fn messages_are_limited_per_channel() {
        let limits = RateLimits::default();
        for _ in 0..MESSAGE_RATE.burst {
            assert!(limits.check_message(1, 10).is_ok());
        }

        assert!(limits.check_message(1, 10).is_err());
        assert!(limits.check_message(1, 11).is_ok());
    }
}
