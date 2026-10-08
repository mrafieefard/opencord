//! Hotkeys on Wayland desktops, through the xdg-desktop-portal
//! GlobalShortcuts interface. Opencord says which keys it would like; the
//! desktop decides, and may ask the user (KDE and GNOME show a dialog;
//! Hyprland binds them in its own configuration).

use std::sync::Arc;

use ashpd::AppID;
use ashpd::desktop::Session;
use ashpd::desktop::global_shortcuts::{BindShortcutsOptions, GlobalShortcuts, NewShortcut};
use futures_util::StreamExt;
use tokio::sync::{OnceCell, mpsc};
use tokio::task::JoinHandle;

use super::{HotkeyAction, HotkeyBinding, HotkeyEvent};

/// Whether this process told the portal who it is; once, before any other
/// portal call (an app outside a sandbox has no app id otherwise).
static REGISTERED: OnceCell<Result<(), String>> = OnceCell::const_new();

async fn register(app_id: &str) -> Result<(), String> {
    REGISTERED
        .get_or_init(|| async {
            let app_id: AppID = app_id.parse().map_err(|error| format!("{error}"))?;
            ashpd::register_host_app(app_id)
                .await
                .map_err(|error| error.to_string())
        })
        .await
        .clone()
}

/// A global shortcuts session, open while kept.
pub(super) struct Portal {
    session: Option<Arc<Session<GlobalShortcuts>>>,
    listener: Option<JoinHandle<()>>,
}

impl Portal {
    /// Binds `bindings` in a new session and passes on their presses and
    /// releases. Fails, saying why, when the desktop has no global
    /// shortcuts portal or refuses.
    pub(super) async fn bind(
        app_id: &str,
        bindings: &[HotkeyBinding],
        events: mpsc::UnboundedSender<HotkeyEvent>,
    ) -> Result<Self, String> {
        register(app_id).await?;
        Self::bind_registered(bindings, events)
            .await
            .map_err(|error| error.to_string())
    }

    async fn bind_registered(
        bindings: &[HotkeyBinding],
        events: mpsc::UnboundedSender<HotkeyEvent>,
    ) -> Result<Self, ashpd::Error> {
        let shortcuts = GlobalShortcuts::new().await?;
        if bindings.is_empty() {
            return Ok(Self {
                session: None,
                listener: None,
            });
        }
        let session = Arc::new(shortcuts.create_session(Default::default()).await?);
        let triggers: Vec<String> = bindings
            .iter()
            .map(|binding| binding.accelerator.to_string())
            .collect();
        let wanted: Vec<NewShortcut> = bindings
            .iter()
            .zip(&triggers)
            .map(|(binding, trigger)| {
                NewShortcut::new(binding.action.id(), binding.action.description())
                    .preferred_trigger(trigger.as_str())
            })
            .collect();
        shortcuts
            .bind_shortcuts(&session, &wanted, None, BindShortcutsOptions::default())
            .await?
            .response()?;
        let mut activated = Box::pin(shortcuts.receive_activated().await?);
        let mut deactivated = Box::pin(shortcuts.receive_deactivated().await?);
        let kept = Arc::clone(&session);
        let listener = tokio::spawn(async move {
            // The portal and the session live as long as this task.
            let _kept = (shortcuts, kept);
            loop {
                let event = tokio::select! {
                    Some(signal) = activated.next() => {
                        HotkeyAction::from_id(signal.shortcut_id()).map(HotkeyEvent::Pressed)
                    }
                    Some(signal) = deactivated.next() => {
                        HotkeyAction::from_id(signal.shortcut_id()).map(HotkeyEvent::Released)
                    }
                    else => return,
                };
                if let Some(event) = event
                    && events.send(event).is_err()
                {
                    return;
                }
            }
        });
        Ok(Self {
            session: Some(session),
            listener: Some(listener),
        })
    }
}

impl Drop for Portal {
    fn drop(&mut self) {
        if let Some(listener) = self.listener.take() {
            listener.abort();
        }
        // An open session would keep its shortcuts bound.
        if let Some(session) = self.session.take()
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            runtime.spawn(async move {
                let _ = session.close().await;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkeys::{HotkeyAction, HotkeySupport, Hotkeys};

    /// Needs a Wayland desktop with the GlobalShortcuts portal, and
    /// Opencord's desktop file installed (the portal looks the app id up);
    /// run with `--ignored` there. It opens a shortcuts session and closes
    /// it.
    #[tokio::test]
    #[ignore = "needs the GlobalShortcuts portal and Opencord's desktop file installed"]
    async fn the_desktop_binds_the_hotkeys() {
        let (events, _received) = mpsc::unbounded_channel();
        let mut hotkeys = Hotkeys::new("dev.opencord.opencord", events);
        let binding = HotkeyBinding {
            action: HotkeyAction::PushToTalk,
            accelerator: "CTRL+SHIFT+F12".parse().unwrap(),
        };

        let support = hotkeys.set(&[binding]).await;
        let empty = hotkeys.set(&[]).await;

        assert!(
            matches!(support, HotkeySupport::Global { .. }),
            "{support:?}"
        );
        assert!(matches!(empty, HotkeySupport::Global { .. }), "{empty:?}");
        drop(hotkeys);
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}
