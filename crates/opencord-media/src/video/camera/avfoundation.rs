//! macOS's cameras: not written yet (docs/plans/macos.md M5: AVFoundation). Until they are, the
//! camera buttons stay hidden on macOS.
//!
//! The contract, as `linux.rs` keeps it: `SUPPORTED` says whether this
//! system has cameras at all; `cameras()` lists them with ids that stay the
//! same while a camera stays plugged in, and returns `CameraError::Denied`
//! when the user refused access (with where to allow it in the message);
//! `Capture::start` opens the camera with that id, or the first one, at up
//! to 1280×720 and 30 fps, and sends packed frames ([`super::pack`]) to
//! `frames` with `try_send`, dropping a frame when the queue is full. The
//! queue closes when the camera goes away; dropping the `Capture` stops it.
//! AVFoundation delivers NV12 at video range.

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
