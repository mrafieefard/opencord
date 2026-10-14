//! Lowering other apps' volume while people talk in a call (Phase 2 plan
//! §12, Discord's "attenuation"). Each system does it in its own way:
//! Linux lowers other apps' PipeWire playback streams, Windows their audio
//! sessions. macOS has no public way, so the setting is hidden there.
//! Whatever is lowered is put back as it was when [`Attenuator::set`] gets
//! `None`, or when the attenuator is dropped.

// One block per system (docs/plans/platforms.md): each session edits only
// its own.
#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod backend;

#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod backend;

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
#[path = "unsupported.rs"]
mod backend;

/// Whether this system can lower other apps.
pub fn supported() -> bool {
    backend::SUPPORTED
}

/// Lowers every other app's sound while asked to.
pub struct Attenuator(backend::Attenuator);

impl Attenuator {
    /// `None` where this system cannot, or its sound server is missing.
    pub fn start() -> Option<Self> {
        backend::Attenuator::start().map(Self)
    }

    /// Other apps play at `level` (0–1) of their own volume from now on,
    /// including ones that start meanwhile; `None` puts them back.
    pub fn set(&self, level: Option<f32>) {
        self.0.set(level.map(|level| level.clamp(0.0, 1.0)));
    }
}
