//! Removing a photo's metadata before it is sent (media uploads plan,
//! Task 20): Exif, with its GPS position and camera serial numbers, XMP,
//! and PNG text. A JPEG keeps its orientation in a minimal Exif block, so
//! it still shows the right way up.

const JPEG_START: [u8; 3] = [0xFF, 0xD8, 0xFF];
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
const EXIF: &[u8] = b"Exif\0\0";
const XMP: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
/// The TIFF orientation tag.
const ORIENTATION: u16 = 0x0112;
/// VP8X flags for Exif and XMP chunks.
const VP8X_EXIF: u8 = 0x08;
const VP8X_XMP: u8 = 0x04;

/// Whether a file starting with `head` (its first 12 bytes or more) is a
/// JPEG, PNG or WebP, which may carry metadata.
pub fn is_photo(head: &[u8]) -> bool {
    head.starts_with(&JPEG_START)
        || head.starts_with(&PNG_SIGNATURE)
        || (head.len() >= 12 && &head[..4] == b"RIFF" && &head[8..12] == b"WEBP")
}

/// The bytes without metadata, or `None` when they aren't a well-formed
/// JPEG, PNG or WebP; the caller then sends the file as it is.
pub fn strip_metadata(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.starts_with(&JPEG_START) {
        jpeg(bytes)
    } else if bytes.starts_with(&PNG_SIGNATURE) {
        png(bytes)
    } else if is_photo(bytes) {
        webp(bytes)
    } else {
        None
    }
}

/// Copies the segments before the image data except APP1 Exif or XMP and
/// APP13 (IPTC), then the image data as it is.
fn jpeg(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(bytes.len());
    out.extend_from_slice(&bytes[..2]);
    let mut orientation = None;
    let mut at = 2;
    loop {
        let start = at;
        if *bytes.get(at)? != 0xFF {
            return None;
        }
        // Fill bytes may come before a marker.
        while *bytes.get(at + 1)? == 0xFF {
            at += 1;
        }
        let marker = *bytes.get(at + 1)?;
        at += 2;
        match marker {
            // Start of scan: the rest is image data.
            0xDA => {
                out.extend_from_slice(&bytes[start..]);
                break;
            }
            // Markers without a length don't belong before the scan.
            0x01 | 0xD0..=0xD9 => return None,
            _ => {
                let length =
                    usize::from(u16::from_be_bytes([*bytes.get(at)?, *bytes.get(at + 1)?]));
                if length < 2 {
                    return None;
                }
                let end = at.checked_add(length)?;
                let payload = bytes.get(at + 2..end)?;
                let metadata = match marker {
                    0xE1 if payload.starts_with(EXIF) => {
                        orientation =
                            orientation.or_else(|| exif_orientation(&payload[EXIF.len()..]));
                        true
                    }
                    0xE1 => payload.starts_with(XMP),
                    0xED => true,
                    _ => false,
                };
                if !metadata {
                    out.extend_from_slice(&bytes[start..end]);
                }
                at = end;
            }
        }
    }
    if let Some(turn @ 2..=8) = orientation {
        out.splice(2..2, orientation_segment(turn));
    }
    Some(out)
}

/// An APP1 Exif segment with only the orientation: big-endian TIFF, one
/// IFD, one SHORT entry.
fn orientation_segment(turn: u16) -> [u8; 36] {
    let [high, low] = turn.to_be_bytes();
    [
        0xFF, 0xE1, 0x00, 0x22, b'E', b'x', b'i', b'f', 0x00, 0x00, b'M', b'M', 0x00, 0x2A, 0x00,
        0x00, 0x00, 0x08, 0x00, 0x01, 0x01, 0x12, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01, high, low,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ]
}

/// The orientation tag in a TIFF block's first IFD, in either byte order.
fn exif_orientation(tiff: &[u8]) -> Option<u16> {
    let big = match tiff.get(..2)? {
        b"MM" => true,
        b"II" => false,
        _ => return None,
    };
    let u16_at = |at: usize| -> Option<u16> {
        let pair = [*tiff.get(at)?, *tiff.get(at + 1)?];
        Some(if big {
            u16::from_be_bytes(pair)
        } else {
            u16::from_le_bytes(pair)
        })
    };
    let u32_at = |at: usize| -> Option<u32> {
        let quad = [
            *tiff.get(at)?,
            *tiff.get(at + 1)?,
            *tiff.get(at + 2)?,
            *tiff.get(at + 3)?,
        ];
        Some(if big {
            u32::from_be_bytes(quad)
        } else {
            u32::from_le_bytes(quad)
        })
    };
    if u16_at(2)? != 42 {
        return None;
    }
    let ifd = usize::try_from(u32_at(4)?).ok()?;
    let count = usize::from(u16_at(ifd)?);
    (0..count).find_map(|index| {
        let entry = ifd.checked_add(2 + index * 12)?;
        // A SHORT sits at the start of the entry's 4-byte value.
        (u16_at(entry)? == ORIENTATION).then(|| u16_at(entry + 8))?
    })
}

/// Copies the chunks up to IEND except `eXIf`, the text chunks (`tEXt`,
/// `zTXt`, `iTXt`, which hold XMP and camera notes) and `tIME`.
fn png(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut out = PNG_SIGNATURE.to_vec();
    let mut at = PNG_SIGNATURE.len();
    loop {
        let length =
            usize::try_from(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?)).ok()?;
        let kind: [u8; 4] = bytes.get(at + 4..at + 8)?.try_into().ok()?;
        // Length, type, data and CRC.
        let end = at.checked_add(12)?.checked_add(length)?;
        let chunk = bytes.get(at..end)?;
        if !matches!(&kind, b"eXIf" | b"tEXt" | b"zTXt" | b"iTXt" | b"tIME") {
            out.extend_from_slice(chunk);
        }
        at = end;
        if &kind == b"IEND" {
            return Some(out);
        }
    }
}

/// Copies the RIFF chunks except `EXIF` and `XMP `, clears their VP8X
/// flags, and writes the new RIFF size.
fn webp(bytes: &[u8]) -> Option<Vec<u8>> {
    let riff_size = usize::try_from(u32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?)).ok()?;
    let end = riff_size.checked_add(8)?;
    if end > bytes.len() {
        return None;
    }
    let mut out = bytes[..12].to_vec();
    let mut at = 12;
    while at < end {
        let kind: [u8; 4] = bytes.get(at..at + 4)?.try_into().ok()?;
        let length = usize::try_from(u32::from_le_bytes(
            bytes.get(at + 4..at + 8)?.try_into().ok()?,
        ))
        .ok()?;
        // An odd-sized chunk carries a pad byte.
        let next = at
            .checked_add(8)?
            .checked_add(length)?
            .checked_add(length & 1)?;
        let chunk = bytes.get(at..next)?;
        match &kind {
            b"EXIF" | b"XMP " => {}
            b"VP8X" => {
                let mut chunk = chunk.to_vec();
                *chunk.get_mut(8)? &= !(VP8X_EXIF | VP8X_XMP);
                out.extend_from_slice(&chunk);
            }
            _ => out.extend_from_slice(chunk),
        }
        at = next;
    }
    let size = u32::try_from(out.len() - 8).ok()?;
    out[4..8].copy_from_slice(&size.to_le_bytes());
    Some(out)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use image::metadata::Orientation;
    use image::{ImageDecoder, ImageFormat, RgbImage, RgbaImage};

    use super::*;

    const SECRET: &[u8] = b"GPS 52.5200N 13.4050E";

    fn encoded(format: ImageFormat) -> Vec<u8> {
        let mut out = Vec::new();
        let picture = RgbaImage::from_fn(16, 8, |x, y| {
            image::Rgba([x as u8 * 16, y as u8 * 32, 90, 255])
        });
        let picture = match format {
            ImageFormat::Jpeg => {
                image::DynamicImage::ImageRgb8(RgbImage::from_fn(16, 8, |x, y| {
                    image::Rgb([x as u8 * 16, y as u8 * 32, 90])
                }))
            }
            _ => image::DynamicImage::ImageRgba8(picture),
        };
        picture
            .write_to(&mut Cursor::new(&mut out), format)
            .unwrap();
        out
    }

    /// Little-endian TIFF: orientation `turn` in IFD0, then `SECRET` where
    /// a GPS block would be.
    fn exif_block(turn: u16) -> Vec<u8> {
        let mut tiff = b"II\x2A\x00\x08\x00\x00\x00".to_vec();
        tiff.extend_from_slice(&1u16.to_le_bytes());
        tiff.extend_from_slice(&ORIENTATION.to_le_bytes());
        tiff.extend_from_slice(&3u16.to_le_bytes());
        tiff.extend_from_slice(&1u32.to_le_bytes());
        tiff.extend_from_slice(&turn.to_le_bytes());
        tiff.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
        tiff.extend_from_slice(SECRET);
        let mut block = EXIF.to_vec();
        block.extend_from_slice(&tiff);
        block
    }

    fn jpeg_segment(marker: u8, payload: &[u8]) -> Vec<u8> {
        let mut segment = vec![0xFF, marker];
        segment.extend_from_slice(&u16::try_from(payload.len() + 2).unwrap().to_be_bytes());
        segment.extend_from_slice(payload);
        segment
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    }

    fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut chunk = u32::try_from(data.len()).unwrap().to_be_bytes().to_vec();
        chunk.extend_from_slice(kind);
        chunk.extend_from_slice(data);
        // The CRC is copied, never checked here; image checks it below.
        let mut crc = crc32(kind);
        crc = crc32_more(crc, data);
        chunk.extend_from_slice(&(!crc).to_be_bytes());
        chunk
    }

    fn crc32(bytes: &[u8]) -> u32 {
        crc32_more(0xFFFF_FFFF, bytes)
    }

    fn crc32_more(mut crc: u32, bytes: &[u8]) -> u32 {
        for &byte in bytes {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        crc
    }

    #[test]
    fn jpeg_gps_exif_is_removed_and_orientation_kept() {
        let plain = encoded(ImageFormat::Jpeg);
        let mut photo = plain[..2].to_vec();
        photo.extend(jpeg_segment(0xE1, &exif_block(6)));
        photo.extend(jpeg_segment(0xE1, &[XMP, SECRET].concat()));
        photo.extend(jpeg_segment(0xED, SECRET));
        photo.extend_from_slice(&plain[2..]);

        let stripped = strip_metadata(&photo).unwrap();

        assert!(!contains(&stripped, SECRET));
        assert_eq!(&stripped[2..38], &orientation_segment(6));
        assert_eq!(stripped.len(), plain.len() + 36);
        let mut decoder = image::codecs::jpeg::JpegDecoder::new(Cursor::new(&stripped)).unwrap();
        assert_eq!(decoder.orientation().unwrap(), Orientation::Rotate90);
        image::load_from_memory(&stripped).unwrap();
    }

    #[test]
    fn jpeg_orientation_1_needs_no_exif() {
        let plain = encoded(ImageFormat::Jpeg);
        let mut photo = plain[..2].to_vec();
        photo.extend(jpeg_segment(0xE1, &exif_block(1)));
        photo.extend_from_slice(&plain[2..]);

        assert_eq!(strip_metadata(&photo).unwrap(), plain);
    }

    #[test]
    fn jpeg_without_exif_is_unchanged() {
        let plain = encoded(ImageFormat::Jpeg);

        assert_eq!(strip_metadata(&plain).unwrap(), plain);
    }

    #[test]
    fn a_truncated_jpeg_is_left_alone() {
        let plain = encoded(ImageFormat::Jpeg);
        let mut photo = plain[..2].to_vec();
        photo.extend(jpeg_segment(0xE1, &exif_block(6)));

        assert_eq!(strip_metadata(&photo[..photo.len() - 4]), None);
    }

    #[test]
    fn png_metadata_chunks_are_removed_and_the_png_decodes() {
        let plain = encoded(ImageFormat::Png);
        // After the signature and IHDR (8 + 25 bytes).
        let mut picture = plain[..33].to_vec();
        picture.extend(png_chunk(b"eXIf", &exif_block(1)[EXIF.len()..]));
        picture.extend(png_chunk(
            b"iTXt",
            &[b"XML:com.adobe.xmp\0\0\0\0\0", SECRET].concat(),
        ));
        picture.extend(png_chunk(b"tEXt", &[b"Comment\0", SECRET].concat()));
        picture.extend_from_slice(&plain[33..]);
        image::load_from_memory(&picture).unwrap();

        let stripped = strip_metadata(&picture).unwrap();

        assert!(!contains(&stripped, SECRET));
        assert_eq!(stripped, plain);
        image::load_from_memory(&stripped).unwrap();
    }

    #[test]
    fn webp_exif_is_removed_and_riff_size_fixed() {
        let plain = encoded(ImageFormat::WebP);
        // Extended format: VP8X (Exif and XMP flags set), the image, EXIF
        // and XMP chunks, one of odd size.
        let mut vp8x = b"VP8X".to_vec();
        vp8x.extend_from_slice(&10u32.to_le_bytes());
        vp8x.extend_from_slice(&[VP8X_EXIF | VP8X_XMP | 0x10, 0, 0, 0, 15, 0, 0, 7, 0, 0]);
        let mut picture = plain[..12].to_vec();
        picture.extend_from_slice(&vp8x);
        picture.extend_from_slice(&plain[12..]);
        for (kind, data) in [
            (b"EXIF", exif_block(6)[EXIF.len()..].to_vec()),
            (b"XMP ", [SECRET, b"!"].concat()),
        ] {
            picture.extend_from_slice(kind);
            picture.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
            picture.extend_from_slice(&data);
            if data.len() % 2 == 1 {
                picture.push(0);
            }
        }
        let size = u32::try_from(picture.len() - 8).unwrap();
        picture[4..8].copy_from_slice(&size.to_le_bytes());
        image::load_from_memory(&picture).unwrap();

        let stripped = strip_metadata(&picture).unwrap();

        assert!(!contains(&stripped, SECRET));
        assert_eq!(stripped[20] & (VP8X_EXIF | VP8X_XMP), 0);
        assert_eq!(stripped[20] & 0x10, 0x10);
        assert_eq!(
            u32::from_le_bytes(stripped[4..8].try_into().unwrap()) as usize,
            stripped.len() - 8
        );
        assert_eq!(stripped.len(), plain.len() + vp8x.len());
        image::load_from_memory(&stripped).unwrap();
    }

    #[test]
    fn other_files_are_left_alone() {
        assert_eq!(strip_metadata(b"PK\x03\x04 a zip file"), None);
        assert_eq!(strip_metadata(b""), None);
        assert_eq!(strip_metadata(&PNG_SIGNATURE), None);
    }
}
