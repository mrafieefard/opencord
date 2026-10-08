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
/// Client-chosen `SendMessage` nonce.
pub const NONCE_MAX_CHARS: usize = 64;

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

// Voice (Phase 2 plan §5, §11.2, §14).

/// Voice node registrations per IP address.
pub const VOICE_NODE_RATE: RateLimit = RateLimit {
    burst: 10,
    period: Duration::from_secs(60),
};

/// Voice state changes per user.
pub const VOICE_STATE_RATE: RateLimit = RateLimit {
    burst: 10,
    period: Duration::from_secs(10),
};

/// How long a voice state outlives a dropped gateway connection.
pub const VOICE_GRACE_PERIOD: Duration = Duration::from_secs(30);
pub const VOICE_TOKEN_LIFETIME: Duration = Duration::from_secs(60);
pub const MEDIA_TOKEN_LIFETIME: Duration = Duration::from_secs(24 * 60 * 60);

/// A voice channel's bitrate in bits per second: at least this, at most the
/// server's maximum.
pub const CHANNEL_BITRATE_MIN: u32 = 8_000;
pub const CHANNEL_BITRATE_DEFAULT: u32 = 64_000;
/// A voice channel's user limit; 0 means none.
pub const USER_LIMIT_MAX: u32 = 99;

/// The server-wide cap on voice bitrates.
pub const SERVER_BITRATE_MIN: u32 = 32_000;
pub const SERVER_BITRATE_MAX: u32 = 256_000;
pub const SERVER_BITRATE_DEFAULT: u32 = 96_000;
pub const SCREEN_SHARE_FPS_CHOICES: [u32; 3] = [15, 30, 60];
pub const SCREEN_SHARE_FPS_DEFAULT: u32 = 30;
pub const STREAM_VIEWERS_MAX: u32 = 200;
pub const STREAM_VIEWERS_DEFAULT: u32 = 50;
pub const CAMERA_PARTICIPANTS_MAX: u32 = 50;
pub const CAMERA_PARTICIPANTS_DEFAULT: u32 = 25;
pub const AFK_TIMEOUT_CHOICES_S: [u32; 5] = [60, 300, 900, 1_800, 3_600];
pub const AFK_TIMEOUT_DEFAULT_S: u32 = 300;
pub const SOUND_COOLDOWN_MAX_S: u32 = 30;
pub const SOUND_COOLDOWN_DEFAULT_S: u32 = 3;
pub const MAX_SOUNDS_LIMIT: u32 = 200;
pub const MAX_SOUNDS_DEFAULT: u32 = 48;
