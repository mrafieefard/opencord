//! Opencord's shared update types, manifest verification, and layout.

pub mod archive;
pub mod check;
pub mod layout;
pub mod release;
pub mod signature;
pub mod track;

pub use archive::{ArchiveError, unpack};
pub use layout::Layout;
pub use release::{Asset, Manifest, ManifestError};
pub use signature::KeyError;
pub use track::Track;

/// The repository releases come from.
pub const REPOSITORY: &str = match option_env!("OPENCORD_UPDATE_REPOSITORY") {
    Some(repo) => repo,
    None => "mrafieefard/opencord",
};
