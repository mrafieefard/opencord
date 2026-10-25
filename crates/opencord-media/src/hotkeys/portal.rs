//! Hotkeys on Wayland desktops, through the xdg-desktop-portal
//! GlobalShortcuts interface. Opencord says which keys it would like; the
//! desktop decides, and may ask the user (KDE and GNOME show a dialog;
//! Hyprland binds them in its own configuration).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use ashpd::AppID;
use ashpd::desktop::Session;
use ashpd::desktop::global_shortcuts::{BindShortcutsOptions, GlobalShortcuts, NewShortcut};
use futures_util::StreamExt;
use tokio::sync::{OnceCell, mpsc};
use tokio::task::JoinHandle;

use super::{Held, HotkeyBinding, HotkeyEvent, action_of, desktop_entry, shortcut_ids};

/// Whether this process told the portal who it is; once, before any other
/// portal call (an app outside a sandbox has no app id otherwise).
static REGISTERED: OnceCell<Result<(), String>> = OnceCell::const_new();

async fn register(app_id: &str) -> Result<(), String> {
    REGISTERED
        .get_or_init(|| async {
            let id: AppID = app_id.parse().map_err(|error| format!("{error}"))?;
            // The portal knows an app by its desktop file; a tarball has
            // none until Opencord writes one.
            let written = ensure_desktop_entry(app_id);
            let mut tries = 0;
            loop {
                match ashpd::register_host_app(id.clone()).await {
                    Ok(()) => return Ok(()),
                    // The desktop notices a new file a moment after.
                    Err(error) if written && tries < REGISTER_TRIES && unknown_app(&error) => {
                        tries += 1;
                        tokio::time::sleep(REGISTER_WAIT).await;
                    }
                    Err(error) => return Err(error.to_string()),
                }
            }
        })
        .await
        .clone()
}

/// How long the desktop gets to find a desktop file just written.
const REGISTER_TRIES: u32 = 10;
const REGISTER_WAIT: Duration = Duration::from_millis(300);

/// Writes the app's own desktop file where no package installed one;
/// whether one was written just now. Inside a Flatpak, the sandbox says
/// who the app is.
fn ensure_desktop_entry(app_id: &str) -> bool {
    // A test's binary is not the app to launch.
    if cfg!(test) || Path::new("/.flatpak-info").exists() {
        return false;
    }
    // Started by the launcher (docs/updater.md), the desktop file launches
    // it, not this version's executable, which the next update removes.
    let launcher = std::env::var_os("OPENCORD_LAUNCHER")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from);
    let entry = launcher
        .map(Ok)
        .unwrap_or_else(std::env::current_exe)
        .and_then(|exe| desktop_entry::ensure(app_id, &exe));
    match entry {
        Ok(entry) => {
            tracing::debug!(?entry, "desktop file");
            entry == desktop_entry::Entry::Written
        }
        Err(error) => {
            tracing::warn!(%error, "could not write the desktop file global shortcuts need");
            false
        }
    }
}

/// The portal could not find the app's desktop file.
fn unknown_app(error: &ashpd::Error) -> bool {
    error.to_string().contains("App info not found")
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
        let ids = shortcut_ids(bindings);
        let wanted: Vec<NewShortcut> = ids
            .iter()
            .zip(&triggers)
            .map(|((id, description), trigger)| {
                NewShortcut::new(id.as_str(), description.as_str())
                    .preferred_trigger(trigger.as_str())
            })
            .collect();
        let index_of: std::collections::HashMap<String, usize> = ids
            .into_iter()
            .enumerate()
            .map(|(index, (id, _))| (id, index))
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
            let mut held = Held::default();
            loop {
                let (id, down) = tokio::select! {
                    Some(signal) = activated.next() => (signal.shortcut_id().to_owned(), true),
                    Some(signal) = deactivated.next() => (signal.shortcut_id().to_owned(), false),
                    else => return,
                };
                let event = action_of(&id)
                    .zip(index_of.get(&id))
                    .and_then(|(action, &index)| match down {
                        true => held.press(index, action),
                        false => held.release(index, action),
                    });
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
    /// Opencord's desktop file installed (the portal looks the app id up;
    /// the app writes it on its first run, a test does not); run with
    /// `--ignored` there. It opens a shortcuts session and closes
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
