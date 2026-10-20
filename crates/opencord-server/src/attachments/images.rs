//! Image processing and thumbnail generation.

use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::imageops::FilterType;
use image::{
    AnimationDecoder, DynamicImage, GenericImageView, ImageDecoder, ImageError, ImageReader, Limits,
};

const MAX_SIDE: u32 = 16_384;
const POSTER_MAX_SIDE: u32 = 8_192;
const MAX_ALLOC_BYTES: u64 = 512 * 1024 * 1024; // 512 MiB
const MAX_PIXELS: u64 = 50_000_000;
const THUMB_MAX_SIDE: u32 = 800;
const JPEG_QUALITY: u8 = 82;

#[derive(Debug, Clone)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    pub animated: bool,
    pub thumbnail: Vec<u8>,
    pub thumb_w: u32,
    pub thumb_h: u32,
}

pub fn process(bytes: &[u8]) -> Result<ImageInfo, ImageError> {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC_BYTES);

    // 1. Read dimensions before full decode to reject > 50 MP
    let mut probe_reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    probe_reader.limits(limits.clone());
    let (dim_w, dim_h) = probe_reader.into_dimensions()?;
    if (dim_w as u64) * (dim_h as u64) > MAX_PIXELS {
        return Err(ImageError::Limits(image::error::LimitError::from_kind(
            image::error::LimitErrorKind::DimensionError,
        )));
    }

    // 2. Animation, before the full decode so their buffers never coexist.
    let animated = is_animated(bytes, &limits);

    // 3. Decode with orientation
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    reader.limits(limits);
    let mut decoder = reader.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut image = DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    let (width, height) = image.dimensions();

    // 4. Thumbnail generation: unchanged if both sides <= 800, else resize
    let (thumb_image, thumb_w, thumb_h) = if width <= THUMB_MAX_SIDE && height <= THUMB_MAX_SIDE {
        (image.clone(), width, height)
    } else {
        let resized = image.resize(THUMB_MAX_SIDE, THUMB_MAX_SIDE, FilterType::Triangle);
        let (rw, rh) = resized.dimensions();
        (resized, rw, rh)
    };

    // 5. Encode JPEG q82 (to_rgb8) or PNG when color().has_alpha()
    let mut thumbnail = Vec::new();
    if thumb_image.color().has_alpha() {
        let encoder = PngEncoder::new(&mut thumbnail);
        thumb_image.write_with_encoder(encoder)?;
    } else {
        let mut encoder = JpegEncoder::new_with_quality(&mut thumbnail, JPEG_QUALITY);
        encoder.encode_image(&thumb_image.to_rgb8())?;
    }

    Ok(ImageInfo {
        width,
        height,
        animated,
        thumbnail,
        thumb_w,
        thumb_h,
    })
}

/// Whether the image has a second frame, decoded within `limits` like the
/// image itself.
fn is_animated(bytes: &[u8], limits: &Limits) -> bool {
    if let Ok(mut decoder) = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))
        && decoder.set_limits(limits.clone()).is_ok()
        && decoder.into_frames().take(2).filter(Result::is_ok).count() == 2
    {
        return true;
    }
    if let Ok(mut decoder) = image::codecs::webp::WebPDecoder::new(Cursor::new(bytes))
        && decoder.set_limits(limits.clone()).is_ok()
        && decoder.has_animation()
    {
        return true;
    }
    false
}

pub fn thumbnail_from_poster(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), ImageError> {
    let mut limits = Limits::default();
    limits.max_image_width = Some(POSTER_MAX_SIDE);
    limits.max_image_height = Some(POSTER_MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC_BYTES);

    let mut probe_reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    probe_reader.limits(limits.clone());
    let (dim_w, dim_h) = probe_reader.into_dimensions()?;
    if (dim_w as u64) * (dim_h as u64) > MAX_PIXELS {
        return Err(ImageError::Limits(image::error::LimitError::from_kind(
            image::error::LimitErrorKind::DimensionError,
        )));
    }

    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    reader.limits(limits);
    let mut decoder = reader.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut image = DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    let (width, height) = image.dimensions();

    let (thumb_image, thumb_w, thumb_h) = if width <= THUMB_MAX_SIDE && height <= THUMB_MAX_SIDE {
        (image, width, height)
    } else {
        let resized = image.resize(THUMB_MAX_SIDE, THUMB_MAX_SIDE, FilterType::Triangle);
        let (rw, rh) = resized.dimensions();
        (resized, rw, rh)
    };

    let mut thumbnail = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut thumbnail, JPEG_QUALITY);
    encoder.encode_image(&thumb_image.to_rgb8())?;

    Ok((thumbnail, thumb_w, thumb_h))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};

    #[test]
    fn thumbnails_fit_800_and_keep_alpha_as_png() {
        // a 1600x400 RGBA PNG -> thumbnail 800x200 PNG
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(1600, 400, Rgba([255, 0, 0, 128]));
        let mut png_bytes = Vec::new();
        img.write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)
            .unwrap();

        let info = process(&png_bytes).unwrap();
        assert_eq!(info.width, 1600);
        assert_eq!(info.height, 400);
        assert_eq!(info.thumb_w, 800);
        assert_eq!(info.thumb_h, 200);

        // Check thumbnail is PNG
        let thumb_reader = ImageReader::new(Cursor::new(&info.thumbnail))
            .with_guessed_format()
            .unwrap();
        assert_eq!(thumb_reader.format(), Some(image::ImageFormat::Png));
    }

    fn two_frame_gif() -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
            for shade in [0u8, 255] {
                let frame = image::RgbaImage::from_pixel(64, 64, Rgba([shade, 0, 0, 255]));
                encoder.encode_frame(image::Frame::new(frame)).unwrap();
            }
        }
        bytes
    }

    #[test]
    fn animation_is_found_within_the_limits() {
        let gif = two_frame_gif();
        assert!(process(&gif).unwrap().animated);
        // The frames are decoded under the same limits as the image.
        let mut tight = Limits::default();
        tight.max_alloc = Some(1024);
        assert!(!is_animated(&gif, &tight));
    }

    #[test]
    fn exif_rotation_is_applied() {
        // 40x20 source JPEG with Orientation=6 APP1 segment inserted after SOI
        let img = image::RgbImage::from_pixel(40, 20, image::Rgb([100, 150, 200]));
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
            .encode_image(&img)
            .unwrap();

        // SOI is FF D8
        assert_eq!(&jpeg[0..2], &[0xFF, 0xD8]);
        let app1_segment: [u8; 34] = [
            0xFF, 0xE1, 0x00, 0x22, b'E', b'x', b'i', b'f', 0x00, 0x00, 0x4D, 0x4D, 0x00, 0x2A,
            0x00, 0x00, 0x00, 0x08, 0x00, 0x01, 0x01, 0x12, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01,
            0x00, 0x06, 0x00, 0x00, 0x00, 0x00,
        ];

        let mut jpeg_with_exif = Vec::with_capacity(jpeg.len() + 34);
        jpeg_with_exif.extend_from_slice(&jpeg[0..2]);
        jpeg_with_exif.extend_from_slice(&app1_segment);
        jpeg_with_exif.extend_from_slice(&jpeg[2..]);

        let info = process(&jpeg_with_exif).unwrap();
        // A 40x20 source reports 20x40 after Orientation=6 rotation
        assert_eq!(info.width, 20);
        assert_eq!(info.height, 40);
    }
}
