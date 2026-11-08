//! What the Opencord server and the `opencord-plugin` tool share: the Luau
//! sandbox plugin scripts run in, and the limits they run under.

pub mod limits;
pub mod sandbox;

/// The plugin API this build provides. A manifest's `api` may be 1 to this.
pub const API_VERSION: u32 = 1;
