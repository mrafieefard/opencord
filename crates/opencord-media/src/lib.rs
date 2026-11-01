//! The client media engine (Phase 2 plan §7): the voice transport, which
//! connects to a voice node and moves Opus packets and H.264 frames, the
//! audio engine around it, video's sending policy, and global hotkeys.

/// Ends a test early on a shared CI runner, where the workflow sets
/// `OPENCORD_SHARED_RUNNER`: a test that times things to a few milliseconds
/// or measures a hardware encoder, which a virtual machine sharing its CPU
/// (and, on macOS, with only a paravirtual encoder) cannot vouch for.
#[cfg(test)]
macro_rules! needs_own_machine {
    () => {
        if std::env::var_os("OPENCORD_SHARED_RUNNER").is_some() {
            eprintln!("skipped: needs a machine of its own, not a shared runner");
            return;
        }
    };
}
#[cfg(test)]
pub(crate) use needs_own_machine;

pub mod attenuation;
pub mod audio;
pub mod hotkeys;
pub mod idle;
pub mod player;
pub mod share_audio;
pub mod soundboard;
pub mod system;
pub mod transport;
pub mod video;
