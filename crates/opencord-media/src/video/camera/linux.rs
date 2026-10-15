//! Linux's cameras: the Camera portal and PipeWire, with V4L2 where the
//! portal or its cameras are missing.

use std::sync::mpsc::SyncSender;

use super::mode::Mode;
use super::{CameraError, CameraInfo, RawFrame, pipewire, v4l2};

pub(super) const SUPPORTED: bool = true;

/// The cameras there are: through the Camera portal (which may ask the
/// user for access), or V4L2 where the portal or its cameras are missing.
/// A refusal is final.
pub(super) async fn cameras() -> Result<Vec<CameraInfo>, CameraError> {
    match pipewire_remote().await {
        Ok(remote) => {
            let listed = tokio::task::spawn_blocking(move || pipewire::cameras(remote))
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

/// Where PipeWire's cameras are: the Camera portal, which may ask the user
/// for access; or, with `OPENCORD_CAMERA_PORTAL=0`, the PipeWire session
/// itself, as an unsandboxed app may reach it — for checks that run with
/// nobody to answer the portal.
async fn pipewire_remote() -> Result<pipewire::Remote, CameraError> {
    if std::env::var_os("OPENCORD_CAMERA_PORTAL").is_some_and(|value| value == "0") {
        return Ok(pipewire::Remote::Session);
    }
    pipewire::portal_remote()
        .await
        .map(pipewire::Remote::Portal)
}

async fn v4l2_cameras() -> Vec<CameraInfo> {
    tokio::task::spawn_blocking(v4l2::cameras)
        .await
        .unwrap_or_default()
}

pub(super) enum Capture {
    PipeWire(pipewire::Capture),
    V4l2(v4l2::Capture),
}

impl Capture {
    pub(super) async fn start(
        camera: Option<String>,
        frames: SyncSender<RawFrame>,
    ) -> Result<Self, CameraError> {
        let v4l2 = camera.as_deref().is_some_and(|id| id.starts_with("v4l2:"));
        let remote = if v4l2 {
            None
        } else {
            match pipewire_remote().await {
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
            Some(remote) => {
                pipewire::Capture::start(remote, camera.as_deref(), frames).map(Self::PipeWire)
            }
            None => v4l2::Capture::start(camera.as_deref(), frames).map(Self::V4l2),
        })
        .await
        .map_err(|error| CameraError::Failed(error.to_string()))?
    }

    pub(super) fn mode(&self) -> Mode {
        match self {
            Self::PipeWire(capture) => capture.mode,
            Self::V4l2(capture) => capture.mode,
        }
    }
}
