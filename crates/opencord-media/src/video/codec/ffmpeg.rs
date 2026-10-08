//! FFmpeg, set up once, and pictures in and out of its frames.

use std::sync::Once;
use std::time::Instant;

use ff::format::Pixel;
use ff::frame;
use ffmpeg_next as ff;

use super::CodecError;
use crate::video::picture::{Picture, PixelFormat};

pub(super) fn init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Err(error) = ff::init() {
            tracing::warn!(%error, "FFmpeg did not start");
        }
        // Encoders and decoders that fail say so through their results.
        ff::log::set_level(ff::log::Level::Fatal);
    });
}

pub(super) fn error(error: ff::Error) -> CodecError {
    CodecError::Ffmpeg(error.to_string())
}

pub(super) fn pixel(format: PixelFormat) -> Pixel {
    match format {
        PixelFormat::Nv12 => Pixel::NV12,
        PixelFormat::I420 => Pixel::YUV420P,
    }
}

/// The picture in an FFmpeg frame of its own.
pub(super) fn to_frame(picture: &Picture) -> frame::Video {
    let mut frame = frame::Video::new(pixel(picture.format), picture.width, picture.height);
    for (index, (plane, row)) in picture.planes().into_iter().enumerate() {
        let stride = frame.stride(index);
        let data = frame.data_mut(index);
        for (line, source) in plane.chunks_exact(row).enumerate() {
            data[line * stride..line * stride + row].copy_from_slice(source);
        }
    }
    frame
}

/// A frame as a packed picture; `None` for formats other than NV12 and
/// 4:2:0 planar.
pub(super) fn from_frame(frame: &frame::Video, captured: Instant) -> Option<Picture> {
    let format = match frame.format() {
        Pixel::NV12 => PixelFormat::Nv12,
        Pixel::YUV420P | Pixel::YUVJ420P => PixelFormat::I420,
        _ => return None,
    };
    let (width, height) = (frame.width(), frame.height());
    if !width.is_multiple_of(2) || !height.is_multiple_of(2) {
        return None;
    }
    let (w, h) = (width as usize, height as usize);
    let planes = match format {
        PixelFormat::Nv12 => vec![(w, h), (w, h / 2)],
        PixelFormat::I420 => vec![(w, h), (w / 2, h / 2), (w / 2, h / 2)],
    };
    let mut data = Vec::with_capacity(Picture::bytes(width, height));
    for (index, (row, rows)) in planes.into_iter().enumerate() {
        let stride = frame.stride(index);
        let source = frame.data(index);
        for line in 0..rows {
            data.extend_from_slice(&source[line * stride..line * stride + row]);
        }
    }
    Picture::from_data(format, width, height, data, captured)
}

/// The same picture in NV12.
pub(super) fn to_nv12(picture: &Picture) -> Picture {
    if picture.format == PixelFormat::Nv12 {
        return picture.clone();
    }
    let luma = picture.width as usize * picture.height as usize;
    let mut data = Vec::with_capacity(picture.data.len());
    data.extend_from_slice(&picture.data[..luma]);
    let (u, v) = picture.data[luma..].split_at(luma / 4);
    for (u, v) in u.iter().zip(v) {
        data.push(*u);
        data.push(*v);
    }
    Picture {
        format: PixelFormat::Nv12,
        data,
        ..*picture
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::pattern::scene;

    #[test]
    fn a_picture_survives_a_frame_and_back() {
        let picture = scene(320, 180, 3, Instant::now());

        let back = from_frame(&to_frame(&picture), picture.captured).unwrap();

        assert_eq!(back.format, PixelFormat::Nv12);
        assert_eq!(back.data, picture.data);
    }

    #[test]
    fn i420_becomes_nv12_with_chroma_interleaved() {
        let mut data = vec![50; 4 * 2];
        data.extend([1, 2]);
        data.extend([3, 4]);
        let i420 = Picture::from_data(PixelFormat::I420, 4, 2, data, Instant::now()).unwrap();

        let nv12 = to_nv12(&i420);

        assert_eq!(&nv12.data[8..], &[1, 3, 2, 4]);
    }
}
