//! Cameras (plan §8): quality is automatic, up to 1280×720 at 30 fps.
//! Linux captures through the Camera portal and PipeWire; other systems
//! get their backends later.

pub mod mode;
#[cfg(target_os = "linux")]
pub mod pipewire;
#[cfg(target_os = "linux")]
pub mod v4l2;

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
}

/// A frame as a camera delivered it, rows packed.
#[derive(Clone)]
pub struct RawFrame {
    pub format: RawFormat,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    pub captured: Instant,
}

/// The cameras there are: through the Camera portal (which may ask the
/// user for access), or V4L2 where the portal or its cameras are missing.
/// A refusal is final.
#[cfg(target_os = "linux")]
pub async fn cameras() -> Result<Vec<CameraInfo>, CameraError> {
    match pipewire::portal_remote().await {
        Ok(remote) => {
            let listed = tokio::task::spawn_blocking(move || {
                pipewire::cameras(pipewire::Remote::Portal(remote))
            })
            .await
            .map_err(|error| CameraError::Failed(error.to_string()))??;
            if listed.is_empty() {
                return Ok(v4l2_cameras().await);
            }
            Ok(listed)
        }
        Err(CameraError::Denied) => Err(CameraError::Denied),
        Err(error) => {
            tracing::debug!(%error, "no Camera portal; trying V4L2");
            Ok(v4l2_cameras().await)
        }
    }
}

#[cfg(target_os = "linux")]
async fn v4l2_cameras() -> Vec<CameraInfo> {
    tokio::task::spawn_blocking(v4l2::cameras)
        .await
        .unwrap_or_default()
}

#[cfg(not(target_os = "linux"))]
pub async fn cameras() -> Result<Vec<CameraInfo>, CameraError> {
    Err(CameraError::NotSupported)
}

/// A camera capturing; stops when dropped. Its frames go to the queue it
/// was started with, which closes if the camera goes away.
pub enum Capture {
    #[cfg(target_os = "linux")]
    PipeWire(pipewire::Capture),
    #[cfg(target_os = "linux")]
    V4l2(v4l2::Capture),
}

impl Capture {
    /// Starts the camera with this id from [`cameras`], or the first one.
    #[cfg(target_os = "linux")]
    pub async fn start(
        camera: Option<String>,
        frames: std::sync::mpsc::SyncSender<RawFrame>,
    ) -> Result<Self, CameraError> {
        let v4l2 = camera.as_deref().is_some_and(|id| id.starts_with("v4l2:"));
        let remote = if v4l2 {
            None
        } else {
            match pipewire::portal_remote().await {
                Ok(remote) => Some(remote),
                Err(CameraError::Denied) => return Err(CameraError::Denied),
                Err(error) if camera.is_some() => return Err(error),
                Err(error) => {
                    tracing::debug!(%error, "no Camera portal; trying V4L2");
                    None
                }
            }
        };
        tokio::task::spawn_blocking(move || match remote {
            Some(remote) => pipewire::Capture::start(
                pipewire::Remote::Portal(remote),
                camera.as_deref(),
                frames,
            )
            .map(Self::PipeWire),
            None => v4l2::Capture::start(camera.as_deref(), frames).map(Self::V4l2),
        })
        .await
        .map_err(|error| CameraError::Failed(error.to_string()))?
    }

    #[cfg(not(target_os = "linux"))]
    pub async fn start(
        _camera: Option<String>,
        _frames: std::sync::mpsc::SyncSender<RawFrame>,
    ) -> Result<Self, CameraError> {
        Err(CameraError::NotSupported)
    }

    /// What the camera delivers.
    pub fn mode(&self) -> Mode {
        match self {
            #[cfg(target_os = "linux")]
            Self::PipeWire(capture) => capture.mode,
            #[cfg(target_os = "linux")]
            Self::V4l2(capture) => capture.mode,
        }
    }
}

/// A capture buffer's bytes as a frame with packed rows; `None` if they do
/// not fit the mode. `stride` is the luma row's length in the buffer (0
/// when packed).
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
        RawFormat::Yuyv => rows(data, 0, luma_row, stride, height, &mut packed)?,
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
