//! Direct conversions for what cameras deliver most, to NV12 in video
//! range: packed YUYV, and the full-range 4:2:2 or 4:2:0 planes FFmpeg
//! decodes MJPEG into; and halving NV12, as each camera layer is half the
//! one above it. swscale takes several times as long for these (plan
//! §16.1's camera budget).

use crate::video::picture::{Picture, PixelFormat};

/// Full range (0–255) to video range (16–235), rounded; in `u16` so that
/// loops over rows vectorize.
fn video_luma(value: u16) -> u8 {
    (16 + (value * 219 + 127) / 255) as u8
}

/// Full-range chroma (0–255, 128 none) to video range (16–240), rounded:
/// 128 + (value − 128)·224/255.
fn video_chroma(value: u16) -> u8 {
    ((value * 224 + 4095) / 255) as u8
}

/// Packed YUYV (4:2:2) as NV12: luma as it is, each chroma pair averaged
/// over the two rows it covers.
pub fn yuyv_to_nv12(yuyv: &[u8], width: usize, height: usize) -> Vec<u8> {
    let row = width * 2;
    let mut out = vec![0u8; width * height + width * height / 2];
    let (luma, chroma) = out.split_at_mut(width * height);
    for (line, target) in yuyv.chunks_exact(row).zip(luma.chunks_exact_mut(width)) {
        for (pixels, pair) in target
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .zip(line.as_chunks::<4>().0)
        {
            *pixels = [pair[0], pair[2]];
        }
    }
    for (rows, target) in yuyv
        .chunks_exact(row * 2)
        .zip(chroma.chunks_exact_mut(width))
    {
        let (top, bottom) = rows.split_at(row);
        for ((uv, a), b) in target
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .zip(top.as_chunks::<4>().0)
            .zip(bottom.as_chunks::<4>().0)
        {
            *uv = [
                (u16::from(a[1]) + u16::from(b[1])).div_ceil(2) as u8,
                (u16::from(a[3]) + u16::from(b[3])).div_ceil(2) as u8,
            ];
        }
    }
    out
}

/// Full-range planar 4:2:2 (luma, blue, red, each with its row length in
/// the buffer) as video-range NV12, chroma averaged over row pairs.
pub fn full_422_to_nv12(
    planes: [&[u8]; 3],
    strides: [usize; 3],
    width: usize,
    height: usize,
) -> Vec<u8> {
    full_planar_to_nv12(planes, strides, width, height, true)
}

/// Full-range planar 4:2:0 as video-range NV12.
pub fn full_420_to_nv12(
    planes: [&[u8]; 3],
    strides: [usize; 3],
    width: usize,
    height: usize,
) -> Vec<u8> {
    full_planar_to_nv12(planes, strides, width, height, false)
}

/// `tall_chroma`: chroma has a row for every luma row (4:2:2), else one for
/// every two (4:2:0). Rows missing from the planes stay black.
fn full_planar_to_nv12(
    planes: [&[u8]; 3],
    strides: [usize; 3],
    width: usize,
    height: usize,
    tall_chroma: bool,
) -> Vec<u8> {
    let mut out = vec![16u8; width * height];
    out.resize(width * height + width * height / 2, 128);
    let (luma, chroma) = out.split_at_mut(width * height);
    for (index, target) in luma.chunks_exact_mut(width).enumerate() {
        let line = row(planes[0], strides[0], width, index);
        for (pixel, value) in target.iter_mut().zip(line) {
            *pixel = video_luma(u16::from(*value));
        }
    }
    let half = width / 2;
    for (pair, target) in chroma.chunks_exact_mut(width).enumerate() {
        let rows = |plane: usize| {
            let (top, bottom) = if tall_chroma {
                (2 * pair, 2 * pair + 1)
            } else {
                (pair, pair)
            };
            (
                row(planes[plane], strides[plane], half, top),
                row(planes[plane], strides[plane], half, bottom),
            )
        };
        let (blue_top, blue_bottom) = rows(1);
        let (red_top, red_bottom) = rows(2);
        let average = |a: &u8, b: &u8| video_chroma((u16::from(*a) + u16::from(*b)).div_ceil(2));
        for ((((uv, blue_top), blue_bottom), red_top), red_bottom) in target
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .zip(blue_top)
            .zip(blue_bottom)
            .zip(red_top)
            .zip(red_bottom)
        {
            *uv = [average(blue_top, blue_bottom), average(red_top, red_bottom)];
        }
    }
    out
}

/// BGRx (as screen capture most often delivers it, the fourth byte unused)
/// with rows `stride` bytes apart, as video-range BT.601 NV12 of `width` ×
/// `height` (even, at most the source's size: an odd source loses its
/// last column or row); chroma from each 2×2 block's mean colour.
pub fn bgrx_to_nv12(bgrx: &[u8], stride: usize, width: usize, height: usize) -> Vec<u8> {
    rgb32_to_nv12::<2, 0>(bgrx, stride, width, height)
}

/// RGBx as video-range BT.601 NV12; see [`bgrx_to_nv12`].
pub fn rgbx_to_nv12(rgbx: &[u8], stride: usize, width: usize, height: usize) -> Vec<u8> {
    rgb32_to_nv12::<0, 2>(rgbx, stride, width, height)
}

/// Four bytes a pixel, red at `RED`, green at 1 and blue at `BLUE`.
fn rgb32_to_nv12<const RED: usize, const BLUE: usize>(
    data: &[u8],
    stride: usize,
    width: usize,
    height: usize,
) -> Vec<u8> {
    let row = width * 4;
    let line = |index: usize| {
        data.get(index * stride..index * stride + row)
            .unwrap_or_default()
    };
    let mut out = vec![16u8; width * height];
    out.resize(width * height + width * height / 2, 128);
    let (luma, chroma) = out.split_at_mut(width * height);
    let luma_of = |pixel: &[u8; 4]| {
        let (r, g, b) = (
            u32::from(pixel[RED]),
            u32::from(pixel[1]),
            u32::from(pixel[BLUE]),
        );
        (((66 * r + 129 * g + 25 * b + 128) >> 8) + 16) as u8
    };
    for (index, target) in luma.chunks_exact_mut(width).enumerate() {
        for (value, pixel) in target.iter_mut().zip(line(index).as_chunks::<4>().0) {
            *value = luma_of(pixel);
        }
    }
    for (pair, target) in chroma.chunks_exact_mut(width).enumerate() {
        let (top, bottom) = (line(2 * pair), line(2 * pair + 1));
        for ((uv, a), b) in target
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .zip(top.as_chunks::<8>().0)
            .zip(bottom.as_chunks::<8>().0)
        {
            let sum = |channel: usize| {
                i32::from(a[channel])
                    + i32::from(a[channel + 4])
                    + i32::from(b[channel])
                    + i32::from(b[channel + 4])
            };
            let (r, g, b) = ((sum(RED) + 2) >> 2, (sum(1) + 2) >> 2, (sum(BLUE) + 2) >> 2);
            *uv = [
                (((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128) as u8,
                (((112 * r - 94 * g - 18 * b + 128) >> 8) + 128) as u8,
            ];
        }
    }
    out
}

/// An NV12 picture at half its width and height, each value the mean of
/// the four it covers; `None` unless the picture is NV12 with both sizes
/// multiples of four.
pub fn halve_nv12(picture: &Picture) -> Option<Picture> {
    let (width, height) = (picture.width as usize, picture.height as usize);
    if picture.format != PixelFormat::Nv12
        || !width.is_multiple_of(4)
        || !height.is_multiple_of(4)
        || picture.data.len() != width * height * 3 / 2
    {
        return None;
    }
    let (half_width, half_height) = (width / 2, height / 2);
    let mut out = vec![0u8; half_width * half_height * 3 / 2];
    let (luma, chroma) = picture.data.split_at(width * height);
    let (out_luma, out_chroma) = out.split_at_mut(half_width * half_height);
    for (rows, target) in luma
        .chunks_exact(width * 2)
        .zip(out_luma.chunks_exact_mut(half_width))
    {
        let (top, bottom) = rows.split_at(width);
        for ((value, a), b) in target
            .iter_mut()
            .zip(top.as_chunks::<2>().0)
            .zip(bottom.as_chunks::<2>().0)
        {
            *value = mean([a[0], a[1], b[0], b[1]]);
        }
    }
    // Chroma rows are `width` bytes of blue and red pairs.
    for (rows, target) in chroma
        .chunks_exact(width * 2)
        .zip(out_chroma.chunks_exact_mut(half_width))
    {
        let (top, bottom) = rows.split_at(width);
        for ((uv, a), b) in target
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .zip(top.as_chunks::<4>().0)
            .zip(bottom.as_chunks::<4>().0)
        {
            *uv = [
                mean([a[0], a[2], b[0], b[2]]),
                mean([a[1], a[3], b[1], b[3]]),
            ];
        }
    }
    Picture::from_data(
        PixelFormat::Nv12,
        half_width as u32,
        half_height as u32,
        out,
        picture.captured,
    )
}

fn mean(values: [u8; 4]) -> u8 {
    let sum: u16 = values.into_iter().map(u16::from).sum();
    ((sum + 2) / 4) as u8
}

/// Row `index` of a plane, `length` bytes; empty where the plane ends.
fn row(plane: &[u8], stride: usize, length: usize, index: usize) -> &[u8] {
    plane
        .get(index * stride..index * stride + length)
        .unwrap_or_default()
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::video::codec::scale::Scaler;
    use crate::video::pattern::scene;

    fn close(a: &[u8], b: &[u8], tolerance: u8) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.abs_diff(*y) <= tolerance)
    }

    #[test]
    fn yuyv_becomes_nv12_with_chroma_from_both_rows() {
        // 4×2: U 90 then 110 by row, V 160 then 140.
        let yuyv = [
            10, 90, 20, 160, 30, 90, 40, 160, //
            50, 110, 60, 140, 70, 110, 80, 140,
        ];

        let nv12 = yuyv_to_nv12(&yuyv, 4, 2);

        assert_eq!(&nv12[..8], &[10, 20, 30, 40, 50, 60, 70, 80]);
        assert_eq!(&nv12[8..], &[100, 150, 100, 150]);
    }

    #[test]
    fn ranges_round_as_the_formulas_do() {
        for value in 0..=255u16 {
            let full = f64::from(value);
            assert_eq!(
                video_luma(value),
                (16.0 + full * 219.0 / 255.0).round() as u8
            );
            assert_eq!(
                video_chroma(value),
                (128.0 + (full - 128.0) * 224.0 / 255.0).round() as u8
            );
        }
    }

    #[test]
    fn full_range_422_planes_become_video_range_nv12() {
        let luma = [0u8, 255, 128, 64];
        let blue = [0u8, 255];
        let red = [128u8, 128];

        let nv12 = full_422_to_nv12([&luma, &blue, &red], [2, 1, 1], 2, 2);

        assert_eq!(&nv12[..4], &[16, 235, 126, 71]);
        assert_eq!(&nv12[4..], &[128, 128]);
    }

    #[test]
    fn they_match_swscale() {
        let source = scene(64, 32, 3, Instant::now());
        let mut scaler = Scaler::new();
        // A YUYV frame of the scene, chroma alike on both rows.
        let mut yuyv = Vec::new();
        let (luma, chroma) = source.data.split_at(64 * 32);
        for row in 0..32 {
            for pair in 0..32 {
                let at = (row / 2) * 64 + pair * 2;
                yuyv.extend([
                    luma[row * 64 + pair * 2],
                    chroma[at],
                    luma[row * 64 + pair * 2 + 1],
                    chroma[at + 1],
                ]);
            }
        }

        let ours = yuyv_to_nv12(&yuyv, 64, 32);

        let theirs = scaler.picture(&source, 64, 32, PixelFormat::Nv12).unwrap();
        assert!(close(&ours, &theirs.data, 1));
    }

    #[test]
    fn screen_pixels_become_nv12_as_swscale_makes_them() {
        use ffmpeg_next as ff;

        // A gradient with some colour, as BGRx.
        let (width, height) = (64usize, 32usize);
        let mut bgrx = Vec::new();
        for y in 0..height {
            for x in 0..width {
                bgrx.extend([(x * 4) as u8, (y * 8) as u8, (255 - x * 3) as u8, 0]);
            }
        }
        let rgbx: Vec<u8> = bgrx
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|pixel| [pixel[2], pixel[1], pixel[0], 0])
            .collect();

        let ours = bgrx_to_nv12(&bgrx, width * 4, width, height);

        crate::video::codec::ffmpeg::init();
        let mut frame = ff::frame::Video::new(ff::format::Pixel::BGRZ, width as u32, height as u32);
        let stride = frame.stride(0);
        for (line, source) in bgrx.chunks_exact(width * 4).enumerate() {
            frame.data_mut(0)[line * stride..line * stride + width * 4].copy_from_slice(source);
        }
        let theirs = Scaler::new()
            .frame(
                &frame,
                width as u32,
                height as u32,
                PixelFormat::Nv12,
                Instant::now(),
            )
            .unwrap();
        assert!(close(&ours, &theirs.data, 2));
        assert_eq!(rgbx_to_nv12(&rgbx, width * 4, width, height), ours);
        // White and black land on video range.
        let white = bgrx_to_nv12(&[255; 16], 8, 2, 2);
        assert_eq!(white, [235, 235, 235, 235, 128, 128]);
        assert_eq!(bgrx_to_nv12(&[0; 16], 8, 2, 2), [16, 16, 16, 16, 128, 128]);
        // An odd 3×3 window loses its last column and row.
        assert_eq!(bgrx_to_nv12(&[255; 36], 12, 2, 2), white);
    }

    #[test]
    fn halving_takes_the_mean_of_each_square() {
        // 4×4 luma; chroma two rows of two blue and red pairs.
        let data = vec![
            0, 4, 8, 8, //
            4, 8, 8, 8, //
            100, 100, 50, 50, //
            100, 101, 50, 51, //
            10, 20, 30, 40, //
            20, 30, 40, 50,
        ];
        let picture = Picture::from_data(PixelFormat::Nv12, 4, 4, data, Instant::now()).unwrap();

        let half = halve_nv12(&picture).unwrap();

        assert_eq!((half.width, half.height), (2, 2));
        assert_eq!(&half.data[..4], &[4, 8, 100, 50]);
        assert_eq!(&half.data[4..], &[25, 35]);
    }

    #[test]
    fn halving_looks_like_scaling_and_needs_sizes_in_fours() {
        let source = scene(1280, 720, 5, Instant::now());
        let mut scaler = Scaler::new();

        let half = halve_nv12(&source).unwrap();

        let scaled = scaler
            .picture(&source, 640, 360, PixelFormat::Nv12)
            .unwrap();
        let difference: u64 = half
            .data
            .iter()
            .zip(&scaled.data)
            .map(|(a, b)| u64::from(a.abs_diff(*b)))
            .sum();
        assert!(difference as f64 / (half.data.len() as f64) < 2.0);
        assert!(halve_nv12(&scene(1280, 722, 5, Instant::now())).is_none());
    }
}
