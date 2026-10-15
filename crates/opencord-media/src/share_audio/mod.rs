//! Screen share audio (Phase 2 plan §10): capture the audio of what is being
//! shared and send it alongside the video. Screens share everything but
//! Opencord's own output, windows share only that app's audio.
//!
//! Each system has its capture backend in a file of its own
//! (docs/plans/platforms.md), all with one contract: `support()` says
//! whether sound can be shared here and, when not, why;
//! `Capture::start(source, frames)` captures `source` as 48 kHz
//! interleaved stereo and sends it to `frames` as [`Captured`] blocks on
//! one clock, until dropped; `Capture::share` changes the source while
//! live; `apps_with_audio()` lists the apps playing now, Opencord left out,
//! for a window share's "Sound from". [`sender`] encodes what was captured,
//! the same everywhere. On Linux, PipeWire links playback streams into a
//! capture stream.

pub mod sender;

// One block per system: each session edits only its own.
#[cfg(target_os = "linux")]
pub mod pipewire;
#[cfg(target_os = "linux")]
use pipewire as backend;

#[cfg(target_os = "macos")]
mod coreaudio;
#[cfg(target_os = "macos")]
use coreaudio as backend;

#[cfg(target_os = "windows")]
mod wasapi;
#[cfg(target_os = "windows")]
use wasapi as backend;

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod unsupported;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
use unsupported as backend;

use std::time::Instant;

pub use backend::{Capture, apps_with_audio, support};

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

/// Whether this system can share a screen's sound, and why not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareAudioSupport {
    pub supported: bool,
    /// Shown beside the disabled switch.
    pub reason: Option<String>,
}

/// What is shared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// Every app's sound but Opencord's: an entire screen.
    Everything,
    /// One app's: every playback stream it names itself by
    /// (`application.name`), from any of its processes.
    App(String),
    /// Every playback stream of a process and its children: a window
    /// whose process is known (X11's `_NET_WM_PID`).
    Process(u32),
    /// Nothing yet: a window share before its app is chosen.
    Nothing,
}

/// An app with a playback stream open, which a window share can share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppInfo {
    /// As it names itself (`application.name`).
    pub name: String,
    /// Its icon's name in the desktop's icon theme, when it gives one.
    pub icon: Option<String>,
    pub pid: Option<u32>,
}

/// What was captured.
#[derive(Debug, Clone)]
pub struct Captured {
    /// Interleaved stereo at 48 kHz.
    pub samples: Vec<f32>,
    /// The first frame's position on this capture's 48 kHz clock.
    pub position: u64,
    /// When the first frame was captured.
    pub captured: Instant,
}
