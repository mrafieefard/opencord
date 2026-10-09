//! Screen capture (plan §9.3). On Linux the ScreenCast portal's system
//! picker chooses a screen or a window and hands over a PipeWire node,
//! which [`capture`] reads; without the portal (checks that run with
//! nobody to pick) a node is named in PipeWire's own session instead.

pub mod capture;
pub mod portal;

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
