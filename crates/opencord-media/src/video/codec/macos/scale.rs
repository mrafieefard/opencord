//! Scaling and colour conversion in portable Rust (docs/plans/macos.md M3,
//! decision C): pic-scale resamples the planes and yuv turns NV12 into
//! RGBA, both with SIMD; a camera picture to each layer's size, a decoded
//! picture to RGBA at the size of its tile (plan §7.8, §7.11). BT.601,
//! video range, as FFmpeg's swscale on Linux. Chosen by measurement over a
//! hand-written filter: 1080p NV12 → 640×360 RGBA took 1.33 ms against
//! 9.93 ms on the Intel test Mac (§7, M3).

use std::sync::Arc;

use pic_scale::{
    ImageSize, ImageStore, ImageStoreMut, Resampling, ResamplingFunction, ThreadingPolicy,
};
use yuv::{YuvBiPlanarImage, YuvConversionMode, YuvRange, YuvStandardMatrix};

use super::super::{CodecError, convert};
use crate::video::picture::{Picture, PixelFormat};

/// A resampling: source width and height, target width and height.
type Sizes = (usize, usize, usize, usize);

/// Scales and converts pictures; one per thread that converts.
pub struct Scaler {
    resampler: pic_scale::Scaler,
    /// Resampling plans by sizes, for luma and for interleaved chroma.
    luma: Vec<(Sizes, Arc<Resampling<u8, 1>>)>,
    chroma: Vec<(Sizes, Arc<Resampling<u8, 2>>)>,
}

impl Default for Scaler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scaler {
    pub fn new() -> Self {
        Self {
            resampler: pic_scale::Scaler::new(ResamplingFunction::Bilinear)
                .set_threading_policy(ThreadingPolicy::Single),
            luma: Vec::new(),
            chroma: Vec::new(),
        }
    }

    /// The picture at another size or format.
    pub fn picture(
        &mut self,
        input: &Picture,
        width: u32,
        height: u32,
        format: PixelFormat,
    ) -> Result<Picture, CodecError> {
        let even = width.is_multiple_of(2) && height.is_multiple_of(2);
        if width == 0 || height == 0 || !even {
            return Err(CodecError::WrongPicture);
        }
        let halved = (input.format == PixelFormat::Nv12
            && (input.width, input.height) == (width * 2, height * 2))
            .then(|| convert::halve_nv12(input))
            .flatten();
        let nv12 = match halved {
            Some(halved) => halved,
            None => {
                let (luma, chroma) = self.nv12_planes(input, width as usize, height as usize)?;
                let mut data = luma;
                data.extend_from_slice(&chroma);
                Picture::from_data(PixelFormat::Nv12, width, height, data, input.captured)
                    .ok_or(CodecError::WrongPicture)?
            }
        };
        Ok(match format {
            PixelFormat::Nv12 => nv12,
            PixelFormat::I420 => to_i420(&nv12),
        })
    }

    /// The picture as RGBA rows, `width` by `height`.
    pub fn rgba(
        &mut self,
        input: &Picture,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, CodecError> {
        if width == 0 || height == 0 {
            return Err(CodecError::WrongPicture);
        }
        let (w, h) = (width as usize, height as usize);
        let (luma, chroma) = self.nv12_planes(input, w, h)?;
        let image = YuvBiPlanarImage {
            y_plane: &luma,
            y_stride: width,
            uv_plane: &chroma,
            uv_stride: w.div_ceil(2) as u32 * 2,
            width,
            height,
        };
        let mut rgba = vec![0u8; w * h * 4];
        yuv::yuv_nv12_to_rgba(
            &image,
            &mut rgba,
            width * 4,
            YuvRange::Limited,
            YuvStandardMatrix::Bt601,
            YuvConversionMode::Balanced,
        )
        .map_err(|error| {
            tracing::debug!(%error, "NV12 did not convert to RGBA");
            CodecError::WrongPicture
        })?;
        Ok(rgba)
    }

    /// The picture's luma at `width` × `height`, and its chroma interleaved
    /// at half that (rounded up).
    fn nv12_planes(
        &mut self,
        input: &Picture,
        width: usize,
        height: usize,
    ) -> Result<(Vec<u8>, Vec<u8>), CodecError> {
        let (in_width, in_height) = (input.width as usize, input.height as usize);
        let interleaved;
        let in_chroma = match input.format {
            PixelFormat::Nv12 => &input.data[in_width * in_height..],
            PixelFormat::I420 => {
                interleaved = interleave(input);
                &interleaved[..]
            }
        };
        let in_luma = input.y();
        let (chroma_width, chroma_height) = (width.div_ceil(2), height.div_ceil(2));
        if (in_width, in_height) == (width, height) {
            return Ok((in_luma.to_vec(), in_chroma.to_vec()));
        }
        let luma = self.luma_plan((in_width, in_height, width, height))?;
        let chroma =
            self.chroma_plan((in_width / 2, in_height / 2, chroma_width, chroma_height))?;
        let failed = |error: pic_scale::PicScaleError| {
            tracing::debug!(%error, "a picture did not scale");
            CodecError::WrongPicture
        };
        let source =
            ImageStore::<u8, 1>::from_slice(in_luma, in_width, in_height).map_err(failed)?;
        let mut out_luma = ImageStoreMut::<u8, 1>::alloc(width, height);
        luma.resample(&source, &mut out_luma).map_err(failed)?;
        let source = ImageStore::<u8, 2>::from_slice(in_chroma, in_width / 2, in_height / 2)
            .map_err(failed)?;
        let mut out_chroma = ImageStoreMut::<u8, 2>::alloc(chroma_width, chroma_height);
        chroma.resample(&source, &mut out_chroma).map_err(failed)?;
        Ok((out_luma.as_bytes().to_vec(), out_chroma.as_bytes().to_vec()))
    }

    fn luma_plan(&mut self, sizes: Sizes) -> Result<Arc<Resampling<u8, 1>>, CodecError> {
        if let Some((_, plan)) = self.luma.iter().find(|(known, _)| *known == sizes) {
            return Ok(Arc::clone(plan));
        }
        let (from_w, from_h, to_w, to_h) = sizes;
        let plan = self
            .resampler
            .plan_planar_resampling(ImageSize::new(from_w, from_h), ImageSize::new(to_w, to_h))
            .map_err(|_| CodecError::WrongPicture)?;
        self.luma.push((sizes, Arc::clone(&plan)));
        Ok(plan)
    }

    fn chroma_plan(&mut self, sizes: Sizes) -> Result<Arc<Resampling<u8, 2>>, CodecError> {
        if let Some((_, plan)) = self.chroma.iter().find(|(known, _)| *known == sizes) {
            return Ok(Arc::clone(plan));
        }
        let (from_w, from_h, to_w, to_h) = sizes;
        let plan = self
            .resampler
            .plan_cbcr_resampling(ImageSize::new(from_w, from_h), ImageSize::new(to_w, to_h))
            .map_err(|_| CodecError::WrongPicture)?;
        self.chroma.push((sizes, Arc::clone(&plan)));
        Ok(plan)
    }
}

/// An I420 picture's chroma, interleaved as NV12's.
fn interleave(picture: &Picture) -> Vec<u8> {
    let planes = picture.planes();
    let (u, v) = (planes[1].0, planes[2].0);
    let mut chroma = Vec::with_capacity(u.len() * 2);
    for (u, v) in u.iter().zip(v) {
        chroma.push(*u);
        chroma.push(*v);
    }
    chroma
}

/// An NV12 picture as I420.
fn to_i420(picture: &Picture) -> Picture {
    let luma = picture.width as usize * picture.height as usize;
    let (y, chroma) = picture.data.split_at(luma);
    let mut data = y.to_vec();
    data.extend(chroma.iter().step_by(2));
    data.extend(chroma.iter().skip(1).step_by(2));
    Picture {
        format: PixelFormat::I420,
        data,
        ..picture.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::video::pattern::scene;

    fn mean(values: &[u8]) -> f64 {
        values.iter().map(|&v| f64::from(v)).sum::<f64>() / values.len() as f64
    }

    #[test]
    fn a_picture_scales_to_a_layer_keeping_its_look() {
        let source = scene(1280, 720, 7, Instant::now());
        let mut scaler = Scaler::new();

        for (width, height) in [(640, 360), (426, 240), (960, 540)] {
            let layer = scaler
                .picture(&source, width, height, PixelFormat::Nv12)
                .unwrap();

            assert_eq!(
                (layer.width, layer.height, layer.format),
                (width, height, PixelFormat::Nv12)
            );
            assert_eq!(layer.captured, source.captured);
            assert!((mean(layer.y()) - mean(source.y())).abs() < 2.0);
        }
    }

    #[test]
    fn i420_converts_to_nv12_at_the_same_size_and_back() {
        let mut source = Picture::black(PixelFormat::I420, 64, 32, Instant::now());
        source.data[64 * 32..64 * 32 + 32 * 16].fill(90);
        let mut scaler = Scaler::new();

        let nv12 = scaler.picture(&source, 64, 32, PixelFormat::Nv12).unwrap();
        let i420 = scaler.picture(&nv12, 64, 32, PixelFormat::I420).unwrap();

        assert_eq!(nv12.format, PixelFormat::Nv12);
        assert_eq!(&nv12.data[64 * 32..64 * 32 + 4], &[90, 128, 90, 128]);
        assert_eq!(i420.data, source.data);
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

    /// BT.601 at video range: patches of known Y'CbCr become the RGB
    /// they stand for, within 3 levels (M3, step 7).
    #[test]
    fn colour_patches_match_bt601_video_range() {
        // (Y, Cb, Cr) → (R, G, B): full red, green and blue, mid grey.
        let patches = [
            ((81, 90, 240), (255, 0, 0)),
            ((145, 54, 34), (0, 255, 0)),
            ((41, 240, 110), (0, 0, 255)),
            ((126, 128, 128), (128, 128, 128)),
        ];
        let mut scaler = Scaler::new();
        for ((y, cb, cr), expected) in patches {
            let mut picture = Picture::black(PixelFormat::Nv12, 16, 16, Instant::now());
            picture.data[..256].fill(y);
            for pair in picture.data[256..].as_chunks_mut::<2>().0 {
                *pair = [cb, cr];
            }

            let rgba = scaler.rgba(&picture, 16, 16).unwrap();

            let got = (rgba[0], rgba[1], rgba[2]);
            let near = |a: u8, b: u8| a.abs_diff(b) <= 3;
            assert!(
                near(got.0, expected.0) && near(got.1, expected.1) && near(got.2, expected.2),
                "({y}, {cb}, {cr}) gave {got:?}, wanted {expected:?}"
            );
        }
    }

    #[test]
    fn rgba_comes_at_the_tile_size_even_odd_ones() {
        let picture = scene(640, 360, 0, Instant::now());
        let mut scaler = Scaler::new();

        for (width, height) in [(320, 180), (640, 360), (333, 187)] {
            assert_eq!(
                scaler.rgba(&picture, width, height).unwrap().len(),
                width as usize * height as usize * 4
            );
        }
    }

    /// Decision C's budget: 1080p NV12 to a 640×360 tile in under 2 ms.
    /// The best of 50, as debug builds of the test itself are slow.
    #[test]
    fn a_1080p_picture_becomes_a_360p_tile_quickly() {
        let picture = scene(1920, 1080, 3, Instant::now());
        let mut scaler = Scaler::new();
        let mut best = Duration::MAX;

        for _ in 0..50 {
            let start = Instant::now();
            std::hint::black_box(scaler.rgba(&picture, 640, 360).unwrap());
            best = best.min(start.elapsed());
        }

        eprintln!("1080p NV12 → 640×360 RGBA: {best:?}");
        if !cfg!(debug_assertions) {
            assert!(best < Duration::from_millis(2), "{best:?}");
        }
    }
}
