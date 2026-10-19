//! Images on the system clipboard (media uploads plan, Task 21): pasted into
//! the composer as PNG, and put there by "Copy image".

use crate::api::types::CoreError;

pub use system::{image, set_image};

#[cfg(target_os = "linux")]
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

/// Systems without a clipboard backend yet: macOS gets one in P2, Windows
/// in P1.
#[cfg(not(target_os = "linux"))]
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
