//! The client media engine (Phase 2 plan §7): the voice transport, which
//! connects to a voice node and moves Opus packets and H.264 frames, the
//! audio engine around it, video's sending policy, and global hotkeys.

pub mod audio;
pub mod hotkeys;
pub mod transport;
pub mod video;
