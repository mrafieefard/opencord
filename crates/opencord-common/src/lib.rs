//! Building blocks shared by the Opencord server and client core.

pub mod address;
pub mod auth;
pub mod channel;
pub mod limits;
pub mod permissions;
pub mod signed;
pub mod snowflake;
pub mod validation;
pub mod voice;

/// Version of the gateway protocol spoken by this build.
pub const PROTOCOL_VERSION: u32 = 1;
