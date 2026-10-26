//! Screen and window capture with GDI, for the Windows that
//! Windows.Graphics.Capture does not work on (beside `windows.rs`): builds
//! before 1903, editions and stripped-down installs that lack it or refuse
//! it, and machines where its session opens but never sends a frame.
//!
//! It is what WebRTC falls back to as well. A screen is copied from the
//! desktop with `BitBlt`, and the cursor drawn on it; a window draws itself
//! with `PrintWindow`, which, with full content asked for, also covers
//! windows that render through Direct3D. It costs more than the capture
//! API, so it is the fallback and never the first choice.

use std::ffi::c_void;
use std::time::Instant;

use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleDC, CreateDIBSection,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, GdiFlush, GetDC, GetMonitorInfoW, GetWindowDC, HBITMAP,
    HDC, HGDIOBJ, HMONITOR, MONITORINFO, ReleaseDC, SRCCOPY, SelectObject,
};
use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
use windows::Win32::UI::WindowsAndMessaging::{
    CURSOR_SHOWING, CURSORINFO, DI_NORMAL, DrawIconEx, GetCursorInfo, GetIconInfo,
    GetWindowPlacement, GetWindowRect, HICON, ICONINFO, IsIconic, WINDOWPLACEMENT,
};

use crate::video::camera::{RawFormat, RawFrame};

/// `PW_RENDERFULLCONTENT`, as in `windows_gdi.rs`.
const PW_RENDERFULLCONTENT: PRINT_WINDOW_FLAGS = PRINT_WINDOW_FLAGS(2);

/// What is copied.
#[derive(Clone, Copy)]
pub(super) enum Target {
    Screen(HMONITOR),
    Window(HWND),
}

/// Copies one screen or window, a frame at a time, on one thread.
pub(super) struct Grabber {
    target: Target,
    /// Kept between frames and made again when the size changes.
    canvas: Option<Canvas>,
}

impl Grabber {
    pub(super) fn new(target: Target) -> Self {
        Self {
            target,
            canvas: None,
        }
    }

    /// Where the source is on the desktop now, or `None` when there is
    /// nothing to copy: the monitor is gone, or the window minimised.
    pub(super) fn rect(&self) -> Option<RECT> {
        let rect = match self.target {
            Target::Screen(monitor) => {
                let mut info = MONITORINFO {
                    cbSize: size_of::<MONITORINFO>() as u32,
                    ..Default::default()
                };
                // SAFETY: `info` is sized as the call requires.
                unsafe { GetMonitorInfoW(monitor, &mut info) }
                    .as_bool()
                    .then_some(info.rcMonitor)?
            }
            Target::Window(window) => {
                // SAFETY: `IsIconic` takes any handle; the rectangle is
                // filled in by a call that has just been told the window.
                unsafe {
                    if IsIconic(window).as_bool() {
                        return None;
                    }
                    let mut rect = RECT::default();
                    GetWindowRect(window, &mut rect).ok()?;
                    rect
                }
            }
        };
        let (width, height) = size_of_rect(rect);
        (width > 0 && height > 0).then_some(rect)
    }

    /// The source's size now; a minimised window's is the size it will
    /// have again when restored.
    pub(super) fn size(&self) -> Option<(u32, u32)> {
        if let Some(rect) = self.rect() {
            return Some(size_of_rect(rect));
        }
        let Target::Window(window) = self.target else {
            return None;
        };
        let mut placement = WINDOWPLACEMENT {
            length: size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        // SAFETY: `placement` is sized as the call requires.
        unsafe { GetWindowPlacement(window, &mut placement) }.ok()?;
        let size = size_of_rect(placement.rcNormalPosition);
        (size.0 > 0 && size.1 > 0).then_some(size)
    }

    /// One frame of the source, or `None` when there was nothing to copy.
    pub(super) fn grab(&mut self) -> Option<RawFrame> {
        let rect = self.rect()?;
        let captured = Instant::now();
        let size = size_of_rect(rect);
        if self
            .canvas
            .as_ref()
            .is_none_or(|canvas| canvas.size != size)
        {
            self.canvas = None;
            self.canvas = Canvas::new(size);
        }
        let canvas = self.canvas.as_ref()?;
        let copied = match self.target {
            Target::Screen(_) => copy_screen(canvas, rect),
            Target::Window(window) => copy_window(canvas, window, size),
        };
        if !copied {
            return None;
        }
        draw_cursor(canvas, rect);
        Some(RawFrame {
            format: RawFormat::Bgrx,
            width: size.0,
            height: size.1,
            data: canvas.pixels(),
            captured,
        })
    }
}

fn size_of_rect(rect: RECT) -> (u32, u32) {
    (
        (rect.right - rect.left).max(0) as u32,
        (rect.bottom - rect.top).max(0) as u32,
    )
}

/// A memory DC with a bitmap whose pixels are ours to read: 32 bits a
/// pixel, top row first, so its rows are exactly a [`RawFormat::Bgrx`]
/// frame's.
struct Canvas {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *const u8,
    size: (u32, u32),
}

impl Canvas {
    fn new(size: (u32, u32)) -> Option<Self> {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size.0 as i32,
                // Negative: rows top down.
                biHeight: -(size.1 as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        // SAFETY: the bitmap and DC made here are deleted in `Drop`, or
        // right here when the other could not be made; `bits` points into
        // the bitmap for as long as it lives.
        unsafe {
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() {
                return None;
            }
            let mut bits: *mut c_void = std::ptr::null_mut();
            let bitmap = match CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)
            {
                Ok(bitmap) if !bits.is_null() => bitmap,
                _ => {
                    let _ = DeleteDC(dc);
                    return None;
                }
            };
            let previous = SelectObject(dc, HGDIOBJ(bitmap.0));
            Some(Self {
                dc,
                bitmap,
                previous,
                bits: bits.cast(),
                size,
            })
        }
    }

    /// The pixels drawn so far.
    fn pixels(&self) -> Vec<u8> {
        let length = self.size.0 as usize * self.size.1 as usize * 4;
        // SAFETY: GDI has finished drawing once flushed, and the bitmap is
        // `length` bytes: 32 bits a pixel, with no padding at that depth.
        unsafe {
            let _ = GdiFlush();
            std::slice::from_raw_parts(self.bits, length).to_vec()
        }
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        // SAFETY: made in `new`; the bitmap is selected out before it is
        // deleted.
        unsafe {
            SelectObject(self.dc, self.previous);
            let _ = DeleteObject(HGDIOBJ(self.bitmap.0));
            let _ = DeleteDC(self.dc);
        }
    }
}

/// Copies the part of the desktop a monitor shows.
fn copy_screen(canvas: &Canvas, rect: RECT) -> bool {
    let (width, height) = size_of_rect(rect);
    // SAFETY: the desktop DC is released before returning; the canvas is
    // the size of the rectangle copied.
    unsafe {
        let desktop = GetDC(None);
        if desktop.is_invalid() {
            return false;
        }
        // `CAPTUREBLT` takes in layered windows too, as WebRTC does.
        let copied = BitBlt(
            canvas.dc,
            0,
            0,
            width as i32,
            height as i32,
            Some(desktop),
            rect.left,
            rect.top,
            SRCCOPY | CAPTUREBLT,
        )
        .is_ok();
        ReleaseDC(None, desktop);
        copied
    }
}

/// Has a window draw itself, falling back to copying its DC when it will
/// not.
fn copy_window(canvas: &Canvas, window: HWND, size: (u32, u32)) -> bool {
    // SAFETY: a window handle the capture thread checks is still alive;
    // the window DC is released before returning.
    unsafe {
        if PrintWindow(window, canvas.dc, PW_RENDERFULLCONTENT).as_bool() {
            return true;
        }
        let from = GetWindowDC(Some(window));
        if from.is_invalid() {
            return false;
        }
        let copied = BitBlt(
            canvas.dc,
            0,
            0,
            size.0 as i32,
            size.1 as i32,
            Some(from),
            0,
            0,
            SRCCOPY,
        )
        .is_ok();
        ReleaseDC(Some(window), from);
        copied
    }
}

/// Draws the cursor where it is over the source, as the capture API does.
/// GDI's copies never have it.
fn draw_cursor(canvas: &Canvas, rect: RECT) {
    let mut cursor = CURSORINFO {
        cbSize: size_of::<CURSORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: `cursor` is sized as the call requires; the icon's two
    // bitmaps, which `GetIconInfo` makes copies of, are deleted.
    unsafe {
        if GetCursorInfo(&mut cursor).is_err() || cursor.flags.0 & CURSOR_SHOWING.0 == 0 {
            return;
        }
        let icon = HICON(cursor.hCursor.0);
        let mut info = ICONINFO::default();
        let hotspot = if GetIconInfo(icon, &mut info).is_ok() {
            if !info.hbmMask.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
            }
            if !info.hbmColor.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
            }
            POINT {
                x: info.xHotspot as i32,
                y: info.yHotspot as i32,
            }
        } else {
            POINT::default()
        };
        let at = POINT {
            x: cursor.ptScreenPos.x - rect.left - hotspot.x,
            y: cursor.ptScreenPos.y - rect.top - hotspot.y,
        };
        let _ = DrawIconEx(canvas.dc, at.x, at.y, icon, 0, 0, 0, None, DI_NORMAL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::screen::windows_gdi as gdi;

    #[test]
    fn a_screen_comes_out_its_own_size_in_packed_rows() {
        let screen = gdi::monitors().into_iter().next().expect("a monitor");
        let mut grabber = Grabber::new(Target::Screen(screen.handle));
        let size = grabber.size().expect("a monitor has a size");
        assert_eq!(size, size_of_rect(screen.rect));

        let frame = grabber.grab().expect("a frame");
        assert_eq!(frame.format, RawFormat::Bgrx);
        assert_eq!((frame.width, frame.height), size);
        assert_eq!(frame.data.len(), size.0 as usize * size.1 as usize * 4);
        // The canvas is kept for the next frame of the same size.
        assert!(grabber.grab().is_some(), "and the next one");
    }

    #[test]
    fn nothing_is_copied_from_a_window_that_is_not_there() {
        let grabber = Grabber::new(Target::Window(HWND(std::ptr::null_mut())));
        assert_eq!(grabber.size(), None);
    }
}
