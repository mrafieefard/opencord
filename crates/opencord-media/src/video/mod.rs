//! Video (Phase 2 plan §7.8-7.11, §8): cameras, pictures, H.264 encoding
//! and decoding, what is sent when, and a synthetic test pattern.

pub mod camera;
pub mod codec;
pub mod layers;
pub mod pattern;
pub mod picture;
pub mod priorities;
#[cfg(target_os = "linux")]
pub mod receiver;
#[cfg(target_os = "linux")]
pub mod sender;
