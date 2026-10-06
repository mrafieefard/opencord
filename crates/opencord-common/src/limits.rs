//! Size and rate limits enforced by the server.

use std::time::Duration;

pub const MESSAGE_MAX_CHARS: usize = 4_000;
/// Display names and nicknames.
pub const NAME_MAX_CHARS: usize = 32;
pub const CHANNEL_NAME_MAX_CHARS: usize = 100;
pub const ROLE_NAME_MAX_CHARS: usize = 100;
pub const TOPIC_MAX_CHARS: usize = 1_024;
pub const SERVER_NAME_MAX_CHARS: usize = 100;
pub const SERVER_DESCRIPTION_MAX_CHARS: usize = 1_000;
/// Kick and ban reasons.
pub const REASON_MAX_CHARS: usize = 512;

/// Roles per server, including @everyone.
pub const MAX_ROLES: usize = 250;
pub const MAX_CHANNELS: usize = 500;

pub const FETCH_LIMIT_DEFAULT: u32 = 50;
pub const FETCH_LIMIT_MAX: u32 = 100;

/// At most `burst` events per `period`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimit {
    pub burst: u32,
    pub period: Duration,
}

/// Messages per user per channel.
pub const MESSAGE_RATE: RateLimit = RateLimit {
    burst: 5,
    period: Duration::from_secs(5),
};

/// Requests of any kind per user.
pub const REQUEST_RATE: RateLimit = RateLimit {
    burst: 50,
    period: Duration::from_secs(10),
};

/// Identify attempts per IP address.
pub const IDENTIFY_RATE: RateLimit = RateLimit {
    burst: 5,
    period: Duration::from_secs(60),
};
