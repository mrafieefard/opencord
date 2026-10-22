//! Scaling and colour conversion on the CPU through FFmpeg's swscale (SIMD):
//! a camera picture to each layer's size, a decoded picture to RGBA at the
//! size of its tile (plan §7.8, §7.11). BT.601, video range.

use ff::format::Pixel;
use ff::software::scaling::{Context, Flags};
use ffmpeg_next as ff;

use super::{CodecError, ffmpeg};
use crate::video::picture::{Picture, PixelFormat};

/// Conversions it has set up, from and to: format, width, height.
type Shape = (Pixel, u32, u32, Pixel, u32, u32);

/// swscale contexts by shape; one per thread that converts.
#[derive(Default)]
pub struct Scaler {
    contexts: Vec<(Shape, Context)>,
}

// SAFETY: a swscale context belongs to whoever holds the scaler and is used
// by one thread at a time (`&mut self`); moving it to another thread is fine.
unsafe impl Send for Scaler {}

impl Scaler {
    pub fn new() -> Self {
        Self::default()
    }

    fn context(&mut self, shape: Shape) -> Result<&mut Context, CodecError> {
        let position = match self.contexts.iter().position(|(known, _)| *known == shape) {
            Some(position) => position,
            None => {
                ffmpeg::init();
                let (from, width, height, to, out_width, out_height) = shape;
                let context = Context::get(
                    from,
                    width,
                    height,
                    to,
                    out_width,
                    out_height,
                    Flags::BILINEAR,
                )
                .map_err(ffmpeg::error)?;
                self.contexts.push((shape, context));
                self.contexts.len() - 1
            }
        };
        Ok(&mut self.contexts[position].1)
    }

    /// The picture at another size or format.
    pub fn picture(
        &mut self,
        input: &Picture,
        width: u32,
        height: u32,
        format: PixelFormat,
    ) -> Result<Picture, CodecError> {
        self.frame(
            &ffmpeg::to_frame(input),
            width,
            height,
            format,
            input.captured,
        )
    }

    /// An FFmpeg frame of any format as a picture.
    pub(super) fn frame(
        &mut self,
        frame: &ff::frame::Video,
        width: u32,
        height: u32,
        format: PixelFormat,
        captured: std::time::Instant,
    ) -> Result<Picture, CodecError> {
        let shape = (
            frame.format(),
            frame.width(),
            frame.height(),
            ffmpeg::pixel(format),
            width,
            height,
        );
        let mut output = ff::frame::Video::empty();
        self.context(shape)?
            .run(frame, &mut output)
            .map_err(ffmpeg::error)?;
        ffmpeg::from_frame(&output, captured).ok_or(CodecError::WrongPicture)
    }

    /// The picture as RGBA rows, `width` by `height`.
    pub fn rgba(
        &mut self,
        input: &Picture,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, CodecError> {
        let frame = ffmpeg::to_frame(input);
        let shape = (
            frame.format(),
            input.width,
            input.height,
            Pixel::RGBA,
            width,
            height,
        );
        let mut output = ff::frame::Video::empty();
        self.context(shape)?
            .run(&frame, &mut output)
            .map_err(ffmpeg::error)?;
        let row = width as usize * 4;
        let stride = output.stride(0);
        let data = output.data(0);
        let mut rgba = Vec::with_capacity(row * height as usize);
        for line in 0..height as usize {
            rgba.extend_from_slice(&data[line * stride..line * stride + row]);
        }
        Ok(rgba)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::video::pattern::scene;

    fn mean(values: &[u8]) -> f64 {
        values.iter().map(|&v| f64::from(v)).sum::<f64>() / values.len() as f64
    }

    #[test]
    fn a_picture_scales_to_a_layer_keeping_its_look() {
        let source = scene(1280, 720, 7, Instant::now());
        let mut scaler = Scaler::new();

        let layer = scaler
            .picture(&source, 640, 360, PixelFormat::Nv12)
            .unwrap();

        assert_eq!(
            (layer.width, layer.height, layer.format),
            (640, 360, PixelFormat::Nv12)
        );
        assert_eq!(layer.captured, source.captured);
        assert!((mean(layer.y()) - mean(source.y())).abs() < 2.0);
    }

    #[test]
    fn i420_converts_to_nv12_at_the_same_size() {
        let mut source = Picture::black(PixelFormat::I420, 64, 32, Instant::now());
        source.data[64 * 32..64 * 32 + 32 * 16].fill(90);
        let mut scaler = Scaler::new();

        let nv12 = scaler.picture(&source, 64, 32, PixelFormat::Nv12).unwrap();

        assert_eq!(nv12.format, PixelFormat::Nv12);
        assert_eq!(&nv12.data[64 * 32..64 * 32 + 4], &[90, 128, 90, 128]);
    }

    #[test]
    fn black_and_white_become_rgba_black_and_white() {
        let mut picture = Picture::black(PixelFormat::Nv12, 64, 32, Instant::now());
        picture.data[..32].fill(235);
        let mut scaler = Scaler::new();

        let rgba = scaler.rgba(&picture, 64, 32).unwrap();

        assert_eq!(rgba.len(), 64 * 32 * 4);
        let white = &rgba[..4];
        let black = &rgba[(31 * 64 + 63) * 4..];
        assert!(
            white[..3].iter().all(|&c| c >= 250) && white[3] == 255,
            "{white:?}"
        );
        assert!(
            black[..3].iter().all(|&c| c <= 5) && black[3] == 255,
            "{black:?}"
        );
    }

    #[test]
    fn rgba_comes_at_the_tile_size() {
        let picture = scene(640, 360, 0, Instant::now());
        let mut scaler = Scaler::new();

        assert_eq!(
            scaler.rgba(&picture, 320, 180).unwrap().len(),
            320 * 180 * 4
        );
        assert_eq!(
            scaler.rgba(&picture, 640, 360).unwrap().len(),
            640 * 360 * 4
        );
    }
}
