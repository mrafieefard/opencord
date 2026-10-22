//! H.264 encoding and decoding (plan §7.9), and scaling and converting
//! pictures. Each system has its codec backend (docs/plans/platforms.md),
//! all with one API, which the video sender and receiver use: `supported()`,
//! `encoder::{Backend, Encoder}`, `decoder::Decoder`, `scale::Scaler` and
//! `raw::RawDecoder`, with the signatures of the FFmpeg files here. Linux and
//! Windows use hardware encoders through FFmpeg, Cisco's OpenH264 as the
//! software fallback and FFmpeg's decoder.

pub mod annexb;
pub mod convert;

// One block per system: each session edits only its own.

// FFmpeg: Linux and Windows (docs/plans/windows.md W3). The hardware device
// differs — VA-API on Linux, D3D11VA on Windows — and so do the encoders it
// offers; everything else is the same code.
#[cfg(windows)]
mod d3d11va;
#[cfg(any(target_os = "linux", windows))]
pub mod decoder;
#[cfg(target_os = "linux")]
pub mod dmabuf;
#[cfg(any(target_os = "linux", windows))]
pub mod encoder;
#[cfg(any(target_os = "linux", windows))]
mod ffmpeg;
#[cfg(any(target_os = "linux", windows))]
pub mod openh264;
#[cfg(any(target_os = "linux", windows))]
pub mod raw;
#[cfg(any(target_os = "linux", windows))]
pub mod scale;
#[cfg(target_os = "linux")]
mod vaapi;
/// Whether this system can encode and decode video.
#[cfg(any(target_os = "linux", windows))]
pub fn supported() -> bool {
    true
}

// VideoToolbox (docs/plans/macos.md M3).
#[cfg(target_os = "macos")]
pub(crate) mod macos;
#[cfg(target_os = "macos")]
pub use macos::{decoder, encoder, raw, scale, supported};

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod unsupported;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub use unsupported::{decoder, encoder, raw, scale, supported};

/// Ends a test early where this system has no codec yet, so tests that
/// need one skip rather than fail there (docs/plans/platforms.md §2.6).
#[cfg(test)]
macro_rules! needs_codec {
    () => {
        if !$crate::video::codec::supported() {
            eprintln!("skipped: no video codec on this system yet");
            return;
        }
    };
}
#[cfg(test)]
pub(crate) use needs_codec;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncoderConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// Bits per second.
    pub bitrate: u32,
}

/// One encoded picture.
#[derive(Debug, Clone)]
pub struct Encoded {
    pub nal_units: Vec<Vec<u8>>,
    /// An IDR picture: decodable on its own.
    pub keyframe: bool,
}

/// The NAL unit type of a NAL unit (ITU-T H.264 Table 7-1).
pub fn nal_type(unit: &[u8]) -> Option<u8> {
    unit.first().map(|header| header & 0x1f)
}

/// NAL unit types: an IDR slice, a sequence and a picture parameter set.
pub const NAL_IDR: u8 = 5;
pub const NAL_SPS: u8 = 7;
pub const NAL_PPS: u8 = 8;
