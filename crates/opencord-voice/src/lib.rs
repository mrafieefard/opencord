//! The voice node: the voice gateway and the SFU that forwards media
//! (Phase 2 plan §3, §6). It runs inside `opencord-server` by default, or on
//! its own as `opencord-voice-node`.

pub mod bridge;
pub mod control;
pub mod node;
pub mod sfu;
pub mod token;
mod udp;

pub use udp::bind_v6_only;
