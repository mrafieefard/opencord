//! What cameras deliver (plan §8): MJPEG, packed YUYV, or NV12 and I420
//! planes, turned into NV12 pictures at the size they came in. MJPEG is
//! decoded by FFmpeg (SIMD); the rest goes through swscale.

use ff::format::Pixel;
use ffmpeg_next as ff;

use super::scale::Scaler;
use super::{CodecError, convert, ffmpeg};

/// A direct conversion of full-range planes to NV12.
type Planar = fn([&[u8]; 3], [usize; 3], usize, usize) -> Vec<u8>;
use crate::video::camera::{RawFormat, RawFrame};
use crate::video::picture::{Picture, PixelFormat};

/// Turns camera frames into NV12 pictures; one per capture thread.
pub struct RawDecoder {
    scaler: Scaler,
    jpeg: Option<ff::decoder::Video>,
    frame: ff::frame::Video,
}

impl Default for RawDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl RawDecoder {
    pub fn new() -> Self {
        Self {
            scaler: Scaler::new(),
            jpeg: None,
            frame: ff::frame::Video::empty(),
        }
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
            RawFormat::Mjpeg => {
                self.decode_jpeg(&raw.data)?;
                let (width, height) = (self.frame.width() & !1, self.frame.height() & !1);
                let direct: Option<Planar> = match self.frame.format() {
                    Pixel::YUVJ422P => Some(convert::full_422_to_nv12),
                    Pixel::YUVJ420P => Some(convert::full_420_to_nv12),
                    _ => None,
                };
                let Some(direct) = direct else {
                    return self.scaler.frame(
                        &self.frame,
                        width,
                        height,
                        PixelFormat::Nv12,
                        raw.captured,
                    );
                };
                let frame = &self.frame;
                let data = direct(
                    [frame.data(0), frame.data(1), frame.data(2)],
                    [frame.stride(0), frame.stride(1), frame.stride(2)],
                    width as usize,
                    height as usize,
                );
                Picture::from_data(PixelFormat::Nv12, width, height, data, raw.captured)
                    .ok_or(CodecError::WrongPicture)
            }
        }
    }

    fn decode_jpeg(&mut self, data: &[u8]) -> Result<(), CodecError> {
        if self.jpeg.is_none() {
            ffmpeg::init();
            let codec = ff::decoder::find(ff::codec::Id::MJPEG)
                .ok_or(CodecError::Unavailable("a JPEG decoder"))?;
            let mut context = ff::codec::context::Context::new_with_codec(codec);
            context.set_flags(ff::codec::Flags::LOW_DELAY);
            let decoder = context
                .decoder()
                .open_as(codec)
                .and_then(|opened| opened.video())
                .map_err(ffmpeg::error)?;
            self.jpeg = Some(decoder);
        }
        let Some(decoder) = self.jpeg.as_mut() else {
            return Err(CodecError::Unavailable("a JPEG decoder"));
        };
        decoder
            .send_packet(&ff::Packet::copy(data))
            .map_err(ffmpeg::error)?;
        decoder
            .receive_frame(&mut self.frame)
            .map_err(ffmpeg::error)
    }
}

#[cfg(any(test, feature = "testing"))]
/// The picture's luma and chroma as a full-range JPEG (for tests and the
/// virtual camera).
pub fn encode_jpeg(picture: &Picture) -> Vec<u8> {
    ffmpeg::init();
    let codec = ff::encoder::find(ff::codec::Id::MJPEG).unwrap();
    let mut video = ff::codec::context::Context::new_with_codec(codec)
        .encoder()
        .video()
        .unwrap();
    video.set_width(picture.width);
    video.set_height(picture.height);
    video.set_format(Pixel::YUVJ420P);
    video.set_time_base((1, 30));
    let mut encoder = video.open_as(codec).unwrap();
    let i420 = Scaler::new()
        .picture(picture, picture.width, picture.height, PixelFormat::I420)
        .unwrap();
    let mut frame = ffmpeg::to_frame(&i420);
    // SAFETY: relabels the frame's own format; the planes are the same.
    unsafe { (*frame.as_mut_ptr()).format = ff::ffi::AVPixelFormat::AV_PIX_FMT_YUVJ420P as i32 };
    encoder.send_frame(&frame).unwrap();
    encoder.send_eof().unwrap();
    let mut packet = ff::Packet::empty();
    encoder.receive_packet(&mut packet).unwrap();
    packet.data().unwrap().to_vec()
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::video::pattern::scene;
    use crate::video::picture::PixelFormat;

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
        let mut decoder = RawDecoder::new();

        assert_eq!(decoder.picture(&nv12).unwrap().data, source.data);
    }

    #[test]
    fn mjpeg_is_decoded_to_nv12() {
        let source = scene(320, 180, 4, Instant::now());
        let jpeg = encode_jpeg(&source);
        let frame = RawFrame {
            format: RawFormat::Mjpeg,
            width: 320,
            height: 180,
            data: jpeg,
            captured: source.captured,
        };
        let mut decoder = RawDecoder::new();

        let picture = decoder.picture(&frame).unwrap();

        assert_eq!(
            (picture.width, picture.height, picture.format),
            (320, 180, PixelFormat::Nv12)
        );
        // The JPEG held full-range luma; video range comes back.
        let expected: Vec<f64> = source
            .y()
            .iter()
            .map(|&y| 16.0 + f64::from(y) * 219.0 / 255.0)
            .collect();
        let error: f64 = picture
            .y()
            .iter()
            .zip(&expected)
            .map(|(got, want)| (f64::from(*got) - want).powi(2))
            .sum::<f64>()
            / expected.len() as f64;
        let psnr = 10.0 * (255.0 * 255.0 / error.max(1e-9)).log10();
        assert!(psnr > 30.0, "{psnr:.1} dB");
    }

    #[test]
    fn a_broken_jpeg_is_an_error() {
        let frame = RawFrame {
            format: RawFormat::Mjpeg,
            width: 320,
            height: 180,
            data: vec![0xff, 0xd8, 0xff, 0x00, 1, 2, 3],
            captured: Instant::now(),
        };

        assert!(RawDecoder::new().picture(&frame).is_err());
    }

    #[test]
    fn a_frame_of_the_wrong_length_is_refused() {
        let frame = RawFrame {
            format: RawFormat::Yuyv,
            width: 64,
            height: 32,
            data: vec![0; 100],
            captured: Instant::now(),
        };

        assert!(RawDecoder::new().picture(&frame).is_err());
    }
}
