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

/// The str0m configuration both ends of a voice connection start from: RTP
/// mode, Opus, H.264 with retransmission on the shared payload types, and
/// the frame-marking extension. Nothing is negotiated, so both ends must
/// build it the same way.
#[cfg(feature = "str0m")]
pub fn rtc_config() -> str0m::RtcConfig {
    use crate::video::{
        FRAME_MARKING_ID, H264_PAYLOAD_TYPE, H264_RTX_PAYLOAD_TYPE, frame_marking_extension,
    };

    /// Constrained Baseline, level 3.1: what the payload type advertises.
    const H264_PROFILE_LEVEL: u32 = 0x42e01f;

    let mut config = str0m::RtcConfig::new()
        .set_rtp_mode(true)
        .clear_codecs()
        .enable_opus(true, false)
        .set_extension(FRAME_MARKING_ID, frame_marking_extension());
    config.codec_config().add_h264(
        H264_PAYLOAD_TYPE.into(),
        Some(H264_RTX_PAYLOAD_TYPE.into()),
        true,
        H264_PROFILE_LEVEL,
    );
    config
}

/// Speaking flags (Discord's values).
pub mod speaking {
    pub const MICROPHONE: u32 = 1;
    pub const SOUNDSHARE: u32 = 2;
    pub const PRIORITY: u32 = 4;
}

/// Voice gateway close codes.
pub mod close {
    pub const INVALID_FRAME: u16 = 4001;
    /// The voice token was refused: get a new one by joining again.
    pub const AUTHENTICATION_FAILED: u16 = 4003;
    pub const HANDSHAKE_TIMEOUT: u16 = 4004;
    /// Resumable.
    pub const HEARTBEAT_TIMEOUT: u16 = 4005;
    /// The voice session is gone: join again for a new token.
    pub const SESSION_INVALID: u16 = 4006;
    /// The same user connected again; this connection is over.
    /// Too many messages; resumable after a pause.
    pub const RATE_LIMITED: u16 = 4008;
    pub const SESSION_REPLACED: u16 = 4009;
    /// Left, moved or disconnected by the main server: do not reconnect.
    pub const DISCONNECTED: u16 = 4014;
    /// The node is going away: wait for the main server to say where next.
    pub const NODE_SHUTDOWN: u16 = 4015;

    /// Whether the client may resume its voice session after this close.
    pub fn is_resumable(code: Option<u16>) -> bool {
        !matches!(
            code,
            Some(
                AUTHENTICATION_FAILED
                    | SESSION_INVALID
                    | SESSION_REPLACED
                    | DISCONNECTED
                    | NODE_SHUTDOWN
            )
        )
    }
}

/// Above this many people in a channel, only the loudest few are heard.
pub const LOUDEST_ONLY_ABOVE: usize = 50;
/// How many are heard then.
pub const LOUDEST_HEARD: usize = 10;

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "str0m")]
    #[test]
    fn both_ends_agree_on_codecs_and_extensions() {
        use crate::video::{
            FRAME_MARKING_ID, FRAME_MARKING_URI, H264_PAYLOAD_TYPE, H264_RTX_PAYLOAD_TYPE,
        };
        use str0m::format::Codec;

        let mut config = rtc_config();

        assert!(config.rtp_mode());
        let params = config.codec_config().params().to_vec();
        let opus = params
            .iter()
            .find(|p| p.spec().codec == Codec::Opus)
            .unwrap();
        let h264 = params
            .iter()
            .find(|p| p.spec().codec == Codec::H264)
            .unwrap();
        assert_eq!(*opus.pt(), OPUS_PAYLOAD_TYPE);
        assert_eq!(*h264.pt(), H264_PAYLOAD_TYPE);
        assert_eq!(h264.resend().map(|pt| *pt), Some(H264_RTX_PAYLOAD_TYPE));
        assert_eq!(params.len(), 2);
        let marking = config.extension_map().lookup(FRAME_MARKING_ID).unwrap();
        assert_eq!(marking.as_uri(), FRAME_MARKING_URI);
    }

    #[test]
    fn receive_mids_fit_a_mid_and_differ_from_the_send_mid() {
        let mid = receive_mid(u32::MAX);

        assert_eq!(mid, "uffffffff");
        assert!(mid.len() <= 16);
        assert_ne!(receive_mid(0), AUDIO_MID);
    }
}
