//! Screen share audio (Phase 2 plan §10): capture the audio of what is being
//! shared and send it alongside the video. On Linux, PipeWire links playback
//! streams into a capture stream; screens share everything but Opencord's own
//! output, windows share only that app's audio.

pub mod pipewire;
pub mod sender;

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("screen audio capture is not supported on this system yet")]
    NotSupported,
    #[error("no screen or window was chosen")]
    Cancelled,
    #[error("screen audio capture was not allowed")]
    Denied,
    #[error("screen audio capture failed: {0}")]
    Failed(String),
}
