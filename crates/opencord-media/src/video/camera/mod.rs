//! Cameras (plan §8): quality is automatic, up to 1280×720 at 30 fps.
//! Each system has its backend in a file of its own (docs/plans/platforms.md),
//! all with one contract: `SUPPORTED`, `cameras()`, and a `Capture` that
//! starts a camera by id (or the first one) into a frame queue, reports its
//! `mode()`, closes the queue when the camera goes away and stops when
//! dropped. Linux captures through the Camera portal and PipeWire, with
//! V4L2 as the fallback.

pub mod mode;

// One block per system: each session edits only its own.
#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod backend;
#[cfg(target_os = "linux")]
pub mod pipewire;
#[cfg(target_os = "linux")]
pub mod v4l2;
#[cfg(all(target_os = "linux", any(test, feature = "testing")))]
pub mod virtual_camera;

#[cfg(target_os = "macos")]
#[path = "avfoundation.rs"]
mod backend;

#[cfg(target_os = "windows")]
#[path = "media_foundation.rs"]
mod backend;

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
#[path = "unsupported.rs"]
mod backend;

use std::time::Instant;

use mode::Mode;

/// A camera, as the app lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraInfo {
    /// Stable across restarts while the camera stays plugged in.
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CameraError {
    #[error("cameras are not supported on this system yet")]
    NotSupported,
    #[error("no camera is connected")]
    NoCamera,
    #[error("camera access was denied")]
    Denied,
    #[error("the camera offers nothing Opencord can use")]
    NoUsableMode,
    #[error("the camera failed: {0}")]
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawFormat {
    Mjpeg,
    /// Packed 4:2:2: Y U Y V.
    Yuyv,
    Nv12,
    I420,
    /// Four bytes a pixel, the last unused: what screen capture delivers.
    Bgrx,
    Rgbx,
}

/// A frame as a camera (or a screen) delivered it, rows packed.
#[derive(Clone)]
pub struct RawFrame {
    pub format: RawFormat,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    pub captured: Instant,
}

/// Whether this system has a camera backend.
pub fn supported() -> bool {
    backend::SUPPORTED
}

/// The cameras there are. On Linux this may ask the user for access first
/// (the Camera portal); a refusal is final.
pub async fn cameras() -> Result<Vec<CameraInfo>, CameraError> {
    backend::cameras().await
}

/// A camera capturing; stops when dropped. Its frames go to the queue it
/// was started with, which closes if the camera goes away.
pub struct Capture(backend::Capture);

impl Capture {
    /// Starts the camera with this id from [`cameras`], or the first one.
    pub async fn start(
        camera: Option<String>,
        frames: std::sync::mpsc::SyncSender<RawFrame>,
    ) -> Result<Self, CameraError> {
        backend::Capture::start(camera, frames).await.map(Self)
    }

    /// What the camera delivers.
    pub fn mode(&self) -> Mode {
        self.0.mode()
    }
}

/// A capture buffer's bytes as a frame with packed rows; `None` if they do
/// not fit the mode. `stride` is the luma row's length in the buffer (0
/// when packed).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn pack(mode: Mode, data: &[u8], stride: i32, captured: Instant) -> Option<RawFrame> {
    let (width, height) = (mode.width as usize, mode.height as usize);
    let frame = |data: Vec<u8>| RawFrame {
        format: mode.format,
        width: mode.width,
        height: mode.height,
        data,
        captured,
    };
    if mode.format == RawFormat::Mjpeg {
        return (!data.is_empty()).then(|| frame(data.to_vec()));
    }
    let luma_row = match mode.format {
        RawFormat::Yuyv => width * 2,
        RawFormat::Bgrx | RawFormat::Rgbx => width * 4,
        _ => width,
    };
    let stride = usize::try_from(stride)
        .ok()
        .filter(|&stride| stride >= luma_row)
        .unwrap_or(luma_row);
    let rows =
        |data: &[u8], start: usize, row: usize, stride: usize, count: usize, out: &mut Vec<u8>| {
            for line in 0..count {
                let from = start + line * stride;
                out.extend_from_slice(data.get(from..from + row)?);
            }
            Some(())
        };
    let mut packed = Vec::with_capacity(luma_row * height * 2);
    match mode.format {
        RawFormat::Yuyv | RawFormat::Bgrx | RawFormat::Rgbx => {
            rows(data, 0, luma_row, stride, height, &mut packed)?;
        }
        RawFormat::Nv12 => {
            rows(data, 0, width, stride, height, &mut packed)?;
            rows(
                data,
                stride * height,
                width,
                stride,
                height / 2,
                &mut packed,
            )?;
        }
        RawFormat::I420 => {
            rows(data, 0, width, stride, height, &mut packed)?;
            let chroma = stride / 2;
            let u = stride * height;
            let v = u + chroma * (height / 2);
            rows(data, u, width / 2, chroma, height / 2, &mut packed)?;
            rows(data, v, width / 2, chroma, height / 2, &mut packed)?;
        }
        RawFormat::Mjpeg => {}
    }
    Some(frame(packed))
}
