//! The client media engine (Phase 2 plan §7): the voice transport, which
//! connects to a voice node and moves Opus packets, the audio engine around
//! it, and global hotkeys.

pub mod audio;
pub mod hotkeys;
pub mod transport;
