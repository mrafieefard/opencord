//! A picture in memory: a camera's frames, the encoders' input and the
//! decoders' output (plan §7.8). Planes are packed one after another with
//! no padding between rows; widths and heights are even.

use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// Luma, then blue and red difference interleaved at half the width and
    /// height.
    Nv12,
    /// Luma, then blue difference, then red difference, both at half the
    /// width and height.
    I420,
}

#[derive(Clone)]
pub struct Picture {
    pub format: PixelFormat,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    /// When it was captured; every layer made from it keeps this.
    pub captured: Instant,
}

impl std::fmt::Debug for Picture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Picture")
            .field("format", &self.format)
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

impl Picture {
    /// Bytes a picture of this size takes.
    pub fn bytes(width: u32, height: u32) -> usize {
        let pixels = width as usize * height as usize;
        pixels + pixels / 2
    }

    /// A black picture (video range: luma 16, chroma 128).
    pub fn black(format: PixelFormat, width: u32, height: u32, captured: Instant) -> Self {
        let luma = width as usize * height as usize;
        let mut data = vec![128; Self::bytes(width, height)];
        data[..luma].fill(16);
        Self {
            format,
            width,
            height,
            data,
            captured,
        }
    }

    /// Wraps packed planes; `None` unless the size is even and the data
    /// fits it exactly.
    pub fn from_data(
        format: PixelFormat,
        width: u32,
        height: u32,
        data: Vec<u8>,
        captured: Instant,
    ) -> Option<Self> {
        let even = width.is_multiple_of(2) && height.is_multiple_of(2) && width > 0 && height > 0;
        (even && data.len() == Self::bytes(width, height)).then_some(Self {
            format,
            width,
            height,
            data,
            captured,
        })
    }

    pub fn y(&self) -> &[u8] {
        &self.data[..self.width as usize * self.height as usize]
    }

    /// Each plane with its row length in bytes.
    pub fn planes(&self) -> Vec<(&[u8], usize)> {
        let width = self.width as usize;
        let luma = width * self.height as usize;
        let (y, chroma) = self.data.split_at(luma);
        match self.format {
            PixelFormat::Nv12 => vec![(y, width), (chroma, width)],
            PixelFormat::I420 => {
                let (u, v) = chroma.split_at(luma / 4);
                vec![(y, width), (u, width / 2), (v, width / 2)]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_holds_a_luma_plane_and_quarter_size_chroma() {
        let picture = Picture::black(PixelFormat::Nv12, 320, 180, Instant::now());

        assert_eq!(picture.data.len(), 320 * 180 * 3 / 2);
        assert_eq!(picture.y().len(), 320 * 180);
        assert!(picture.y().iter().all(|&y| y == 16));
        assert!(picture.data[320 * 180..].iter().all(|&c| c == 128));
    }

    #[test]
    fn nv12_has_one_interleaved_chroma_plane_and_i420_two() {
        let nv12 = Picture::black(PixelFormat::Nv12, 64, 32, Instant::now());
        let i420 = Picture::black(PixelFormat::I420, 64, 32, Instant::now());

        assert_eq!(nv12.planes().len(), 2);
        assert_eq!(nv12.planes()[1], (&nv12.data[64 * 32..], 64));
        assert_eq!(i420.planes().len(), 3);
        assert_eq!(i420.planes()[1].0.len(), 32 * 16);
        assert_eq!(i420.planes()[2], (&i420.data[64 * 32 + 32 * 16..], 32));
    }

    #[test]
    fn odd_sizes_are_refused() {
        assert!(
            Picture::from_data(PixelFormat::Nv12, 63, 32, vec![0; 3000], Instant::now()).is_none()
        );
        assert!(
            Picture::from_data(PixelFormat::Nv12, 64, 32, vec![0; 10], Instant::now()).is_none()
        );
        assert!(
            Picture::from_data(PixelFormat::I420, 64, 32, vec![0; 3072], Instant::now()).is_some()
        );
    }
}
