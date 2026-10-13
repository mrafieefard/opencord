//! VideoToolbox H.264 decoding (M3): a stub until then.

use std::time::Instant;

use super::super::CodecError;
use crate::video::picture::Picture;

/// Never made: opening always fails here.
pub struct Decoder {
    _never: (),
}

impl Decoder {
    /// A decoder; in hardware if `hardware` and there is one.
    pub fn open(_hardware: bool) -> Result<Self, CodecError> {
        Err(CodecError::Unavailable("an H.264 decoder"))
    }

    pub fn is_hardware(&self) -> bool {
        unreachable!("no codec is ever opened here")
    }

    /// Decodes one access unit (Annex B NAL units); the pictures that came
    /// out of it, stamped `captured`. An error means the stream is damaged
    /// until the next keyframe.
    pub fn decode(
        &mut self,
        _nal_units: &[impl AsRef<[u8]>],
        _captured: Instant,
    ) -> Result<Vec<Picture>, CodecError> {
        unreachable!("no codec is ever opened here")
    }
}
