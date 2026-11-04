//! Images on the system clipboard (media uploads plan, Task 21): pasted into
//! the composer as PNG, and put there by "Copy image".

use crate::api::types::CoreError;

pub use system::{image, set_image};

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod system {
    use std::borrow::Cow;
    use std::io::Cursor;
    use std::sync::{Mutex, PoisonError};

    use arboard::{Clipboard, Error, ImageData};

    use super::{CoreError, failed};

    /// The same limits as the server's image processing.
    const MAX_SIDE: u32 = 16_384;
    const MAX_PIXELS: u64 = 50_000_000;
    const MAX_ALLOC: u64 = 512 * 1024 * 1024;

    /// Kept for the app's life: on Linux a copied image is served from this
    /// process, and goes when its clipboard is dropped.
    static CLIPBOARD: Mutex<Option<Clipboard>> = Mutex::new(None);

    fn with_clipboard<T>(
        work: impl FnOnce(&mut Clipboard) -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        let mut held = CLIPBOARD.lock().unwrap_or_else(PoisonError::into_inner);
        let clipboard = match &mut *held {
            Some(clipboard) => clipboard,
            empty => empty.insert(Clipboard::new().map_err(failed)?),
        };
        work(clipboard)
    }

    /// The clipboard's image as PNG; `None` when it holds text or nothing.
    pub fn image() -> Result<Option<Vec<u8>>, CoreError> {
        with_clipboard(|clipboard| {
            if clipboard.get_text().is_ok_and(|text| !text.is_empty()) {
                return Ok(None);
            }
            let picture = match clipboard.get_image() {
                Ok(picture) => picture,
                Err(Error::ContentNotAvailable) => return Ok(None),
                Err(error) => return Err(failed(error)),
            };
            let width = u32::try_from(picture.width).map_err(failed)?;
            let height = u32::try_from(picture.height).map_err(failed)?;
            let rgba = image::RgbaImage::from_raw(width, height, picture.bytes.into_owned())
                .ok_or_else(|| failed("the clipboard's image has the wrong size"))?;
            let mut png = Vec::new();
            rgba.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
                .map_err(failed)?;
            Ok(Some(png))
        })
    }

    /// Puts a PNG, JPEG, GIF or WebP on the clipboard.
    pub fn set_image(bytes: &[u8]) -> Result<(), CoreError> {
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(MAX_SIDE);
        limits.max_image_height = Some(MAX_SIDE);
        limits.max_alloc = Some(MAX_ALLOC);
        let mut reader = image::ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(failed)?;
        reader.limits(limits);
        let picture = reader.decode().map_err(failed)?;
        if u64::from(picture.width()) * u64::from(picture.height()) > MAX_PIXELS {
            return Err(failed("the image is too large"));
        }
        let rgba = picture.to_rgba8();
        let data = ImageData {
            width: rgba.width() as usize,
            height: rgba.height() as usize,
            bytes: Cow::Owned(rgba.into_raw()),
        };
        with_clipboard(|clipboard| clipboard.set_image(data).map_err(failed))
    }
}

/// Windows' clipboard, on the Win32 API directly (media uploads plan, Task 21,
/// settled for Windows in P1). `arboard` would do this, but on Windows it
/// brings `clipboard-win` and `error-code`, both BSL-1.0, which the plan's
/// licence list doesn't allow; `windows` is already a dependency here.
///
/// Images travel on the clipboard as DIBs, not as files. Reading prefers the
/// registered `PNG` format (browsers put one there), then `CF_DIBV5`, then
/// `CF_DIB`. Writing puts both a `PNG` and a `CF_DIBV5`, so browsers and
/// Paint alike find something they understand.
#[cfg(target_os = "windows")]
mod system {
    use std::io::Cursor;

    // GlobalFree lives in Foundation, not Memory, unlike the rest of the
    // Global* family.
    use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable,
        OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
    };
    use windows::Win32::System::Memory::{
        GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
    };
    use windows::core::PCWSTR;

    use super::{CoreError, failed};

    /// The same limits as the server's image processing, as on Linux.
    const MAX_SIDE: u32 = 16_384;
    const MAX_PIXELS: u64 = 50_000_000;
    const MAX_ALLOC: u64 = 512 * 1024 * 1024;

    const CF_DIB: u32 = 8;
    const CF_DIBV5: u32 = 17;
    const CF_UNICODETEXT: u32 = 13;

    const BI_RGB: u32 = 0;
    const BI_BITFIELDS: u32 = 3;

    const INFO_HEADER: usize = 40;
    const V5_HEADER: usize = 124;

    /// `RegisterClipboardFormatW("PNG")`, the format Chrome and Firefox use.
    fn png_format() -> u32 {
        let name: Vec<u16> = "PNG\0".encode_utf16().collect();
        // Registering the same name twice returns the same id, so there is no
        // need to cache it; it fails only when the atom table is full.
        unsafe { RegisterClipboardFormatW(PCWSTR(name.as_ptr())) }
    }

    /// Holds the clipboard open for one piece of work and always closes it.
    /// Another process may own it for a moment, so opening is retried.
    struct Clipboard;

    impl Clipboard {
        fn open() -> Result<Self, CoreError> {
            let mut last = None;
            for attempt in 0..10 {
                match unsafe { OpenClipboard(None) } {
                    Ok(()) => return Ok(Self),
                    Err(error) => {
                        last = Some(error);
                        std::thread::sleep(std::time::Duration::from_millis(10 * (attempt + 1)));
                    }
                }
            }
            Err(failed(last.map_or_else(
                || "the clipboard could not be opened".to_string(),
                |error| error.to_string(),
            )))
        }
    }

    impl Drop for Clipboard {
        fn drop(&mut self) {
            // Nothing to do about a failure while closing.
            let _ = unsafe { CloseClipboard() };
        }
    }

    /// The bytes of one clipboard format, copied out while the clipboard is
    /// held. The handle itself belongs to the clipboard and is never freed.
    fn format_bytes(format: u32) -> Result<Option<Vec<u8>>, CoreError> {
        if unsafe { IsClipboardFormatAvailable(format) }.is_err() {
            return Ok(None);
        }
        let handle = match unsafe { GetClipboardData(format) } {
            Ok(handle) if !handle.is_invalid() => handle,
            // Available but unreadable: treat it as absent rather than fail,
            // so a later format still gets its turn.
            _ => return Ok(None),
        };
        let global = HGLOBAL(handle.0);
        let size = unsafe { GlobalSize(global) };
        if size == 0 {
            return Ok(None);
        }
        if size as u64 > MAX_ALLOC {
            return Err(failed("the clipboard's image is too large"));
        }
        let locked = unsafe { GlobalLock(global) };
        if locked.is_null() {
            return Ok(None);
        }
        let bytes = unsafe { std::slice::from_raw_parts(locked.cast::<u8>(), size) }.to_vec();
        let _ = unsafe { GlobalUnlock(global) };
        Ok(Some(bytes))
    }

    /// True while the clipboard holds non-empty text, which wins over an
    /// image exactly as it does on Linux.
    fn holds_text() -> bool {
        match format_bytes(CF_UNICODETEXT) {
            Ok(Some(bytes)) => {
                // CF_UNICODETEXT is UTF-16 and NUL-terminated: text is there
                // only if the very first unit is not the terminator.
                bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| u16::from_le_bytes(*pair))
                    .next()
                    .is_some_and(|first| first != 0)
            }
            _ => false,
        }
    }

    fn u32_at(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ])
    }

    fn i32_at(bytes: &[u8], offset: usize) -> i32 {
        u32_at(bytes, offset) as i32
    }

    /// A packed DIB (`CF_DIB` or `CF_DIBV5`) as RGBA.
    ///
    /// Only the shapes that really turn up on the clipboard are read: 32-bit
    /// `BI_RGB` and `BI_BITFIELDS`, and 24-bit `BI_RGB`. Anything else (a
    /// palette, 16-bit, RLE) is reported rather than guessed at.
    pub(super) fn dib_to_rgba(dib: &[u8]) -> Result<image::RgbaImage, CoreError> {
        if dib.len() < INFO_HEADER {
            return Err(failed("the clipboard's image header is too short"));
        }
        let header = u32_at(dib, 0) as usize;
        if header < INFO_HEADER || header > dib.len() {
            return Err(failed("the clipboard's image has an odd header"));
        }
        let width = i32_at(dib, 4);
        let raw_height = i32_at(dib, 8);
        let bpp = u32::from(u16::from_le_bytes([dib[14], dib[15]]));
        let compression = u32_at(dib, 16);

        if width <= 0 || raw_height == 0 {
            return Err(failed("the clipboard's image is empty"));
        }
        // A negative height means the rows are stored top-down.
        let top_down = raw_height < 0;
        let height = raw_height.unsigned_abs();
        let width = width as u32;
        if width > MAX_SIDE || height > MAX_SIDE {
            return Err(failed("the clipboard's image is too large"));
        }
        if u64::from(width) * u64::from(height) > MAX_PIXELS {
            return Err(failed("the clipboard's image is too large"));
        }
        if compression != BI_RGB && compression != BI_BITFIELDS {
            return Err(failed("the clipboard's image is compressed"));
        }
        if bpp != 32 && bpp != 24 {
            return Err(failed(format!(
                "the clipboard's image is {bpp} bits a pixel, which isn't read"
            )));
        }

        // BI_BITFIELDS on a BITMAPINFOHEADER puts three masks after it; a
        // BITMAPV5HEADER carries its masks inside the header instead.
        let masks = if compression == BI_BITFIELDS && header == INFO_HEADER {
            12
        } else {
            0
        };
        let pixels = &dib[header + masks..];
        // Each row is padded to a 4-byte boundary.
        let stride = (u64::from(width) * u64::from(bpp)).div_ceil(32) as usize * 4;
        let needed = stride
            .checked_mul(height as usize)
            .ok_or_else(|| failed("the clipboard's image is too large"))?;
        if pixels.len() < needed {
            return Err(failed("the clipboard's image is shorter than its header"));
        }

        let bytes = (bpp / 8) as usize;
        let mut rgba = image::RgbaImage::new(width, height);
        // 32-bit DIBs often carry a zeroed alpha channel. Honouring it would
        // paste a wholly transparent picture, so alpha is only believed when
        // some pixel actually uses it.
        let mut any_alpha = false;
        for y in 0..height {
            let row = if top_down { y } else { height - 1 - y };
            let start = row as usize * stride;
            for x in 0..width {
                let at = start + x as usize * bytes;
                let blue = pixels[at];
                let green = pixels[at + 1];
                let red = pixels[at + 2];
                let alpha = if bytes == 4 { pixels[at + 3] } else { 255 };
                if alpha != 0 {
                    any_alpha = true;
                }
                rgba.put_pixel(x, y, image::Rgba([red, green, blue, alpha]));
            }
        }
        if bytes == 4 && !any_alpha {
            for pixel in rgba.pixels_mut() {
                pixel.0[3] = 255;
            }
        }
        Ok(rgba)
    }

    /// RGBA as a packed `CF_DIBV5`: a `BITMAPV5HEADER` and top-down BGRA.
    pub(super) fn rgba_to_dibv5(rgba: &image::RgbaImage) -> Vec<u8> {
        let width = rgba.width();
        let height = rgba.height();
        let mut out = vec![0u8; V5_HEADER + (width as usize * height as usize * 4)];
        fn put(out: &mut [u8], at: usize, value: u32) {
            out[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        put(&mut out, 0, V5_HEADER as u32); // bV5Size
        put(&mut out, 4, width);
        // Negative: top-down, the order the pixels are written below.
        put(&mut out, 8, (height as i32).wrapping_neg() as u32);
        out[12..14].copy_from_slice(&1u16.to_le_bytes()); // bV5Planes
        out[14..16].copy_from_slice(&32u16.to_le_bytes()); // bV5BitCount
        put(&mut out, 16, BI_BITFIELDS);
        put(&mut out, 20, width * height * 4); // bV5SizeImage
        put(&mut out, 40, 0x00FF_0000); // bV5RedMask
        put(&mut out, 44, 0x0000_FF00); // bV5GreenMask
        put(&mut out, 48, 0x0000_00FF); // bV5BlueMask
        put(&mut out, 52, 0xFF00_0000); // bV5AlphaMask
        put(&mut out, 56, 0x7352_4742); // bV5CSType: 'BGRs', sRGB
        put(&mut out, 108, 4); // bV5Intent: LCS_GM_IMAGES

        for (index, pixel) in rgba.pixels().enumerate() {
            let [red, green, blue, alpha] = pixel.0;
            let at = V5_HEADER + index * 4;
            out[at] = blue;
            out[at + 1] = green;
            out[at + 2] = red;
            out[at + 3] = alpha;
        }
        out
    }

    /// Decodes clipboard or caller bytes under the same limits as everything else.
    fn decode_limited(bytes: &[u8]) -> Result<image::DynamicImage, CoreError> {
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(MAX_SIDE);
        limits.max_image_height = Some(MAX_SIDE);
        limits.max_alloc = Some(MAX_ALLOC);
        let mut reader = image::ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(failed)?;
        reader.limits(limits);
        let picture = reader.decode().map_err(failed)?;
        if u64::from(picture.width()) * u64::from(picture.height()) > MAX_PIXELS {
            return Err(failed("the image is too large"));
        }
        Ok(picture)
    }

    fn to_png(rgba: &image::RgbaImage) -> Result<Vec<u8>, CoreError> {
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(rgba.clone())
            .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .map_err(failed)?;
        Ok(png)
    }

    /// The clipboard's image as PNG; `None` when it holds text or nothing.
    pub fn image() -> Result<Option<Vec<u8>>, CoreError> {
        let _clipboard = Clipboard::open()?;
        if holds_text() {
            return Ok(None);
        }
        // A PNG on the clipboard is already what the caller wants, but it is
        // still decoded, so a broken or oversized one is caught here.
        if let Some(bytes) = format_bytes(png_format())? {
            let picture = decode_limited(&bytes)?;
            return Ok(Some(to_png(&picture.to_rgba8())?));
        }
        for format in [CF_DIBV5, CF_DIB] {
            if let Some(dib) = format_bytes(format)? {
                return Ok(Some(to_png(&dib_to_rgba(&dib)?)?));
            }
        }
        Ok(None)
    }

    /// Hands one format's bytes to the clipboard, which takes the memory on.
    fn offer(format: u32, bytes: &[u8]) -> Result<(), CoreError> {
        let global = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) }.map_err(failed)?;
        let locked = unsafe { GlobalLock(global) };
        if locked.is_null() {
            let _ = unsafe { GlobalFree(Some(global)) };
            return Err(failed("the clipboard's memory could not be locked"));
        }
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), locked.cast::<u8>(), bytes.len()) };
        let _ = unsafe { GlobalUnlock(global) };
        // On success the clipboard owns the block; on failure it is still
        // ours to free.
        match unsafe { SetClipboardData(format, Some(HANDLE(global.0))) } {
            Ok(_) => Ok(()),
            Err(error) => {
                let _ = unsafe { GlobalFree(Some(global)) };
                Err(failed(error))
            }
        }
    }

    /// Puts a PNG, JPEG, GIF or WebP on the clipboard.
    pub fn set_image(bytes: &[u8]) -> Result<(), CoreError> {
        let rgba = decode_limited(bytes)?.to_rgba8();
        let dib = rgba_to_dibv5(&rgba);
        let png = to_png(&rgba)?;

        let _clipboard = Clipboard::open()?;
        unsafe { EmptyClipboard() }.map_err(failed)?;
        // CF_DIBV5 first: it is what Paint and Explorer look for. Windows
        // synthesises CF_DIB and CF_BITMAP from it.
        offer(CF_DIBV5, &dib)?;
        offer(png_format(), &png)?;
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn picture() -> image::RgbaImage {
            image::RgbaImage::from_fn(5, 3, |x, y| {
                image::Rgba([(x * 40) as u8, (y * 60) as u8, 7, 255])
            })
        }

        #[test]
        fn a_dibv5_round_trips_through_rgba() {
            let before = picture();
            let after = dib_to_rgba(&rgba_to_dibv5(&before)).expect("a DIB this code wrote");
            assert_eq!(before, after, "the picture changed on the way round");
        }

        #[test]
        fn alpha_survives_the_round_trip() {
            let before =
                image::RgbaImage::from_fn(4, 2, |x, _| image::Rgba([1, 2, 3, (x * 80) as u8]));
            let after = dib_to_rgba(&rgba_to_dibv5(&before)).expect("a DIB this code wrote");
            assert_eq!(before, after, "the alpha channel changed");
        }

        /// Screenshots often arrive 32-bit with every alpha byte zero. Taken
        /// literally that is an invisible picture, so it must come back opaque.
        #[test]
        fn a_zeroed_alpha_channel_is_read_as_opaque() {
            let mut dib = rgba_to_dibv5(&picture());
            for index in 0..(5 * 3) {
                dib[V5_HEADER + index * 4 + 3] = 0;
            }
            let after = dib_to_rgba(&dib).expect("a DIB this code wrote");
            assert!(
                after.pixels().all(|pixel| pixel.0[3] == 255),
                "a zeroed alpha channel should be read as opaque"
            );
        }

        /// Bottom-up rows are the default on the clipboard, so they must be
        /// turned the right way up.
        #[test]
        fn bottom_up_rows_are_turned_over() {
            let before = picture();
            let mut dib = rgba_to_dibv5(&before);
            // Say bottom-up, and reverse the rows to match.
            dib[8..12].copy_from_slice(&3u32.to_le_bytes());
            let stride = 5 * 4;
            let pixels = dib[V5_HEADER..].to_vec();
            for row in 0..3usize {
                let from = (2 - row) * stride;
                dib[V5_HEADER + row * stride..V5_HEADER + (row + 1) * stride]
                    .copy_from_slice(&pixels[from..from + stride]);
            }
            let after = dib_to_rgba(&dib).expect("a DIB this code wrote");
            assert_eq!(before, after, "bottom-up rows came out upside down");
        }

        #[test]
        fn a_24_bit_dib_is_read() {
            // 2x2, BI_RGB, 24-bit, bottom-up, rows padded to 4 bytes.
            let mut dib = vec![0u8; INFO_HEADER + 16];
            dib[0..4].copy_from_slice(&(INFO_HEADER as u32).to_le_bytes());
            dib[4..8].copy_from_slice(&2u32.to_le_bytes());
            dib[8..12].copy_from_slice(&2u32.to_le_bytes());
            dib[12..14].copy_from_slice(&1u16.to_le_bytes());
            dib[14..16].copy_from_slice(&24u16.to_le_bytes());
            dib[16..20].copy_from_slice(&BI_RGB.to_le_bytes());
            // Bottom row first: blue, green. Then top row: red, white.
            let rows: [[u8; 3]; 4] = [
                [255, 0, 0], // blue  (BGR)
                [0, 255, 0], // green
                [0, 0, 255], // red
                [255, 255, 255],
            ];
            for (index, bgr) in rows.iter().enumerate() {
                let at = INFO_HEADER + (index / 2) * 8 + (index % 2) * 3;
                dib[at..at + 3].copy_from_slice(bgr);
            }
            let rgba = dib_to_rgba(&dib).expect("a 24-bit DIB");
            assert_eq!(rgba.dimensions(), (2, 2));
            assert_eq!(rgba.get_pixel(0, 0).0, [255, 0, 0, 255], "top left is red");
            assert_eq!(
                rgba.get_pixel(0, 1).0,
                [0, 0, 255, 255],
                "bottom left is blue"
            );
        }

        #[test]
        fn a_palette_dib_is_reported_not_guessed_at() {
            let mut dib = vec![0u8; INFO_HEADER + 16];
            dib[0..4].copy_from_slice(&(INFO_HEADER as u32).to_le_bytes());
            dib[4..8].copy_from_slice(&2u32.to_le_bytes());
            dib[8..12].copy_from_slice(&2u32.to_le_bytes());
            dib[14..16].copy_from_slice(&8u16.to_le_bytes());
            let error = dib_to_rgba(&dib).expect_err("8-bit is not read");
            assert!(
                format!("{error:?}").contains("8 bits a pixel"),
                "the error should say what it found: {error:?}"
            );
        }

        #[test]
        fn a_dib_shorter_than_its_header_claims_is_refused() {
            let mut dib = rgba_to_dibv5(&picture());
            dib.truncate(V5_HEADER + 8);
            dib_to_rgba(&dib).expect_err("a truncated DIB is refused");
        }

        #[test]
        fn an_oversized_dib_is_refused_before_it_is_allocated() {
            let mut dib = rgba_to_dibv5(&picture());
            dib[4..8].copy_from_slice(&40_000u32.to_le_bytes());
            dib[8..12].copy_from_slice(&40_000u32.to_le_bytes());
            dib_to_rgba(&dib).expect_err("a huge DIB is refused");
        }
    }
}

/// Systems without a clipboard backend.
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod system {
    use super::CoreError;

    pub fn image() -> Result<Option<Vec<u8>>, CoreError> {
        Ok(None)
    }

    pub fn set_image(_bytes: &[u8]) -> Result<(), CoreError> {
        Err(super::failed("this system's clipboard isn't supported"))
    }
}

fn failed(error: impl std::fmt::Display) -> CoreError {
    CoreError::InvalidInput {
        message: format!("the clipboard couldn't be used: {error}"),
    }
}
