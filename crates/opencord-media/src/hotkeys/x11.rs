//! Hotkeys on X11 sessions: XInput 2 raw key events show every key press
//! wherever the focus is, without taking keys away from other apps (as a
//! key grab would). A thread reads them and matches the bindings.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use tokio::sync::mpsc;
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xinput::{self, ConnectionExt as _};
use x11rb::protocol::xproto::ConnectionExt as _;

use super::{HotkeyAction, HotkeyBinding, HotkeyEvent};

/// How often the thread looks for key events, and for being stopped.
const POLL: Duration = Duration::from_millis(5);
/// XInput 2's "all master devices": every keyboard attached to one.
const ALL_MASTER_DEVICES: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Logo,
}

/// The keysyms of the modifier keys, left and right.
const MODIFIER_KEYSYMS: [(u32, Modifier); 8] = [
    (0xffe3, Modifier::Ctrl),
    (0xffe4, Modifier::Ctrl),
    (0xffe1, Modifier::Shift),
    (0xffe2, Modifier::Shift),
    (0xffe9, Modifier::Alt),
    (0xffea, Modifier::Alt),
    (0xffeb, Modifier::Logo),
    (0xffec, Modifier::Logo),
];

/// One binding, in keycodes.
struct Watched {
    action: HotkeyAction,
    keys: Vec<u32>,
    modifiers: Vec<Modifier>,
}

/// Matches key presses and releases against the bindings.
struct Matcher {
    watched: Vec<Watched>,
    modifiers: HashMap<u32, Modifier>,
    held: HashSet<u32>,
    active: HashSet<HotkeyAction>,
}

impl Matcher {
    /// `keysyms` lists each keycode's keysyms, from `first_keycode` on.
    fn new(bindings: &[HotkeyBinding], first_keycode: u32, keysyms: &[Vec<u32>]) -> Self {
        let keycodes = |keysym: u32| -> Vec<u32> {
            keysyms
                .iter()
                .enumerate()
                .filter(|(_, syms)| syms.contains(&keysym))
                .map(|(index, _)| first_keycode + index as u32)
                .collect()
        };
        let watched = bindings
            .iter()
            .map(|binding| {
                let accelerator = &binding.accelerator;
                let modifiers = [
                    (accelerator.ctrl, Modifier::Ctrl),
                    (accelerator.alt, Modifier::Alt),
                    (accelerator.shift, Modifier::Shift),
                    (accelerator.logo, Modifier::Logo),
                ]
                .into_iter()
                .filter_map(|(held, modifier)| held.then_some(modifier))
                .collect();
                Watched {
                    action: binding.action,
                    keys: keycodes(accelerator.keysym),
                    modifiers,
                }
            })
            .collect();
        let modifiers = MODIFIER_KEYSYMS
            .iter()
            .flat_map(|(keysym, modifier)| {
                keycodes(*keysym).into_iter().map(|key| (key, *modifier))
            })
            .collect();
        Self {
            watched,
            modifiers,
            held: HashSet::new(),
            active: HashSet::new(),
        }
    }

    fn holding(&self, modifier: Modifier) -> bool {
        self.held
            .iter()
            .any(|key| self.modifiers.get(key) == Some(&modifier))
    }

    fn press(&mut self, keycode: u32) -> Vec<HotkeyEvent> {
        self.held.insert(keycode);
        let mut events = Vec::new();
        for watched in &self.watched {
            if watched.keys.contains(&keycode)
                && watched
                    .modifiers
                    .iter()
                    .all(|modifier| self.holding(*modifier))
                && self.active.insert(watched.action)
            {
                events.push(HotkeyEvent::Pressed(watched.action));
            }
        }
        events
    }

    fn release(&mut self, keycode: u32) -> Vec<HotkeyEvent> {
        self.held.remove(&keycode);
        let mut events = Vec::new();
        for watched in &self.watched {
            if watched.keys.contains(&keycode) && self.active.remove(&watched.action) {
                events.push(HotkeyEvent::Released(watched.action));
            }
        }
        events
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum X11Error {
    #[error(transparent)]
    Connect(#[from] x11rb::errors::ConnectError),
    #[error(transparent)]
    Connection(#[from] x11rb::errors::ConnectionError),
    #[error(transparent)]
    Reply(#[from] x11rb::errors::ReplyError),
    #[error("the X server has no XInput 2")]
    NoXInput2,
}

/// The reading thread, while kept.
pub(super) struct X11Keys {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl X11Keys {
    /// Watches `display`'s keyboards for `bindings`.
    pub(super) fn start(
        display: &str,
        bindings: &[HotkeyBinding],
        events: mpsc::UnboundedSender<HotkeyEvent>,
    ) -> Result<Self, X11Error> {
        let (connection, screen) = x11rb::connect(Some(display))?;
        let version = connection.xinput_xi_query_version(2, 2)?.reply()?;
        if version.major_version < 2 {
            return Err(X11Error::NoXInput2);
        }
        let setup = connection.setup();
        let root = setup.roots[screen].root;
        let first = setup.min_keycode;
        let count = setup.max_keycode - first + 1;
        let mapping = connection.get_keyboard_mapping(first, count)?.reply()?;
        let per_keycode = usize::from(mapping.keysyms_per_keycode.max(1));
        let keysyms: Vec<Vec<u32>> = mapping
            .keysyms
            .chunks(per_keycode)
            .map(<[u32]>::to_vec)
            .collect();
        let mut matcher = Matcher::new(bindings, u32::from(first), &keysyms);
        connection.xinput_xi_select_events(
            root,
            &[xinput::EventMask {
                deviceid: ALL_MASTER_DEVICES,
                mask: vec![
                    xinput::XIEventMask::RAW_KEY_PRESS | xinput::XIEventMask::RAW_KEY_RELEASE,
                ],
            }],
        )?;
        connection.flush()?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("opencord-hotkeys".to_owned())
            .spawn(move || {
                while !stopped.load(Ordering::Relaxed) {
                    let found = match connection.poll_for_event() {
                        Ok(Some(event)) => match event {
                            Event::XinputRawKeyPress(key) => matcher.press(key.detail),
                            Event::XinputRawKeyRelease(key) => matcher.release(key.detail),
                            _ => continue,
                        },
                        Ok(None) => {
                            std::thread::sleep(POLL);
                            continue;
                        }
                        // The X server went away.
                        Err(_) => return,
                    };
                    for event in found {
                        if events.send(event).is_err() {
                            return;
                        }
                    }
                }
            })
            .expect("the system can start a thread");
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for X11Keys {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkeys::Accelerator;

    /// A keyboard: keycode 10 Control_L, 11 Shift_L, 12 "m", 13 F12.
    fn matcher(bindings: &[(HotkeyAction, &str)]) -> Matcher {
        let bindings: Vec<HotkeyBinding> = bindings
            .iter()
            .map(|(action, text)| HotkeyBinding {
                action: *action,
                accelerator: text.parse::<Accelerator>().unwrap(),
            })
            .collect();
        let keysyms = vec![
            vec![0xffe3],
            vec![0xffe1],
            vec![u32::from(b'm'), u32::from(b'M')],
            vec![0xffc9],
        ];
        Matcher::new(&bindings, 10, &keysyms)
    }

    #[test]
    fn a_key_with_its_modifiers_presses_and_releases_its_action() {
        let mut keys = matcher(&[(HotkeyAction::PushToTalk, "CTRL+m")]);

        let alone = keys.press(12);
        keys.release(12);
        keys.press(10);
        let with_ctrl = keys.press(12);
        let repeated = keys.press(12);
        let let_go = keys.release(12);

        assert!(alone.is_empty());
        assert_eq!(
            with_ctrl,
            vec![HotkeyEvent::Pressed(HotkeyAction::PushToTalk)]
        );
        assert!(repeated.is_empty(), "held keys do not press again");
        assert_eq!(
            let_go,
            vec![HotkeyEvent::Released(HotkeyAction::PushToTalk)]
        );
    }

    #[test]
    fn extra_modifiers_do_not_stop_a_hotkey() {
        let mut keys = matcher(&[(HotkeyAction::PrioritySpeaker, "F12")]);

        keys.press(11);
        let pressed = keys.press(13);

        assert_eq!(
            pressed,
            vec![HotkeyEvent::Pressed(HotkeyAction::PrioritySpeaker)]
        );
    }

    #[test]
    fn releasing_the_modifier_first_keeps_the_hotkey_until_the_key_goes() {
        let mut keys = matcher(&[(HotkeyAction::PushToTalk, "CTRL+m")]);
        keys.press(10);
        keys.press(12);

        let modifier_up = keys.release(10);
        let key_up = keys.release(12);

        assert!(modifier_up.is_empty());
        assert_eq!(
            key_up,
            vec![HotkeyEvent::Released(HotkeyAction::PushToTalk)]
        );
    }

    /// A private X server, gone when dropped.
    struct Xvfb {
        child: std::process::Child,
        display: String,
    }

    impl Xvfb {
        fn start() -> Self {
            let number = (90..200)
                .find(|n| !std::path::Path::new(&format!("/tmp/.X11-unix/X{n}")).exists())
                .expect("a free display number");
            let display = format!(":{number}");
            let child = std::process::Command::new("Xvfb")
                .args([display.as_str(), "-nolisten", "tcp"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .expect("Xvfb is installed");
            let socket = format!("/tmp/.X11-unix/X{number}");
            for _ in 0..100 {
                if std::path::Path::new(&socket).exists() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Self { child, display }
        }
    }

    impl Drop for Xvfb {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    /// Needs Xvfb; run with `--ignored` where it is installed.
    #[test]
    #[ignore = "needs Xvfb"]
    fn keys_typed_anywhere_on_an_x_server_press_and_release_hotkeys() {
        use x11rb::protocol::xtest::ConnectionExt as _;

        let server = Xvfb::start();
        let (sender, mut received) = mpsc::unbounded_channel();
        let binding = HotkeyBinding {
            action: HotkeyAction::PushToTalk,
            accelerator: "CTRL+m".parse().unwrap(),
        };
        let _keys = X11Keys::start(&server.display, &[binding], sender).unwrap();
        let (typist, screen) = x11rb::connect(Some(&server.display)).unwrap();
        let root = typist.setup().roots[screen].root;
        let first = typist.setup().min_keycode;
        let mapping = typist
            .get_keyboard_mapping(first, typist.setup().max_keycode - first + 1)
            .unwrap()
            .reply()
            .unwrap();
        let per = usize::from(mapping.keysyms_per_keycode);
        let keycode = |keysym: u32| {
            let index = mapping
                .keysyms
                .chunks(per)
                .position(|syms| syms.contains(&keysym))
                .unwrap();
            first + index as u8
        };
        let (ctrl, m) = (keycode(0xffe3), keycode(u32::from(b'm')));

        for (kind, key) in [(2, ctrl), (2, m), (3, m), (3, ctrl)] {
            typist
                .xtest_fake_input(kind, key, 0, root, 0, 0, 0)
                .unwrap();
        }
        typist.flush().unwrap();
        let mut events = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while events.len() < 2 && std::time::Instant::now() < deadline {
            match received.try_recv() {
                Ok(event) => events.push(event),
                Err(_) => std::thread::sleep(Duration::from_millis(10)),
            }
        }

        assert_eq!(
            events,
            vec![
                HotkeyEvent::Pressed(HotkeyAction::PushToTalk),
                HotkeyEvent::Released(HotkeyAction::PushToTalk),
            ]
        );
    }
}
