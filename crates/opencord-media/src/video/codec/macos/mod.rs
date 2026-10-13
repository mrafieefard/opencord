//! macOS's codec (docs/plans/macos.md M3, decisions B and C): VideoToolbox
//! encodes and decodes H.264, without FFmpeg; pic-scale and yuv scale and
//! convert in portable Rust.
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
pub(crate) mod ffi;
pub mod raw;
pub mod scale;

/// An NV12 Core Video pixel buffer (what VideoToolbox, AVFoundation and
/// ScreenCaptureKit give) as a packed picture, cut to even sizes.
///
/// # Safety
///
/// `image` is a valid pixel buffer for the call.
pub(crate) unsafe fn nv12_picture(
    image: ffi::Ref,
    captured: std::time::Instant,
) -> Option<crate::video::picture::Picture> {
    // SAFETY: the caller's promise.
    unsafe { decoder::picture_of(image, captured) }
}

/// Whether this system can encode and decode video: VideoToolbox's
/// software H.264 encoder and decoder are on every Mac. The first call
/// also starts loading the hardware encoder in the background.
pub fn supported() -> bool {
    encoder::warm_up();
    true
}
