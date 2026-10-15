//! Systems without a video codec backend: video is neither sent nor
//! received.

/// Whether this system can encode and decode video.
pub fn supported() -> bool {
    false
}

pub mod encoder {
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
}

pub mod decoder {
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
}

pub mod scale {
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
}

pub mod raw {
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
}
