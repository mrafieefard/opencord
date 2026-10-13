//! Screen capture (plan §9.3). Each system has its backend in a file of its
//! own (docs/plans/platforms.md), all with one contract:
//!
//! - `support()`: whether screens can be shared here, why not, and whether
//!   Opencord shows its own picker (`own_picker`) or the system shows one
//!   after the share dialog.
//! - `sources()`: the screens and windows Opencord's own picker offers;
//!   empty where the system picks.
//! - `start(request, frames)`: picks (the system's picker, or the source
//!   `request.source_id` names) and starts capturing it into `frames`,
//!   at most `request.fps` frames a second; returns once frames flow, with
//!   what was picked. The capture stops when dropped; when its source goes
//!   away (the window closed, the monitor unplugged) the queue closes and
//!   `finished()` turns true.
//!
//! On Linux the ScreenCast portal's system picker chooses a screen or a
//! window and hands over a PipeWire node; without the portal (checks that
//! run with nobody to pick) a node is named in PipeWire's own session.

use std::sync::mpsc::SyncSender;

use crate::share_audio::Source;
use crate::video::camera::RawFrame;

// One block per system: each session edits only its own.
#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod backend;
#[cfg(target_os = "linux")]
pub mod capture;
#[cfg(target_os = "linux")]
pub mod portal;

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod backend;

#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod backend;

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
#[path = "unsupported.rs"]
mod backend;

/// What was shared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Screen,
    Window,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScreenError {
    #[error("screen sharing is not supported on this system yet")]
    NotSupported,
    /// The user closed the system picker.
    #[error("no screen or window was chosen")]
    Cancelled,
    #[error("screen sharing was not allowed")]
    Denied,
    #[error("screen capture failed: {0}")]
    Failed(String),
}

/// Whether this system can share screens, and how the source is picked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenSupport {
    pub supported: bool,
    /// Why not, when not.
    pub reason: Option<String>,
    /// Opencord lists the sources itself ([`sources`]); otherwise the
    /// system's picker opens after the share dialog.
    pub own_picker: bool,
}

/// A screen or window Opencord's own picker offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenSource {
    /// What [`StartRequest::source_id`] names it by.
    pub id: String,
    pub kind: SourceKind,
    /// "Screen 1 · 2560×1440", or the window's title.
    pub title: String,
    /// The app a window belongs to.
    pub app: Option<String>,
    pub thumbnail: Option<Thumbnail>,
}

/// A small picture of a source, as RGBA rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumbnail {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// What to share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartRequest {
    pub fps: u32,
    /// The system picker's earlier choice, to share it again without
    /// asking (Linux's portal).
    pub restore_token: Option<String>,
    /// The source chosen in Opencord's own picker.
    pub source_id: Option<String>,
}

/// The app whose sound a window share sends, when the system says which
/// app the window belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowSound {
    pub source: Source,
    /// As the app shows it.
    pub app: String,
}

/// What was picked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picked {
    pub kind: SourceKind,
    /// For sharing the same source again without the picker.
    pub restore_token: Option<String>,
    /// A window's app, when known; otherwise the app asks ("Sound from").
    pub sound: Option<WindowSound>,
}

/// A screen or window being captured; stops when dropped.
pub struct Capture(backend::Capture);

impl Capture {
    /// The source's size when capture started.
    pub fn size(&self) -> (u32, u32) {
        self.0.size()
    }

    /// Whether capture has ended: the source went away.
    pub fn finished(&self) -> bool {
        self.0.finished()
    }
}

pub fn support() -> ScreenSupport {
    backend::support()
}

/// The sources Opencord's own picker offers; empty where the system picks.
pub async fn sources() -> Result<Vec<ScreenSource>, ScreenError> {
    backend::sources().await
}

/// Picks a source and starts capturing it.
pub async fn start(
    request: StartRequest,
    frames: SyncSender<RawFrame>,
) -> Result<(Capture, Picked), ScreenError> {
    let (capture, picked) = backend::start(request, frames).await?;
    Ok((Capture(capture), picked))
}
