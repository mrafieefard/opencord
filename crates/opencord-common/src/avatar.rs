//! Profile pictures and server icons: a square PNG of [`SIDE`] pixels.
//! Apps make one from any picture before sending it; the server keeps one
//! that arrives in that shape as it is, so both name it by the same hash,
//! and makes one from anything else.

use std::io::Cursor;

use image::imageops::FilterType;
use image::{GenericImageView, ImageFormat, ImageReader, Limits};
use sha2::{Digest, Sha256};

/// Each side of a stored picture.
pub const SIDE: u32 = 256;
/// The largest picture taken to make one from.
pub const MAX_UPLOAD_BYTES: usize = 8 * 1024 * 1024;
/// The largest side and allocation decoding may need.
const MAX_SOURCE_SIDE: u32 = 8192;
const MAX_ALLOC_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum AvatarError {
    #[error("the picture is larger than 8 MB")]
    TooLarge,
    #[error("the picture couldn't be read: {0}")]
    Unreadable(String),
}

fn limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SOURCE_SIDE);
    limits.max_image_height = Some(MAX_SOURCE_SIDE);
    limits.max_alloc = Some(MAX_ALLOC_BYTES);
    limits
}

/// A stored picture made from [bytes]: the middle square, [`SIDE`] pixels
/// a side, as PNG.
pub fn normalize(bytes: &[u8]) -> Result<Vec<u8>, AvatarError> {
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err(AvatarError::TooLarge);
    }
    let unreadable = |error: image::ImageError| AvatarError::Unreadable(error.to_string());
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| AvatarError::Unreadable(error.to_string()))?;
    reader.limits(limits());
    let picture = reader.decode().map_err(unreadable)?;
    let (width, height) = picture.dimensions();
    let side = width.min(height);
    let square = picture.crop_imm((width - side) / 2, (height - side) / 2, side, side);
    let resized = square.resize_exact(SIDE, SIDE, FilterType::Lanczos3);
    let mut png = Vec::new();
    resized
        .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
        .map_err(unreadable)?;
    Ok(png)
}

/// Whether [bytes] are already a stored picture: a PNG of [`SIDE`] pixels
/// a side that decodes.
pub fn is_normalized(bytes: &[u8]) -> bool {
    if bytes.len() > MAX_UPLOAD_BYTES {
        return false;
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Png);
    reader.limits(limits());
    reader
        .decode()
        .is_ok_and(|picture| picture.dimensions() == (SIDE, SIDE))
}

/// A stored picture's name: its SHA-256, in hex.
pub fn hash(png: &[u8]) -> String {
    hex::encode(Sha256::digest(png))
}

/// Whether [text] could be a picture's name, so it is safe in a path.
pub fn valid_hash(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};

    fn png(width: u32, height: u32) -> Vec<u8> {
        let picture: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_fn(width, height, |x, _| Rgba([(x % 256) as u8, 40, 90, 255]));
        let mut bytes = Vec::new();
        picture
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap();
        bytes
    }

    #[test]
    fn any_picture_becomes_the_middle_square() {
        let made = normalize(&png(900, 300)).unwrap();
        assert!(is_normalized(&made));
        assert!(!is_normalized(&png(900, 300)));
        // Made again from what it made, it is the same: an app and the
        // server name it by one hash.
        assert_eq!(normalize(&made).unwrap(), made);
    }

    #[test]
    fn what_isnt_a_picture_or_is_too_big_is_refused() {
        assert!(matches!(
            normalize(b"not a picture"),
            Err(AvatarError::Unreadable(_))
        ));
        let huge = vec![0u8; MAX_UPLOAD_BYTES + 1];
        assert!(matches!(normalize(&huge), Err(AvatarError::TooLarge)));
        assert!(!is_normalized(b"not a picture"));
    }

    #[test]
    fn hashes_are_safe_names() {
        let name = hash(b"x");
        assert!(valid_hash(&name));
        assert!(!valid_hash("../../etc/passwd"));
        assert!(!valid_hash(&name.to_uppercase()));
    }
}
