//! Linux's global hotkeys: the GlobalShortcuts portal on Wayland, XInput 2
//! on X11.

use tokio::sync::mpsc;

use super::{HotkeyBinding, HotkeyEvent, HotkeySupport, portal, x11};

/// The keys bound now; they stay bound while this is kept.
pub(super) enum Keys {
    Portal(#[expect(dead_code, reason = "held for its session")] portal::Portal),
    X11(#[expect(dead_code, reason = "held for its thread")] x11::X11Keys),
}

/// Binds `bindings` through whatever this session offers.
pub(super) async fn bind(
    app_id: &str,
    bindings: &[HotkeyBinding],
    events: &mpsc::UnboundedSender<HotkeyEvent>,
) -> (Option<Keys>, HotkeySupport) {
    let focused_only = |reason: String| HotkeySupport::FocusedOnly {
        reason,
        settings_url: None,
    };
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        // XWayland sees no keys meant for Wayland windows, so only the
        // portal can help here.
        return match portal::Portal::bind(app_id, bindings, events.clone()).await {
            Ok(portal) => (
                Some(Keys::Portal(portal)),
                HotkeySupport::Global {
                    method: "the desktop's global shortcuts".to_owned(),
                },
            ),
            Err(error) => (
                None,
                focused_only(format!("this desktop offers no global shortcuts ({error})")),
            ),
        };
    }
    if let Some(display) = std::env::var_os("DISPLAY") {
        return match x11::X11Keys::start(&display.to_string_lossy(), bindings, events.clone()) {
            Ok(keys) => (
                Some(Keys::X11(keys)),
                HotkeySupport::Global {
                    method: "X11 input".to_owned(),
                },
            ),
            Err(error) => (
                None,
                focused_only(format!("the X server's keyboard cannot be read ({error})")),
            ),
        };
    }
    (
        None,
        focused_only("no graphical session was found".to_owned()),
    )
}
