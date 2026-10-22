//! How long this device's keyboard and mouse have been idle, for AFK moves
//! (Phase 2 plan §12). Each system answers in its own way; one that cannot
//! answers `None`, and its user is never moved for being away.

use std::time::Duration;

// One block per system (docs/plans/platforms.md): each session edits only
// its own.
#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod backend;

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod backend;

#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod backend;

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
#[path = "unsupported.rs"]
mod backend;

/// Idle time this short or shorter may be reported as zero: some systems
/// only say when this much has passed.
pub const THRESHOLD: Duration = Duration::from_secs(60);

/// How long the keyboard and mouse have been idle; `None` when this system
/// cannot tell. Under [`THRESHOLD`] it may read zero.
pub async fn idle_time() -> Option<Duration> {
    backend::idle_time().await
}
