//! Systems without a screen capture backend.

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
        reason: Some("Screen sharing is not available on this system".to_owned()),
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
