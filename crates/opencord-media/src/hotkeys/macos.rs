//! macOS's global hotkeys: not written yet (docs/plans/macos.md M2: a
//! listen-only event tap). Until they are, hotkeys work only while Opencord
//! is focused.
//!
//! The contract, as `linux.rs` keeps it: `bind` binds `bindings`
//! system-wide, replacing nothing (the caller drops the old `Keys` first), and
//! sends each press and release to `events` through [`super::Held`], so an
//! action with several keys presses when the first goes down and releases
//! when the last comes up. The keys stay bound while the returned `Keys` is
//! kept and unbind when it is dropped. Accelerators name keys as xkb keysyms
//! (`CTRL+SHIFT+m`, `Control_R`, `F13`, `KP_5`). When Input Monitoring is
//! missing, say so with `settings_url` set to its pane; the app shows an
//! "Open settings" button and binds again when its window is focused.

use tokio::sync::mpsc;

use super::{HotkeyBinding, HotkeyEvent, HotkeySupport};

/// The keys bound now; they stay bound while this is kept.
pub(super) struct Keys;

pub(super) async fn bind(
    _app_id: &str,
    _bindings: &[HotkeyBinding],
    _events: &mpsc::UnboundedSender<HotkeyEvent>,
) -> (Option<Keys>, HotkeySupport) {
    (
        None,
        HotkeySupport::FocusedOnly {
            reason: "global hotkeys are not available on this system yet".to_owned(),
            settings_url: None,
        },
    )
}
