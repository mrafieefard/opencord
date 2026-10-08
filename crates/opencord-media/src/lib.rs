//! The client media engine (Phase 2 plan §7): the voice transport, which
//! connects to a voice node and moves Opus packets, and the audio engine
//! around it.

pub mod audio;
pub mod transport;
