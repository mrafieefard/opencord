//! What cameras and screens deliver, turned into NV12 pictures at the size
//! they came in (docs/plans/macos.md M3, step 4): NV12 as it is, I420
//! repacked, YUYV, BGRx and RGBx through `convert.rs`. AVFoundation and
//! ScreenCaptureKit deliver NV12, so there is no JPEG decoder here.

use super::super::{CodecError, convert};
use super::scale::Scaler;
use crate::video::camera::{RawFormat, RawFrame};
use crate::video::picture::{Picture, PixelFormat};

/// Turns camera and screen frames into NV12 pictures of their own size;
/// one per capture thread.
#[derive(Default)]
pub struct RawDecoder {
    scaler: Scaler,
}

impl RawDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// The frame as an NV12 picture of its own size.
    pub fn picture(&mut self, raw: &RawFrame) -> Result<Picture, CodecError> {
        let (width, height) = (raw.width, raw.height);
        match raw.format {
            RawFormat::Nv12 => Picture::from_data(
                PixelFormat::Nv12,
                width,
                height,
                raw.data.clone(),
                raw.captured,
            )
            .ok_or(CodecError::WrongPicture),
            RawFormat::I420 => {
                let picture = Picture::from_data(
                    PixelFormat::I420,
                    width,
                    height,
                    raw.data.clone(),
                    raw.captured,
                )
                .ok_or(CodecError::WrongPicture)?;
                self.scaler
                    .picture(&picture, width, height, PixelFormat::Nv12)
            }
            RawFormat::Yuyv => {
                let row = width as usize * 2;
                let even = width.is_multiple_of(2) && height.is_multiple_of(2);
                if raw.data.len() != row * height as usize || width == 0 || height == 0 || !even {
                    return Err(CodecError::WrongPicture);
                }
                let data = convert::yuyv_to_nv12(&raw.data, width as usize, height as usize);
                Picture::from_data(PixelFormat::Nv12, width, height, data, raw.captured)
                    .ok_or(CodecError::WrongPicture)
            }
            RawFormat::Bgrx | RawFormat::Rgbx => {
                let stride = width as usize * 4;
                let (even_width, even_height) = (width & !1, height & !1);
                if raw.data.len() != stride * height as usize || even_width == 0 || even_height == 0
                {
                    return Err(CodecError::WrongPicture);
                }
                let convert = match raw.format {
                    RawFormat::Bgrx => convert::bgrx_to_nv12,
                    _ => convert::rgbx_to_nv12,
                };
                let data = convert(&raw.data, stride, even_width as usize, even_height as usize);
                Picture::from_data(
                    PixelFormat::Nv12,
                    even_width,
                    even_height,
                    data,
                    raw.captured,
                )
                .ok_or(CodecError::WrongPicture)
            }
            RawFormat::Mjpeg => Err(CodecError::Unavailable("a JPEG decoder")),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::video::pattern::scene;

    #[test]
    fn yuyv_becomes_nv12_with_its_luma_and_chroma() {
        // Two rows of four pixels: Y U Y V, with U 90 and V 160 everywhere.
        let mut data = Vec::new();
        for luma in [16u8, 60, 120, 200, 30, 70, 130, 210] {
            data.push(luma);
            data.push(if data.len() % 4 == 1 { 90 } else { 160 });
        }
        let frame = RawFrame {
            format: RawFormat::Yuyv,
            width: 4,
            height: 2,
            data,
            captured: Instant::now(),
        };
        let mut decoder = RawDecoder::new();

        let picture = decoder.picture(&frame).unwrap();

        assert_eq!(picture.format, PixelFormat::Nv12);
        assert_eq!(picture.y(), &[16, 60, 120, 200, 30, 70, 130, 210]);
        assert_eq!(&picture.data[8..], &[90, 160, 90, 160]);
        assert_eq!(picture.captured, frame.captured);
    }

    #[test]
    fn nv12_and_i420_pass_through_as_nv12() {
        let source = scene(64, 32, 1, Instant::now());
        let nv12 = RawFrame {
            format: RawFormat::Nv12,
            width: 64,
            height: 32,
            data: source.data.clone(),
            captured: source.captured,
        };
        let i420 = RawFrame {
            format: RawFormat::I420,
            data: Scaler::new()
                .picture(&source, 64, 32, PixelFormat::I420)
                .unwrap()
                .data,
            ..nv12.clone()
        };
        let mut decoder = RawDecoder::new();

        assert_eq!(decoder.picture(&nv12).unwrap().data, source.data);
        assert_eq!(decoder.picture(&i420).unwrap().data, source.data);
    }

    #[test]
    fn bgrx_becomes_video_range_nv12() {
        // White, then black: video range's 235 and 16.
        let frame = RawFrame {
            format: RawFormat::Bgrx,
            width: 2,
            height: 2,
            data: [[255u8, 255, 255, 0], [0, 0, 0, 0]].repeat(2).concat(),
            captured: Instant::now(),
        };

        let picture = RawDecoder::new().picture(&frame).unwrap();

        assert_eq!(picture.y(), &[235, 16, 235, 16]);
    }

    #[test]
    fn mjpeg_and_frames_of_the_wrong_length_are_refused() {
        let frame = RawFrame {
            format: RawFormat::Yuyv,
            width: 64,
            height: 32,
            data: vec![0; 100],
            captured: Instant::now(),
        };
        let jpeg = RawFrame {
            format: RawFormat::Mjpeg,
            ..frame.clone()
        };

        assert!(RawDecoder::new().picture(&frame).is_err());
        assert!(matches!(
            RawDecoder::new().picture(&jpeg),
            Err(CodecError::Unavailable(_))
        ));
    }
}
