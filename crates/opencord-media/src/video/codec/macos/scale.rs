//! Scaling and converting in portable Rust (M3, decision C): a stub until
//! then.

use super::super::CodecError;
use crate::video::picture::{Picture, PixelFormat};

/// Scales and converts pictures: BT.601, video range, as the FFmpeg
/// scaler does.
#[derive(Default)]
pub struct Scaler;

impl Scaler {
    pub fn new() -> Self {
        Self
    }

    /// `input` at `width` × `height` in `format`.
    pub fn picture(
        &mut self,
        _input: &Picture,
        _width: u32,
        _height: u32,
        _format: PixelFormat,
    ) -> Result<Picture, CodecError> {
        Err(CodecError::Unavailable("a scaler"))
    }

    /// `input` at `width` × `height` as RGBA rows.
    pub fn rgba(
        &mut self,
        _input: &Picture,
        _width: u32,
        _height: u32,
    ) -> Result<Vec<u8>, CodecError> {
        Err(CodecError::Unavailable("a scaler"))
    }
}
