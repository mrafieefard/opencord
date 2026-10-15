//! What Opencord's own picker needs to know about this desktop
//! (docs/plans/windows.md W6, step 1), beside `windows.rs`: which monitors
//! and windows there are, which app a window belongs to, and a small
//! picture of each.
//!
//! All of it is GDI and plain Win32 rather than Windows.Graphics.Capture:
//! a capture session per source would light up every window's capture
//! border for as long as the picker is open, which is not what a list of
//! thumbnails should do.

use std::ffi::c_void;

use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, MAX_PATH, RECT, TRUE};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleBitmap, CreateCompatibleDC,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, EnumDisplayMonitors, GetDC, GetDIBits, GetMonitorInfoW,
    HALFTONE, HBITMAP, HDC, HGDIOBJ, HMONITOR, MONITORINFO, MONITORINFOEXW, ReleaseDC, SRCCOPY,
    SelectObject, SetStretchBltMode, StretchBlt,
};
use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};
use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, EnumWindows, GWL_EXSTYLE, GetClassNameW, GetWindowLongPtrW, GetWindowRect,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindow, IsWindowVisible,
    WS_EX_TOOLWINDOW,
};
use windows::core::PCWSTR;

use super::Thumbnail;

/// The biggest a thumbnail may be (W6 step 1).
const THUMBNAIL: (i32, i32) = (320, 180);

/// `PW_RENDERFULLCONTENT`: draws a window that renders itself through
/// Direct3D, which a plain `PrintWindow` comes back empty for.
const PW_RENDERFULLCONTENT: PRINT_WINDOW_FLAGS = PRINT_WINDOW_FLAGS(2);

/// The class a UWP app's real window has inside `ApplicationFrameHost`
/// (windows.md §3, decision E).
const CORE_WINDOW: &str = "Windows.UI.Core.CoreWindow";

pub(super) struct Monitor {
    pub(super) handle: HMONITOR,
    /// `\\.\DISPLAY1`, which is what the source id is built from.
    pub(super) device: String,
    pub(super) rect: RECT,
}

pub(super) struct Window {
    pub(super) handle: HWND,
    pub(super) title: String,
    /// The exe's `FileDescription`, which is what people call the app.
    pub(super) app: Option<String>,
}

/// A window handle as a source id, and back.
pub(super) fn handle_id(window: HWND) -> isize {
    window.0 as isize
}

pub(super) fn window_of_id(id: &str) -> Option<HWND> {
    id.parse::<isize>().ok().map(|raw| HWND(raw as *mut c_void))
}

pub(super) fn still_a_window(window: HWND) -> bool {
    // SAFETY: `IsWindow` is made for handles that may already be dead.
    unsafe { IsWindow(Some(window)) }.as_bool()
}

/// Every monitor, in the order Windows enumerates them.
pub(super) fn monitors() -> Vec<Monitor> {
    let mut found: Vec<Monitor> = Vec::new();
    // SAFETY: the callback is handed a pointer to `found`, which outlives
    // the call, and nothing keeps it afterwards.
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(add_monitor),
            LPARAM(std::ptr::from_mut(&mut found) as isize),
        );
    }
    found
}

unsafe extern "system" fn add_monitor(
    handle: HMONITOR,
    _dc: HDC,
    _clip: *mut RECT,
    into: LPARAM,
) -> windows::core::BOOL {
    // SAFETY: `into` is the `Vec` `monitors` passed, alive for this call.
    let found = unsafe { &mut *(into.0 as *mut Vec<Monitor>) };
    let mut info = MONITORINFOEXW {
        monitorInfo: MONITORINFO {
            cbSize: size_of::<MONITORINFOEXW>() as u32,
            ..Default::default()
        },
        ..Default::default()
    };
    // SAFETY: `info` is sized as `GetMonitorInfoW` requires, and the cast
    // to its base is what the API expects of `MONITORINFOEXW`.
    if unsafe { GetMonitorInfoW(handle, std::ptr::from_mut(&mut info).cast()) }.as_bool() {
        found.push(Monitor {
            handle,
            device: from_wide(&info.szDevice),
            rect: info.monitorInfo.rcMonitor,
        });
    }
    TRUE
}

/// The windows worth offering, topmost first — which is the order Windows
/// stacks them, so the one the user was last in comes first.
pub(super) fn shareable_windows() -> Vec<Window> {
    let mut found: Vec<Window> = Vec::new();
    // SAFETY: as `monitors`.
    unsafe {
        let _ = EnumWindows(
            Some(add_window),
            LPARAM(std::ptr::from_mut(&mut found) as isize),
        );
    }
    found
}

unsafe extern "system" fn add_window(window: HWND, into: LPARAM) -> windows::core::BOOL {
    // SAFETY: `into` is the `Vec` `shareable_windows` passed.
    let found = unsafe { &mut *(into.0 as *mut Vec<Window>) };
    if let Some(window) = worth_offering(window) {
        found.push(window);
    }
    // Always true: stopping early would cut the list short.
    TRUE
}

/// Whether a window belongs in the picker, and what to call it.
fn worth_offering(window: HWND) -> Option<Window> {
    // SAFETY: a handle Windows has just handed us in an enumeration.
    unsafe {
        if !IsWindowVisible(window).as_bool() {
            return None;
        }
        // A tool window is a palette or a tooltip, not something to share.
        let styles = GetWindowLongPtrW(window, GWL_EXSTYLE) as u32;
        if styles & WS_EX_TOOLWINDOW.0 != 0 {
            return None;
        }
        // Cloaked windows are "visible" but not on screen: a UWP app that
        // is suspended, or a window on another virtual desktop.
        let mut cloaked = 0u32;
        if DwmGetWindowAttribute(
            window,
            DWMWA_CLOAKED,
            std::ptr::from_mut(&mut cloaked).cast(),
            size_of::<u32>() as u32,
        )
        .is_ok()
            && cloaked != 0
        {
            return None;
        }
        // Nothing with an empty title: those are the invisible helpers
        // every app keeps around.
        let length = GetWindowTextLengthW(window);
        if length <= 0 {
            return None;
        }
        let mut text = vec![0u16; length as usize + 1];
        let written = GetWindowTextW(window, &mut text);
        if written <= 0 {
            return None;
        }
        // And never Opencord's own windows: sharing one is a hall of
        // mirrors.
        let mut pid = 0;
        GetWindowThreadProcessId(window, Some(&mut pid));
        if pid == GetCurrentProcessId() {
            return None;
        }
        // A zero-sized window has nothing to show.
        let mut rect = RECT::default();
        if GetWindowRect(window, &mut rect).is_err()
            || rect.right - rect.left < 2
            || rect.bottom - rect.top < 2
        {
            return None;
        }
        Some(Window {
            handle: window,
            title: from_wide(&text[..written as usize]),
            app: app_of_process(sound_process_of(window)?),
        })
    }
}

/// Which process's sound a window share should send (decision E). For a
/// UWP app the window belongs to `ApplicationFrameHost.exe`, and the app
/// itself is in the hosted `CoreWindow` child.
pub(super) fn sound_process_of(window: HWND) -> Option<u32> {
    let mut pid = 0;
    // SAFETY: a live window handle; `pid` is filled in.
    unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };
    if pid == 0 {
        return None;
    }
    if !exe_of_process(pid).is_some_and(|exe| exe.eq_ignore_ascii_case("ApplicationFrameHost.exe"))
    {
        return Some(pid);
    }
    let mut hosted = 0u32;
    // SAFETY: the callback writes one `u32` that outlives the call.
    unsafe {
        let _ = EnumChildWindows(
            Some(window),
            Some(find_core_window),
            LPARAM(std::ptr::from_mut(&mut hosted) as isize),
        );
    }
    Some(if hosted == 0 { pid } else { hosted })
}

unsafe extern "system" fn find_core_window(child: HWND, into: LPARAM) -> windows::core::BOOL {
    // SAFETY: `into` is the `u32` `sound_process_of` passed.
    let hosted = unsafe { &mut *(into.0 as *mut u32) };
    let mut class = [0u16; 128];
    // SAFETY: a handle from an enumeration, and a buffer of its own length.
    let written = unsafe { GetClassNameW(child, &mut class) };
    if written > 0 && from_wide(&class[..written as usize]) == CORE_WINDOW {
        let mut pid = 0;
        // SAFETY: as above.
        unsafe { GetWindowThreadProcessId(child, Some(&mut pid)) };
        if pid != 0 {
            *hosted = pid;
            // Found it; no need to look at the rest.
            return windows::core::BOOL(0);
        }
    }
    TRUE
}

/// A process's executable, by file name only.
fn exe_of_process(pid: u32) -> Option<String> {
    full_path_of_process(pid).and_then(|path| {
        std::path::Path::new(&path)
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
    })
}

fn full_path_of_process(pid: u32) -> Option<String> {
    // SAFETY: the handle is closed on every path out; the buffer is as
    // long as the length passed in.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut path = [0u16; MAX_PATH as usize];
        let mut length = path.len() as u32;
        let got = QueryFullProcessImageNameW(
            process,
            Default::default(),
            windows::core::PWSTR(path.as_mut_ptr()),
            &mut length,
        );
        let _ = CloseHandle(process);
        got.ok()?;
        Some(from_wide(&path[..length as usize]))
    }
}

/// What people call the app a process belongs to: the exe's
/// `FileDescription`, falling back to its file name without `.exe`.
pub(super) fn app_of_process(pid: u32) -> Option<String> {
    let path = full_path_of_process(pid)?;
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let file_name = || {
        std::path::Path::new(&path)
            .file_stem()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
    };
    // SAFETY: the version block is read into a buffer of the size Windows
    // asked for, and every pointer it hands back points inside it.
    unsafe {
        let mut unused = 0;
        let size = GetFileVersionInfoSizeW(PCWSTR(wide.as_ptr()), Some(&mut unused));
        if size == 0 {
            return file_name();
        }
        let mut block = vec![0u8; size as usize];
        if GetFileVersionInfoW(PCWSTR(wide.as_ptr()), None, size, block.as_mut_ptr().cast())
            .is_err()
        {
            return file_name();
        }
        // Which languages the file has strings for; the first will do.
        let mut translation = std::ptr::null_mut();
        let mut length = 0;
        let languages = wide_of(r"\VarFileInfo\Translation");
        if VerQueryValueW(
            block.as_ptr().cast(),
            PCWSTR(languages.as_ptr()),
            &mut translation,
            &mut length,
        )
        .as_bool()
            && length >= 4
        {
            let pair = translation.cast::<u16>();
            let key = format!(
                r"\StringFileInfo\{:04x}{:04x}\FileDescription",
                *pair,
                *pair.add(1)
            );
            let key = wide_of(&key);
            let mut text = std::ptr::null_mut();
            let mut characters = 0;
            if VerQueryValueW(
                block.as_ptr().cast(),
                PCWSTR(key.as_ptr()),
                &mut text,
                &mut characters,
            )
            .as_bool()
                && characters > 0
            {
                let description = from_wide(std::slice::from_raw_parts(
                    text.cast::<u16>(),
                    characters as usize,
                ));
                if !description.trim().is_empty() {
                    return Some(description.trim().to_owned());
                }
            }
        }
        file_name()
    }
}

/// A small picture of one monitor, copied from the desktop.
pub(super) fn thumbnail_of_screen(rect: RECT) -> Option<Thumbnail> {
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    // SAFETY: the desktop DC is released and every GDI object deleted on
    // the way out of `shrink_from`.
    unsafe {
        let desktop = GetDC(None);
        if desktop.is_invalid() {
            return None;
        }
        let thumbnail = shrink_from(desktop, rect.left, rect.top, width, height);
        ReleaseDC(None, desktop);
        thumbnail
    }
}

/// A small picture of one window, drawn by the window itself.
pub(super) fn thumbnail_of_window(window: HWND) -> Option<Thumbnail> {
    let mut rect = RECT::default();
    // SAFETY: a live handle, and a rectangle it fills in.
    unsafe { GetWindowRect(window, &mut rect) }.ok()?;
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    if width < 2 || height < 2 {
        return None;
    }
    // SAFETY: each GDI object is deleted on every path out; the bitmap is
    // selected into the memory DC only while it is drawn into.
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return None;
        }
        let full = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        ReleaseDC(None, screen);
        if full.is_invalid() || bitmap.is_invalid() {
            if !full.is_invalid() {
                let _ = DeleteDC(full);
            }
            return None;
        }
        let previous = SelectObject(full, HGDIOBJ(bitmap.0));
        // Full content, or a window that draws itself with Direct3D — most
        // of them now — comes back blank.
        let drawn = PrintWindow(window, full, PW_RENDERFULLCONTENT).as_bool();
        let thumbnail = drawn
            .then(|| shrink_from(full, 0, 0, width, height))
            .flatten();
        SelectObject(full, previous);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let _ = DeleteDC(full);
        thumbnail
    }
}

/// Copies a rectangle of `from`, shrunk to fit inside [`THUMBNAIL`], as
/// RGBA rows.
///
/// # Safety
/// `from` must be a valid device context, and the rectangle inside it.
unsafe fn shrink_from(from: HDC, x: i32, y: i32, width: i32, height: i32) -> Option<Thumbnail> {
    if width < 1 || height < 1 {
        return None;
    }
    // Keep the shape; never make it bigger than it is.
    let scale = f64::from(THUMBNAIL.0) / f64::from(width);
    let scale = scale
        .min(f64::from(THUMBNAIL.1) / f64::from(height))
        .min(1.0);
    let small = (
        ((f64::from(width) * scale) as i32).max(1),
        ((f64::from(height) * scale) as i32).max(1),
    );

    // SAFETY: the caller promises `from`; everything made here is deleted
    // before returning.
    unsafe {
        let into = CreateCompatibleDC(Some(from));
        let bitmap = CreateCompatibleBitmap(from, small.0, small.1);
        if into.is_invalid() || bitmap.is_invalid() {
            if !into.is_invalid() {
                let _ = DeleteDC(into);
            }
            return None;
        }
        let previous = SelectObject(into, HGDIOBJ(bitmap.0));
        // Halftone averages rather than dropping pixels, which a shrink by
        // eight badly needs.
        SetStretchBltMode(into, HALFTONE);
        let copied = StretchBlt(
            into,
            0,
            0,
            small.0,
            small.1,
            Some(from),
            x,
            y,
            width,
            height,
            SRCCOPY,
        )
        .as_bool();
        let rgba = copied.then(|| rgba_of(into, bitmap, small)).flatten();
        SelectObject(into, previous);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let _ = DeleteDC(into);
        rgba.map(|rgba| Thumbnail {
            width: small.0 as u32,
            height: small.1 as u32,
            rgba,
        })
    }
}

/// A bitmap's pixels as RGBA rows, top down.
///
/// # Safety
/// `bitmap` must be a bitmap of `size` selected into `dc`.
unsafe fn rgba_of(dc: HDC, bitmap: HBITMAP, size: (i32, i32)) -> Option<Vec<u8>> {
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size.0,
            // Negative: rows top down, as everything else here expects.
            biHeight: -size.1,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bgra = vec![0u8; size.0 as usize * size.1 as usize * 4];
    // SAFETY: the buffer is exactly the size the header describes.
    let rows = unsafe {
        GetDIBits(
            dc,
            bitmap,
            0,
            size.1 as u32,
            Some(bgra.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        )
    };
    if rows == 0 {
        return None;
    }
    // GDI gives BGRA with the fourth byte unset; the picker wants RGBA.
    for pixel in bgra.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
        pixel[3] = 255;
    }
    Some(bgra)
}

fn from_wide(text: &[u16]) -> String {
    let end = text
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(text.len());
    String::from_utf16_lossy(&text[..end])
}

fn wide_of(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_monitors_device_name_looks_like_a_device() {
        let found = monitors();
        assert!(!found.is_empty(), "a PC has a monitor");
        for monitor in &found {
            assert!(monitor.device.starts_with(r"\\.\"), "{}", monitor.device);
            assert!(monitor.rect.right > monitor.rect.left);
            assert!(monitor.rect.bottom > monitor.rect.top);
        }
    }

    #[test]
    fn a_window_handle_survives_being_an_id() {
        let window = HWND(0x1234 as *mut c_void);
        assert_eq!(window_of_id(&handle_id(window).to_string()), Some(window));
        assert_eq!(window_of_id("not a number"), None);
    }

    #[test]
    fn this_process_is_named_after_its_executable() {
        // SAFETY: nothing but a read of our own id.
        let us = unsafe { GetCurrentProcessId() };
        let app = app_of_process(us).expect("this process has a name");
        assert!(!app.trim().is_empty(), "{app:?}");
    }

    #[test]
    fn the_offered_windows_are_visible_titled_and_not_ours() {
        // SAFETY: a read of our own id.
        let us = unsafe { GetCurrentProcessId() };
        for window in shareable_windows() {
            assert!(!window.title.is_empty(), "every window has a title");
            // SAFETY: handles from an enumeration a moment ago.
            unsafe {
                assert!(IsWindowVisible(window.handle).as_bool());
                let mut pid = 0;
                GetWindowThreadProcessId(window.handle, Some(&mut pid));
                assert_ne!(pid, us, "never Opencord's own windows");
            }
        }
    }
}
