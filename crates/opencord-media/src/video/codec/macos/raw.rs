//! Camera and screen frames into NV12 pictures (M3): a stub until then.

use super::super::CodecError;
use crate::video::camera::RawFrame;
use crate::video::picture::Picture;

/// Turns camera and screen frames into NV12 pictures of their own size.
#[derive(Default)]
pub struct RawDecoder;

impl RawDecoder {
    pub fn new() -> Self {
        Self
    }

    pub fn picture(&mut self, _raw: &RawFrame) -> Result<Picture, CodecError> {
        Err(CodecError::Unavailable("a picture converter"))
    }
}
