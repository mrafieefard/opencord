//! Opencord's shared update types, manifest verification, and layout.

pub mod archive;
pub mod check;
pub mod layout;
pub mod markers;
pub mod pending;
pub mod release;
pub mod restart;
pub mod signature;
pub mod track;

pub use archive::{ArchiveError, unpack};
pub use layout::Layout;
pub use markers::Mark;
pub use pending::{DownloadNote, FRESH_FOR, Pending, switchable};
pub use release::{Asset, Manifest, ManifestError};
pub use restart::{RESTART_FOR, RestartNote, VoiceNote, WindowNote};
pub use signature::KeyError;
pub use track::Track;

/// The repository releases come from.
pub const REPOSITORY: &str = match option_env!("OPENCORD_UPDATE_REPOSITORY") {
    Some(repo) => repo,
    None => "mrafieefard/opencord",
};
