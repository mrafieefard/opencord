//! H.264 encoding and decoding (plan §7.9): hardware encoders through
//! FFmpeg, Cisco's OpenH264 as the software fallback, FFmpeg's decoder.
//! Linux only for now; other systems get their backends with their
//! capture code.

pub mod annexb;
pub mod convert;
#[cfg(target_os = "linux")]
pub mod decoder;
#[cfg(target_os = "linux")]
pub mod encoder;
#[cfg(target_os = "linux")]
mod ffmpeg;
#[cfg(target_os = "linux")]
pub mod openh264;
#[cfg(target_os = "linux")]
pub mod raw;
#[cfg(target_os = "linux")]
pub mod scale;
#[cfg(target_os = "linux")]
mod vaapi;

/// Why a codec could not do something.
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("{0} is not available on this computer")]
    Unavailable(&'static str),
    #[error("FFmpeg: {0}")]
    Ffmpeg(String),
    #[error("OpenH264: {0}")]
    OpenH264(String),
    #[error("the picture is not the size the codec was set up for")]
    WrongPicture,
}

/// The NAL unit type of a NAL unit (ITU-T H.264 Table 7-1).
pub fn nal_type(unit: &[u8]) -> Option<u8> {
    unit.first().map(|header| header & 0x1f)
}

/// NAL unit types: an IDR slice, a sequence and a picture parameter set.
pub const NAL_IDR: u8 = 5;
pub const NAL_SPS: u8 = 7;
pub const NAL_PPS: u8 = 8;
