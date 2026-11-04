//! Building blocks shared by the Opencord server and client core.

pub mod address;
pub mod auth;
pub mod avatar;
pub mod blob;
pub mod channel;
pub mod limits;
pub mod mp4;
pub mod permissions;
pub mod private;
pub mod rate;
pub mod signed;
pub mod sniff;
pub mod snowflake;
pub mod sound;
pub mod upload;
pub mod validation;
pub mod video;
pub mod voice;
pub mod waveform;
pub mod words;

/// Version of the gateway protocol spoken by this build.
pub const PROTOCOL_VERSION: u32 = 1;
