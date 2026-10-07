//! What clients and voice nodes agree on for media (Phase 2 plan §6, §7),
//! without depending on each other.

/// RTP payload type of Opus on every voice connection.
pub const OPUS_PAYLOAD_TYPE: u8 = 111;

/// The media a client sends its microphone on.
pub const AUDIO_MID: &str = "a";

/// The media a client receives `ssrc`'s audio on: one per other speaker.
pub fn receive_mid(ssrc: u32) -> String {
    format!("u{ssrc:08x}")
}

/// Speaking flags (Discord's values).
pub mod speaking {
    pub const MICROPHONE: u32 = 1;
    pub const SOUNDSHARE: u32 = 2;
    pub const PRIORITY: u32 = 4;
}

/// Above this many people in a channel, only the loudest few are heard.
pub const LOUDEST_ONLY_ABOVE: usize = 50;
/// How many are heard then.
pub const LOUDEST_HEARD: usize = 10;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receive_mids_fit_a_mid_and_differ_from_the_send_mid() {
        let mid = receive_mid(u32::MAX);

        assert_eq!(mid, "uffffffff");
        assert!(mid.len() <= 16);
        assert_ne!(receive_mid(0), AUDIO_MID);
    }
}
