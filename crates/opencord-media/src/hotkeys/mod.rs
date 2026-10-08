//! Global hotkeys (plan §7.13): push-to-talk and the priority speaker key
//! work while Opencord is in the background, straight from here to the
//! audio engine; the mute and deafen toggles go to the app.
//!
//! On Linux, Wayland desktops bind them through the GlobalShortcuts portal
//! and X11 sessions read the keyboard through XInput 2. Elsewhere (for now)
//! hotkeys only work while Opencord is focused, and [`Hotkeys::set`] says so.

#[cfg(target_os = "linux")]
mod portal;
#[cfg(target_os = "linux")]
mod x11;

use tokio::sync::mpsc;

/// What a hotkey does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HotkeyAction {
    PushToTalk,
    PrioritySpeaker,
    ToggleMute,
    ToggleDeafen,
}

impl HotkeyAction {
    /// The id the portal knows it by.
    fn id(self) -> &'static str {
        match self {
            Self::PushToTalk => "push-to-talk",
            Self::PrioritySpeaker => "priority-speaker",
            Self::ToggleMute => "toggle-mute",
            Self::ToggleDeafen => "toggle-deafen",
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        [
            Self::PushToTalk,
            Self::PrioritySpeaker,
            Self::ToggleMute,
            Self::ToggleDeafen,
        ]
        .into_iter()
        .find(|action| action.id() == id)
    }

    /// What the system's shortcut settings show.
    fn description(self) -> &'static str {
        match self {
            Self::PushToTalk => "Push to talk",
            Self::PrioritySpeaker => "Priority speaker",
            Self::ToggleMute => "Toggle mute",
            Self::ToggleDeafen => "Toggle deafen",
        }
    }
}

/// A key and the modifiers held with it, written as the XDG shortcuts
/// specification writes them: `CTRL+SHIFT+m`, `LOGO+F12`, `grave`. The key
/// is an xkb keysym name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accelerator {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub logo: bool,
    /// The keysym name, as written.
    pub key: String,
    pub keysym: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AcceleratorError {
    #[error("no key in \"{0}\"")]
    NoKey(String),
    #[error("\"{0}\" is not a key a hotkey can use")]
    UnknownKey(String),
    #[error("\"{0}\" is not a modifier (CTRL, ALT, SHIFT or LOGO)")]
    UnknownModifier(String),
}

impl std::str::FromStr for Accelerator {
    type Err = AcceleratorError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut parts: Vec<&str> = text.split('+').map(str::trim).collect();
        let key = parts.pop().filter(|key| !key.is_empty());
        let key = key.ok_or_else(|| AcceleratorError::NoKey(text.to_owned()))?;
        let keysym = keysym(key).ok_or_else(|| AcceleratorError::UnknownKey(key.to_owned()))?;
        let mut accelerator = Self {
            ctrl: false,
            alt: false,
            shift: false,
            logo: false,
            key: key.to_owned(),
            keysym,
        };
        for modifier in parts {
            match modifier.to_ascii_uppercase().as_str() {
                "CTRL" => accelerator.ctrl = true,
                "ALT" => accelerator.alt = true,
                "SHIFT" => accelerator.shift = true,
                "LOGO" => accelerator.logo = true,
                _ => return Err(AcceleratorError::UnknownModifier(modifier.to_owned())),
            }
        }
        Ok(accelerator)
    }
}

impl std::fmt::Display for Accelerator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (held, name) in [
            (self.ctrl, "CTRL+"),
            (self.alt, "ALT+"),
            (self.shift, "SHIFT+"),
            (self.logo, "LOGO+"),
        ] {
            if held {
                f.write_str(name)?;
            }
        }
        f.write_str(&self.key)
    }
}

/// The keysyms of the keys a hotkey can use: letters, digits, F1–F24, and
/// keys left free in most apps.
fn keysym(name: &str) -> Option<u32> {
    if let [character] = name.as_bytes() {
        return match character {
            b'a'..=b'z' | b'0'..=b'9' => Some(u32::from(*character)),
            _ => None,
        };
    }
    if let Some(number) = name.strip_prefix('F').and_then(|n| n.parse::<u32>().ok())
        && (1..=24).contains(&number)
    {
        return Some(0xffbe + number - 1);
    }
    if let Some(digit) = name.strip_prefix("KP_").and_then(|n| n.parse::<u32>().ok())
        && digit <= 9
    {
        return Some(0xffb0 + digit);
    }
    Some(match name {
        "space" => 0x20,
        "apostrophe" => 0x27,
        "comma" => 0x2c,
        "minus" => 0x2d,
        "period" => 0x2e,
        "slash" => 0x2f,
        "semicolon" => 0x3b,
        "equal" => 0x3d,
        "bracketleft" => 0x5b,
        "backslash" => 0x5c,
        "bracketright" => 0x5d,
        "grave" => 0x60,
        "Tab" => 0xff09,
        "Pause" => 0xff13,
        "Scroll_Lock" => 0xff14,
        "Home" => 0xff50,
        "Prior" => 0xff55,
        "Next" => 0xff56,
        "End" => 0xff57,
        "Insert" => 0xff63,
        "Delete" => 0xffff,
        _ => return None,
    })
}

/// A hotkey the app asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyBinding {
    pub action: HotkeyAction,
    pub accelerator: Accelerator,
}

/// A hotkey went down or up, wherever the focus was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    Pressed(HotkeyAction),
    Released(HotkeyAction),
}

/// Whether the hotkeys work while Opencord is in the background.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotkeySupport {
    /// System-wide, through `method`.
    Global { method: String },
    /// Only while Opencord is focused, because of `reason`.
    FocusedOnly { reason: String },
}

/// The system's global hotkeys, while kept.
pub struct Hotkeys {
    /// The app's id, as its desktop file names it; portals want it.
    app_id: String,
    events: mpsc::UnboundedSender<HotkeyEvent>,
    #[cfg(target_os = "linux")]
    portal: Option<portal::Portal>,
    #[cfg(target_os = "linux")]
    x11: Option<x11::X11Keys>,
}

impl Hotkeys {
    /// Hotkey presses and releases go to `events`; `app_id` is the app's
    /// id, as its desktop file names it.
    pub fn new(app_id: &str, events: mpsc::UnboundedSender<HotkeyEvent>) -> Self {
        Self {
            app_id: app_id.to_owned(),
            events,
            #[cfg(target_os = "linux")]
            portal: None,
            #[cfg(target_os = "linux")]
            x11: None,
        }
    }

    /// Binds these hotkeys system-wide where the system allows, replacing
    /// the ones bound before. Needs a tokio runtime.
    pub async fn set(&mut self, bindings: &[HotkeyBinding]) -> HotkeySupport {
        self.set_on_this_system(bindings).await
    }

    #[cfg(target_os = "linux")]
    async fn set_on_this_system(&mut self, bindings: &[HotkeyBinding]) -> HotkeySupport {
        self.portal = None;
        self.x11 = None;
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            // XWayland sees no keys meant for Wayland windows, so only the
            // portal can help here.
            return match portal::Portal::bind(&self.app_id, bindings, self.events.clone()).await {
                Ok(portal) => {
                    self.portal = Some(portal);
                    HotkeySupport::Global {
                        method: "the desktop's global shortcuts".to_owned(),
                    }
                }
                Err(error) => HotkeySupport::FocusedOnly {
                    reason: format!("this desktop offers no global shortcuts ({error})"),
                },
            };
        }
        if let Some(display) = std::env::var_os("DISPLAY") {
            return match x11::X11Keys::start(
                &display.to_string_lossy(),
                bindings,
                self.events.clone(),
            ) {
                Ok(keys) => {
                    self.x11 = Some(keys);
                    HotkeySupport::Global {
                        method: "X11 input".to_owned(),
                    }
                }
                Err(error) => HotkeySupport::FocusedOnly {
                    reason: format!("the X server's keyboard cannot be read ({error})"),
                },
            };
        }
        HotkeySupport::FocusedOnly {
            reason: "no graphical session was found".to_owned(),
        }
    }

    #[cfg(not(target_os = "linux"))]
    async fn set_on_this_system(&mut self, _bindings: &[HotkeyBinding]) -> HotkeySupport {
        let _ = (&self.app_id, &self.events);
        HotkeySupport::FocusedOnly {
            reason: "global hotkeys are not available on this system yet".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accelerators_are_written_as_the_shortcuts_specification_writes_them() {
        let push_to_talk: Accelerator = "CTRL+SHIFT+m".parse().unwrap();
        let function: Accelerator = "LOGO+F12".parse().unwrap();
        let bare: Accelerator = "grave".parse().unwrap();

        assert!(push_to_talk.ctrl && push_to_talk.shift && !push_to_talk.alt);
        assert_eq!(push_to_talk.keysym, u32::from(b'm'));
        assert_eq!(function.keysym, 0xffc9);
        assert!(function.logo);
        assert_eq!(bare.keysym, 0x60);
        assert_eq!(push_to_talk.to_string(), "CTRL+SHIFT+m");
        assert_eq!(
            "ctrl+alt+KP_5".parse::<Accelerator>().unwrap().to_string(),
            "CTRL+ALT+KP_5"
        );
    }

    #[test]
    fn unknown_keys_and_modifiers_are_refused() {
        assert_eq!(
            "CTRL+".parse::<Accelerator>(),
            Err(AcceleratorError::NoKey("CTRL+".to_owned()))
        );
        assert_eq!(
            "CTRL+Return".parse::<Accelerator>(),
            Err(AcceleratorError::UnknownKey("Return".to_owned()))
        );
        assert_eq!(
            "HYPER+m".parse::<Accelerator>(),
            Err(AcceleratorError::UnknownModifier("HYPER".to_owned()))
        );
        assert!("F25".parse::<Accelerator>().is_err());
        assert!(
            "M".parse::<Accelerator>().is_err(),
            "keysym names are lower case"
        );
    }

    #[test]
    fn actions_keep_their_portal_ids() {
        for action in [
            HotkeyAction::PushToTalk,
            HotkeyAction::PrioritySpeaker,
            HotkeyAction::ToggleMute,
            HotkeyAction::ToggleDeafen,
        ] {
            assert_eq!(HotkeyAction::from_id(action.id()), Some(action));
        }
        assert_eq!(HotkeyAction::from_id("something-else"), None);
    }
}
