//! Soundboard sounds (Phase 2 plan §11): their limits, how a sound is
//! referred to, the default sounds every client has, and the Ogg Opus files
//! they are stored in.

use std::fmt;
use std::time::Duration;

use crate::limits::RateLimit;

pub mod defaults;
pub mod ogg;

/// The longest sound, plus the server's tolerance for encoders that round
/// up (plan §11.2).
pub const MAX_DURATION_MS: u32 = 5_000;
pub const DURATION_TOLERANCE_MS: u32 = 20;
/// The largest stored sound file.
pub const MAX_FILE_BYTES: usize = 256 * 1024;
/// The largest file a client accepts for trimming.
pub const MAX_INPUT_BYTES: usize = 10 * 1024 * 1024;
pub const NAME_MIN_CHARS: usize = 2;
pub const NAME_MAX_CHARS: usize = 32;
/// Emoji sequences are short; family and flag sequences are the longest.
pub const EMOJI_MAX_CHARS: usize = 8;
/// Percent.
pub const VOLUME_MAX: u32 = 100;
pub const VOLUME_DEFAULT: u32 = 100;
/// Sounds playing at once in one channel; more are refused with
/// `SOUND_COOLDOWN`.
pub const MAX_PLAYING_PER_CHANNEL: usize = 3;
/// What the server keeps of other servers' sounds that were played here.
pub const EXTERNAL_CACHE_BYTES: u64 = 50 * 1024 * 1024;
/// External sounds uploaded per user.
pub const EXTERNAL_UPLOAD_RATE: RateLimit = RateLimit {
    burst: 10,
    period: Duration::from_secs(60),
};
/// The client's cache across every server, deduplicated by SHA-256.
pub const CLIENT_CACHE_BYTES: u64 = 200 * 1024 * 1024;
/// What uploads are encoded at, and their loudness (EBU R128).
pub const UPLOAD_BITRATE: i32 = 96_000;
pub const NORMALIZE_LUFS: f64 = -18.0;
/// Sound downloads running at once, per server.
pub const DOWNLOADS_PER_SERVER: usize = 2;

/// Whether a stored sound plays within the limit, counting the longer of
/// what its header and its packets say.
pub fn within_duration(info: &ogg::OpusInfo) -> bool {
    info.duration_ms.max(info.decoded_ms) <= MAX_DURATION_MS + DURATION_TOLERANCE_MS
}

/// Which sound to play: `server:<sha256 hex>`, `external:<sha256 hex>` or
/// `default:<id>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SoundRef {
    /// One of this server's sounds.
    Server([u8; 32]),
    /// A sound from another server the player is in.
    External([u8; 32]),
    /// A sound every client ships with.
    Default(&'static defaults::DefaultSound),
}

impl SoundRef {
    pub fn parse(value: &str) -> Option<Self> {
        let (kind, rest) = value.split_once(':')?;
        match kind {
            "server" => sha256(rest).map(Self::Server),
            "external" => sha256(rest).map(Self::External),
            "default" => defaults::by_id(rest).map(Self::Default),
            _ => None,
        }
    }

    /// The file's hash, for sounds that are files on a server.
    pub fn sha256(&self) -> Option<&[u8; 32]> {
        match self {
            Self::Server(hash) | Self::External(hash) => Some(hash),
            Self::Default(_) => None,
        }
    }
}

impl fmt::Display for SoundRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Server(hash) => write!(f, "server:{}", hex::encode(hash)),
            Self::External(hash) => write!(f, "external:{}", hex::encode(hash)),
            Self::Default(sound) => write!(f, "default:{}", sound.id),
        }
    }
}

/// A lowercase hex SHA-256.
pub fn sha256(hex_value: &str) -> Option<[u8; 32]> {
    let lowercase = hex_value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if hex_value.len() != 64 || !lowercase {
        return None;
    }
    let mut hash = [0; 32];
    hex::decode_to_slice(hex_value, &mut hash).ok()?;
    Some(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_refs_round_trip() {
        let hash = [0xab; 32];
        for sound in [
            SoundRef::Server(hash),
            SoundRef::External(hash),
            SoundRef::Default(&defaults::DEFAULT_SOUNDS[0]),
        ] {
            assert_eq!(SoundRef::parse(&sound.to_string()), Some(sound));
        }
    }

    #[test]
    fn malformed_sound_refs_are_refused() {
        let hex = "ab".repeat(32);
        for bad in [
            "",
            "server",
            "server:",
            "server:abc",
            &format!("server:{}", hex.to_uppercase()),
            &format!("other:{hex}"),
            &format!("server:{hex}0"),
            "default:no-such-sound",
        ] {
            assert_eq!(SoundRef::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_longer_count_decides_the_duration() {
        let info = |duration_ms, decoded_ms| ogg::OpusInfo {
            channels: 2,
            pre_skip: 312,
            duration_ms,
            decoded_ms,
        };

        assert!(within_duration(&info(5_000, 5_020)));
        assert!(!within_duration(&info(5_021, 5_021)));
        assert!(!within_duration(&info(1_000, 6_000)));
    }
}
