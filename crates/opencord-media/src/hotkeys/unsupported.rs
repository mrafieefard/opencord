//! Systems without a hotkey backend: hotkeys work only while Opencord is
//! focused.

use tokio::sync::mpsc;

use super::{HotkeyBinding, HotkeyEvent, HotkeySupport};

pub(super) struct Keys;

pub(super) async fn bind(
    _app_id: &str,
    _bindings: &[HotkeyBinding],
    _events: &mpsc::UnboundedSender<HotkeyEvent>,
) -> (Option<Keys>, HotkeySupport) {
    (
        None,
        HotkeySupport::FocusedOnly {
            reason: "global hotkeys are not available on this system".to_owned(),
            settings_url: None,
        },
    )
}
