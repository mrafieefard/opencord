//! VideoToolbox H.264 encoding (M3): a stub until then.

use super::super::CodecError;
pub use super::super::{Encoded, EncoderConfig};
use crate::video::picture::Picture;

/// The encoders this system has, hardware first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// None: nothing opens here.
    Unavailable,
}

impl Backend {
    pub const ALL: [Backend; 0] = [];

    pub fn is_hardware(self) -> bool {
        false
    }
}

/// Never made: opening always fails here.
pub struct Encoder {
    _never: (),
}

impl Encoder {
    /// The first of `backends` that opens for this configuration.
    pub fn open(_config: EncoderConfig, _backends: &[Backend]) -> Result<Self, CodecError> {
        Err(CodecError::Unavailable("an H.264 encoder"))
    }

    pub fn backend(&self) -> Backend {
        unreachable!("no codec is ever opened here")
    }

    pub fn config(&self) -> EncoderConfig {
        unreachable!("no codec is ever opened here")
    }

    /// Encodes a picture of the configured size; an IDR picture led by its
    /// SPS and PPS when `keyframe` is set. Every picture comes out of its
    /// own call.
    pub fn encode(
        &mut self,
        _picture: &Picture,
        _keyframe: bool,
    ) -> Result<Option<Encoded>, CodecError> {
        unreachable!("no codec is ever opened here")
    }
}
