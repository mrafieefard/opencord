//! Plays a video or audio file from any `Read + Seek` source (media uploads
//! plan, Task 33): FFmpeg on Linux and Windows, nothing elsewhere yet.

#[cfg(any(target_os = "linux", target_os = "windows"))]
mod decode;
#[cfg(any(target_os = "linux", target_os = "windows"))]
mod ffmpeg;
#[cfg(any(target_os = "linux", target_os = "windows"))]
mod sync;
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub use self::ffmpeg::*;

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
mod unsupported;
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub use self::unsupported::*;

pub trait MediaSource: std::io::Read + std::io::Seek + Send + 'static {}
impl<T: std::io::Read + std::io::Seek + Send + 'static> MediaSource for T {}

pub trait VideoOut: Send + Sync + 'static {
    fn present(&self, width: u32, height: u32, rgba: Vec<u8>);
}

pub trait AudioOut: Send + 'static {
    fn write(&mut self, stereo_48k: &[f32]) -> usize;
    fn queued_frames(&self) -> usize;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerEvent {
    Opened {
        duration_ms: u32,
        width: u32,
        height: u32,
        has_video: bool,
        has_audio: bool,
    },
    Position {
        position_ms: u32,
        buffering: bool,
    },
    Ended,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    pub width: u32,
    pub height: u32,
    pub duration_ms: u32,
    pub has_video: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum PlayerError {
    #[error("media player is not supported on this platform")]
    Unsupported,
    #[error("failed to open media: {0}")]
    Open(String),
    #[error("failed to decode media: {0}")]
    Decode(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("playback failed: {0}")]
    Playback(String),
}
