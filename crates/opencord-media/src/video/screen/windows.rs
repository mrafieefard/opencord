//! Windows' screens: not written yet (docs/plans/windows.md W6:
//! Windows.Graphics.Capture and Opencord's own picker, from `sources()`).
//! Until it is, the Screen button stays hidden on Windows.
//!
//! The contract is in `mod.rs`. `start` shares `request.source_id`, one of
//! `sources()`'s ids; frames go to `frames` as packed [`RawFrame`]s (BGRx)
//! with `try_send`, dropping one when the queue is full; a window share's
//! `Picked.sound` is `Source::Process` of the window's process.

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
        own_picker: true,
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
