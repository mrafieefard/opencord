//! Systems without a screen sound backend.

use std::sync::mpsc::SyncSender;

use super::{AppInfo, CaptureError, Captured, ShareAudioSupport, Source};

pub fn support() -> ShareAudioSupport {
    ShareAudioSupport {
        supported: false,
        reason: Some("Sharing sound is not available on this system".to_owned()),
    }
}

pub fn apps_with_audio() -> Result<Vec<AppInfo>, CaptureError> {
    Ok(Vec::new())
}

/// Never made: starting always fails here.
pub struct Capture {
    _never: (),
}

impl Capture {
    pub fn start(_source: Source, _frames: SyncSender<Captured>) -> Result<Self, CaptureError> {
        Err(CaptureError::NotSupported)
    }

    pub fn share(&self, _source: Source) {
        unreachable!("no sound is ever captured here")
    }
}
