//! The short sounds voice events make on this device (Phase 2 §12): joining
//! and leaving, muting and deafening, a stream starting and stopping. They
//! are bundled like the default sounds and made the same way, by
//! `examples/cue_sounds.rs`.

/// A voice event with a sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cue {
    /// This device joined a voice channel.
    Join,
    /// This device left, or was disconnected.
    Leave,
    /// Someone else joined the channel this device is in.
    UserJoin,
    /// Someone else left it.
    UserLeave,
    Mute,
    Unmute,
    Deafen,
    Undeafen,
    /// Someone in the channel went live, this device included.
    StreamStart,
    StreamStop,
}

impl Cue {
    pub const ALL: [Self; 10] = [
        Self::Join,
        Self::Leave,
        Self::UserJoin,
        Self::UserLeave,
        Self::Mute,
        Self::Unmute,
        Self::Deafen,
        Self::Undeafen,
        Self::StreamStart,
        Self::StreamStop,
    ];

    /// Its file's name, without `.opus`.
    pub fn id(self) -> &'static str {
        match self {
            Self::Join => "join",
            Self::Leave => "leave",
            Self::UserJoin => "user-join",
            Self::UserLeave => "user-leave",
            Self::Mute => "mute",
            Self::Unmute => "unmute",
            Self::Deafen => "deafen",
            Self::Undeafen => "undeafen",
            Self::StreamStart => "stream-start",
            Self::StreamStop => "stream-stop",
        }
    }

    /// Its Ogg Opus file.
    pub fn file(self) -> &'static [u8] {
        match self {
            Self::Join => include_bytes!("../../sounds/cues/join.opus"),
            Self::Leave => include_bytes!("../../sounds/cues/leave.opus"),
            Self::UserJoin => include_bytes!("../../sounds/cues/user-join.opus"),
            Self::UserLeave => include_bytes!("../../sounds/cues/user-leave.opus"),
            Self::Mute => include_bytes!("../../sounds/cues/mute.opus"),
            Self::Unmute => include_bytes!("../../sounds/cues/unmute.opus"),
            Self::Deafen => include_bytes!("../../sounds/cues/deafen.opus"),
            Self::Undeafen => include_bytes!("../../sounds/cues/undeafen.opus"),
            Self::StreamStart => include_bytes!("../../sounds/cues/stream-start.opus"),
            Self::StreamStop => include_bytes!("../../sounds/cues/stream-stop.opus"),
        }
    }
}

#[cfg(test)]
mod tests {
    use opencord_common::sound::ogg::OpusFile;

    use super::*;
    use crate::soundboard::decode_mono;

    #[test]
    fn every_cue_is_bundled_short_and_audible() {
        for cue in Cue::ALL {
            let info = OpusFile::read(cue.file()).unwrap().info;
            assert!(
                (60..=900).contains(&info.duration_ms),
                "{}: {} ms",
                cue.id(),
                info.duration_ms
            );
            let samples = decode_mono(cue.file()).unwrap();
            let peak = samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()));
            assert!(peak > 0.05 && peak <= 1.0, "{}: peak {peak}", cue.id());
        }
    }
}
