//! What the Opencord server and the `opencord-plugin` tool share: the Luau
//! sandbox plugin scripts run in, the limits they run under, the rules
//! a plugin's package and manifest must follow, and how packages are signed.
//! The app's core shares the blocks a client script draws with and the
//! checks its trees pass.

pub mod blocks;
pub mod limits;
pub mod manifest;
pub mod package;
pub mod permissions;
pub mod sandbox;
pub mod signing;
pub mod view;

/// The plugin API this build provides. A manifest's `api` may be 1 to this.
pub const API_VERSION: u32 = 1;
