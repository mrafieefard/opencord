//! macOS's screen sound: not written yet (docs/plans/macos.md M7: Core Audio process taps, macOS 14.2+). Until it is, the
//! share dialog's sound switch is disabled with the reason below.
//!
//! The contract, as `pipewire.rs` keeps it: `support()` answers quickly
//! enough to call when the share dialog opens; `Capture::start` captures
//! `source` (an entire screen: everything but Opencord's own process and its
//! children; a window: that app's processes) as 48 kHz interleaved stereo
//! `f32` and sends [`Captured`] blocks to `frames`, `position` counting
//! frames on the capture's own clock and `captured` when its first frame
//! was; `share` switches the source while live; dropping it stops.

use std::sync::mpsc::SyncSender;

use super::{AppInfo, CaptureError, Captured, ShareAudioSupport, Source};

pub fn support() -> ShareAudioSupport {
    ShareAudioSupport {
        supported: false,
        reason: Some("Sharing sound is not available on this system yet".to_owned()),
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
