//! macOS's screens: not written yet (docs/plans/macos.md M6:
//! ScreenCaptureKit, the system's `SCContentSharingPicker` on macOS 14+).
//! Until it is, the Screen button stays hidden on macOS.
//!
//! The contract is in `mod.rs`. Frames go to `frames` as packed
//! [`RawFrame`]s (NV12 from ScreenCaptureKit) with `try_send`, dropping one
//! when the queue is full; `Picked.sound` names the window's app when the
//! picker's filter says which it is.

use std::sync::mpsc::SyncSender;

use super::{Picked, ScreenError, ScreenSource, ScreenSupport, StartRequest};
use crate::video::camera::RawFrame;

/// Never made: starting always fails here.
pub(super) struct Capture {
    _never: (),
}

impl Capture {
    pub(super) fn size(&self) -> (u32, u32) {
        unreachable!("no screen is ever captured here")
    }

    pub(super) fn finished(&self) -> bool {
        unreachable!("no screen is ever captured here")
    }
}

pub(super) fn support() -> ScreenSupport {
    ScreenSupport {
        supported: false,
        reason: Some("Screen sharing is not available on this system yet".to_owned()),
        own_picker: false,
    }
}

pub(super) async fn sources() -> Result<Vec<ScreenSource>, ScreenError> {
    Ok(Vec::new())
}

pub(super) async fn start(
    _request: StartRequest,
    _frames: SyncSender<RawFrame>,
) -> Result<(Capture, Picked), ScreenError> {
    Err(ScreenError::NotSupported)
}
