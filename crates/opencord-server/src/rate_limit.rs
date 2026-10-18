//! Per-user and per-IP rate limits (see `opencord_common::limits`).

use std::net::IpAddr;
use std::num::NonZeroU32;
use std::time::Duration;

use governor::clock::{Clock, DefaultClock};
use governor::{DefaultKeyedRateLimiter, Quota};
use opencord_common::limits::{
    COMMAND_AUTOCOMPLETE_RATE, IDENTIFY_RATE, MESSAGE_RATE, MUSIC_CONTROL_RATE, MUSIC_ENQUEUE_RATE,
    MUSIC_SEARCH_RATE, REQUEST_RATE, RateLimit, STREAM_CREATE_RATE, UPLOAD_CREATE_RATE,
    VOICE_NODE_RATE, VOICE_STATE_RATE,
};
use opencord_common::sound::EXTERNAL_UPLOAD_RATE;

#[derive(Debug)]
pub struct RateLimits {
    identify: DefaultKeyedRateLimiter<IpAddr>,
    requests: DefaultKeyedRateLimiter<i64>,
    messages: DefaultKeyedRateLimiter<(i64, i64)>,
    voice_states: DefaultKeyedRateLimiter<i64>,
    voice_nodes: DefaultKeyedRateLimiter<IpAddr>,
    streams: DefaultKeyedRateLimiter<i64>,
    external_sounds: DefaultKeyedRateLimiter<i64>,
    music: [DefaultKeyedRateLimiter<i64>; 3],
    autocomplete: DefaultKeyedRateLimiter<i64>,
    uploads: DefaultKeyedRateLimiter<i64>,
    clock: DefaultClock,
}

impl Default for RateLimits {
    fn default() -> Self {
        Self::with_identify_per_minute(IDENTIFY_RATE.burst)
    }
}

impl RateLimits {
    /// The usual limits, with this many sign-ins per address and minute
    /// (`[gateway] identify_per_minute`).
    pub fn with_identify_per_minute(identify: u32) -> Self {
        let identify = RateLimit {
            burst: identify.max(1),
            ..IDENTIFY_RATE
        };
        Self {
            identify: DefaultKeyedRateLimiter::keyed(quota(identify)),
            requests: DefaultKeyedRateLimiter::keyed(quota(REQUEST_RATE)),
            messages: DefaultKeyedRateLimiter::keyed(quota(MESSAGE_RATE)),
            voice_states: DefaultKeyedRateLimiter::keyed(quota(VOICE_STATE_RATE)),
            voice_nodes: DefaultKeyedRateLimiter::keyed(quota(VOICE_NODE_RATE)),
            streams: DefaultKeyedRateLimiter::keyed(quota(STREAM_CREATE_RATE)),
            external_sounds: DefaultKeyedRateLimiter::keyed(quota(EXTERNAL_UPLOAD_RATE)),
            music: [MUSIC_SEARCH_RATE, MUSIC_ENQUEUE_RATE, MUSIC_CONTROL_RATE]
                .map(|limit| DefaultKeyedRateLimiter::keyed(quota(limit))),
            autocomplete: DefaultKeyedRateLimiter::keyed(quota(COMMAND_AUTOCOMPLETE_RATE)),
            uploads: DefaultKeyedRateLimiter::keyed(quota(UPLOAD_CREATE_RATE)),
            clock: DefaultClock::default(),
        }
    }

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

    pub fn check_stream_create(&self, user_id: i64) -> Result<(), Duration> {
        self.streams
            .check_key(&user_id)
            .map_err(|not_until| not_until.wait_time_from(self.clock.now()))
    }

    /// Sounds from other servers uploaded here (plan §11.2).
    pub fn check_external_sound(&self, user_id: i64) -> Result<(), Duration> {
        self.external_sounds
            .check_key(&user_id)
            .map_err(|not_until| not_until.wait_time_from(self.clock.now()))
    }

    /// Music searches, additions, and playback and queue changes (plan
    /// §18.1).
    pub fn check_music(&self, kind: MusicLimit, user_id: i64) -> Result<(), Duration> {
        self.music[kind as usize]
            .check_key(&user_id)
            .map_err(|not_until| not_until.wait_time_from(self.clock.now()))
    }

    pub fn check_autocomplete(&self, user_id: i64) -> Result<(), Duration> {
        self.autocomplete
            .check_key(&user_id)
            .map_err(|not_until| not_until.wait_time_from(self.clock.now()))
    }

    pub fn check_upload(&self, user_id: i64) -> Result<(), Duration> {
        self.uploads
            .check_key(&user_id)
            .map_err(|not_until| not_until.wait_time_from(self.clock.now()))
    }

    /// Forgets keys whose limits have fully recovered.
    pub fn prune(&self) {
        self.identify.retain_recent();
        self.requests.retain_recent();
        self.messages.retain_recent();
        self.voice_states.retain_recent();
        self.voice_nodes.retain_recent();
        self.streams.retain_recent();
        self.external_sounds.retain_recent();
        for limiter in &self.music {
            limiter.retain_recent();
        }
        self.autocomplete.retain_recent();
        self.uploads.retain_recent();
    }
}

/// Which music limit a request counts toward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MusicLimit {
    Search = 0,
    Enqueue = 1,
    Control = 2,
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
