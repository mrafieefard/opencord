//! Systems without a camera backend.

use std::sync::mpsc::SyncSender;

use super::mode::Mode;
use super::{CameraError, CameraInfo, RawFrame};

pub(super) const SUPPORTED: bool = false;

pub(super) async fn cameras() -> Result<Vec<CameraInfo>, CameraError> {
    Err(CameraError::NotSupported)
}

/// Never made: starting always fails here.
pub(super) struct Capture {
    _never: (),
}

impl Capture {
    pub(super) async fn start(
        _camera: Option<String>,
        _frames: SyncSender<RawFrame>,
    ) -> Result<Self, CameraError> {
        Err(CameraError::NotSupported)
    }

    pub(super) fn mode(&self) -> Mode {
        unreachable!("no camera is ever started here")
    }
}
