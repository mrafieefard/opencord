//! macOS's codec: not written yet (docs/plans/macos.md M3: VideoToolbox,
//! no FFmpeg). Until it is, video is neither sent nor received on macOS.
//!
//! The contract is the FFmpeg files' API, which the video sender and
//! receiver use unchanged: pictures are NV12 or I420 at video range
//! ([`crate::video::picture`]); an encoder hands every picture back from
//! its own `encode` call, as Annex B NAL units with the SPS and PPS before
//! each IDR; a decoder takes Annex B access units and gives back NV12; the
//! scaler and the raw decoder convert with BT.601 at video range, as the
//! FFmpeg ones do.

pub mod decoder;
pub mod encoder;
pub mod raw;
pub mod scale;

/// Whether this system can encode and decode video.
pub fn supported() -> bool {
    false
}
