//! Hotkeys on X11 sessions: XInput 2 raw key and button events show every
//! key press and click wherever the focus is, without taking them away from
//! other apps (as a grab would). A thread reads them and matches the
//! bindings.

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

use super::{Held, HotkeyAction, HotkeyBinding, HotkeyEvent, MouseButton};

/// How often the thread looks for key events, and for being stopped.
const POLL: Duration = Duration::from_millis(5);
/// XInput 2's "all master devices": every keyboard and pointer attached to
/// one.
const ALL_MASTER_DEVICES: u16 = 1;

/// The X button number (a raw event's detail) of each button a hotkey can
/// use. 1 and 3 are left and right, and 4 to 7 the wheel.
fn button_of(detail: u32) -> Option<MouseButton> {
    match detail {
        2 => Some(MouseButton::Middle),
        8 => Some(MouseButton::Back),
        9 => Some(MouseButton::Forward),
        _ => None,
    }
}

/// A button as the matcher counts it: a pseudo-key past every keycode,
/// which X keeps below 256.
fn button_key(button: MouseButton) -> u32 {
    0x1_0000
        + match button {
            MouseButton::Middle => 2,
            MouseButton::Back => 8,
            MouseButton::Forward => 9,
        }
}

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

/// One binding, in keycodes, or a button's pseudo-key.
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
    /// Bindings down, by index.
    down: HashSet<usize>,
    actions: Held,
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
                    keys: match accelerator.button {
                        Some(button) => vec![button_key(button)],
                        None => keycodes(accelerator.keysym),
                    },
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
            down: HashSet::new(),
            actions: Held::default(),
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
        for (index, watched) in self.watched.iter().enumerate() {
            if watched.keys.contains(&keycode)
                && watched
                    .modifiers
                    .iter()
                    .all(|modifier| self.holding(*modifier))
                && self.down.insert(index)
            {
                events.extend(self.actions.press(index, watched.action));
            }
        }
        events
    }

    fn release(&mut self, keycode: u32) -> Vec<HotkeyEvent> {
        self.held.remove(&keycode);
        let mut events = Vec::new();
        for (index, watched) in self.watched.iter().enumerate() {
            if watched.keys.contains(&keycode) && self.down.remove(&index) {
                events.extend(self.actions.release(index, watched.action));
            }
        }
        events
    }

    /// A button went down or came up: the buttons a hotkey can't use
    /// count for nothing.
    fn button(&mut self, detail: u32, down: bool) -> Vec<HotkeyEvent> {
        match button_of(detail).map(button_key) {
            Some(key) if down => self.press(key),
            Some(key) => self.release(key),
            None => Vec::new(),
        }
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
                    xinput::XIEventMask::RAW_KEY_PRESS
                        | xinput::XIEventMask::RAW_KEY_RELEASE
                        | xinput::XIEventMask::RAW_BUTTON_PRESS
                        | xinput::XIEventMask::RAW_BUTTON_RELEASE,
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
                            Event::XinputRawButtonPress(button) => {
                                matcher.button(button.detail, true)
                            }
                            Event::XinputRawButtonRelease(button) => {
                                matcher.button(button.detail, false)
                            }
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

    #[test]
    fn push_to_talk_on_two_keys_lasts_until_both_are_up() {
        let mut keys = matcher(&[
            (HotkeyAction::PushToTalk, "CTRL+m"),
            (HotkeyAction::PushToTalk, "F12"),
        ]);
        keys.press(10);

        let first = keys.press(12);
        let second = keys.press(13);
        let one_up = keys.release(12);
        let last_up = keys.release(13);

        assert_eq!(first, vec![HotkeyEvent::Pressed(HotkeyAction::PushToTalk)]);
        assert!(second.is_empty());
        assert!(one_up.is_empty(), "F12 is still down");
        assert_eq!(
            last_up,
            vec![HotkeyEvent::Released(HotkeyAction::PushToTalk)]
        );
    }

    #[test]
    fn a_modifier_key_alone_is_a_hotkey() {
        let mut keys = matcher(&[(HotkeyAction::PushToTalk, "Control_L")]);

        let down = keys.press(10);
        let up = keys.release(10);

        assert_eq!(down, vec![HotkeyEvent::Pressed(HotkeyAction::PushToTalk)]);
        assert_eq!(up, vec![HotkeyEvent::Released(HotkeyAction::PushToTalk)]);
    }

    const BACK: u32 = 8;

    #[test]
    fn a_button_with_its_modifiers_presses_and_releases() {
        let mut keys = matcher(&[(HotkeyAction::PushToTalk, "CTRL+Mouse4")]);

        keys.press(10);
        let down = keys.button(BACK, true);
        let again = keys.button(BACK, true);
        let up = keys.button(BACK, false);

        assert_eq!(down, vec![HotkeyEvent::Pressed(HotkeyAction::PushToTalk)]);
        assert!(again.is_empty(), "a held button does not press again");
        assert_eq!(up, vec![HotkeyEvent::Released(HotkeyAction::PushToTalk)]);
    }

    #[test]
    fn a_button_without_its_modifiers_does_nothing() {
        let mut keys = matcher(&[(HotkeyAction::ToggleMute, "CTRL+Mouse4")]);

        let down = keys.button(BACK, true);
        let up = keys.button(BACK, false);
        // Nor does Ctrl coming down while the button already is.
        keys.button(BACK, true);
        let ctrl_after = keys.press(10);

        assert!(down.is_empty() && up.is_empty());
        assert!(ctrl_after.is_empty());
    }

    #[test]
    fn a_key_and_a_button_on_one_action_press_once() {
        let mut keys = matcher(&[
            (HotkeyAction::PushToTalk, "F12"),
            (HotkeyAction::PushToTalk, "Mouse5"),
        ]);

        let key = keys.press(13);
        let button = keys.button(9, true);
        let key_up = keys.release(13);
        let button_up = keys.button(9, false);

        assert_eq!(key, vec![HotkeyEvent::Pressed(HotkeyAction::PushToTalk)]);
        assert!(button.is_empty(), "already down");
        assert!(key_up.is_empty(), "the button still holds it");
        assert_eq!(
            button_up,
            vec![HotkeyEvent::Released(HotkeyAction::PushToTalk)]
        );
    }

    #[test]
    fn button_numbers_map_to_the_three_buttons() {
        assert_eq!(button_of(2), Some(MouseButton::Middle));
        assert_eq!(button_of(8), Some(MouseButton::Back));
        assert_eq!(button_of(9), Some(MouseButton::Forward));
        for unbindable in [1, 3, 4, 5, 6, 7, 10] {
            assert_eq!(button_of(unbindable), None, "button {unbindable}");
        }
        let mut keys = matcher(&[(HotkeyAction::ToggleMute, "Mouse3")]);
        assert!(keys.button(4, true).is_empty(), "the wheel");
        assert_eq!(
            keys.button(2, true),
            vec![HotkeyEvent::Pressed(HotkeyAction::ToggleMute)]
        );
    }

    #[test]
    fn a_button_is_no_key() {
        // A button's keysym is 0, which unmapped keycodes carry too.
        let mut keys = Matcher::new(
            &[HotkeyBinding {
                action: HotkeyAction::PushToTalk,
                accelerator: "Mouse4".parse().unwrap(),
            }],
            10,
            &[vec![0], vec![0, 0]],
        );

        assert!(keys.press(10).is_empty());
        assert!(keys.press(11).is_empty());
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

    /// Types `keys` (X keysym, down?) on a private X server watched for
    /// `bindings`, and returns the hotkey events that came.
    fn typed_on_x(
        bindings: &[(HotkeyAction, &str)],
        keys: &[(u32, bool)],
        expected: usize,
    ) -> Vec<HotkeyEvent> {
        let typed: Vec<Input> = keys
            .iter()
            .map(|(keysym, down)| Input::Key(*keysym, *down))
            .collect();
        input_on_x(bindings, &typed, expected)
    }

    /// A key (X keysym) or a button (X button number), down or up.
    enum Input {
        Key(u32, bool),
        Button(u8, bool),
    }

    /// Sends `inputs` to a private X server watched for `bindings`, and
    /// returns the hotkey events that came.
    fn input_on_x(
        bindings: &[(HotkeyAction, &str)],
        inputs: &[Input],
        expected: usize,
    ) -> Vec<HotkeyEvent> {
        use x11rb::protocol::xtest::ConnectionExt as _;

        let server = Xvfb::start();
        let (sender, mut received) = mpsc::unbounded_channel();
        let bindings: Vec<HotkeyBinding> = bindings
            .iter()
            .map(|(action, text)| HotkeyBinding {
                action: *action,
                accelerator: text.parse().unwrap(),
            })
            .collect();
        let _keys = X11Keys::start(&server.display, &bindings, sender).unwrap();
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

        for input in inputs {
            // KeyPress, KeyRelease, ButtonPress, ButtonRelease.
            let (kind, detail) = match *input {
                Input::Key(keysym, down) => (if down { 2 } else { 3 }, keycode(keysym)),
                Input::Button(button, down) => (if down { 4 } else { 5 }, button),
            };
            typist
                .xtest_fake_input(kind, detail, 0, root, 0, 0, 0)
                .unwrap();
        }
        typist.flush().unwrap();
        let mut events = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while events.len() < expected && std::time::Instant::now() < deadline {
            match received.try_recv() {
                Ok(event) => events.push(event),
                Err(_) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        // Anything more would come at once.
        std::thread::sleep(Duration::from_millis(200));
        while let Ok(event) = received.try_recv() {
            events.push(event);
        }
        events
    }

    /// Needs Xvfb; run with `--ignored` where it is installed.
    #[test]
    #[ignore = "needs Xvfb"]
    fn keys_typed_anywhere_on_an_x_server_press_and_release_hotkeys() {
        let (ctrl, m) = (0xffe3, u32::from(b'm'));

        let events = typed_on_x(
            &[(HotkeyAction::PushToTalk, "CTRL+m")],
            &[(ctrl, true), (m, true), (m, false), (ctrl, false)],
            2,
        );

        assert_eq!(
            events,
            vec![
                HotkeyEvent::Pressed(HotkeyAction::PushToTalk),
                HotkeyEvent::Released(HotkeyAction::PushToTalk),
            ]
        );
    }

    /// Needs Xvfb; run with `--ignored` where it is installed.
    #[test]
    #[ignore = "needs Xvfb"]
    fn buttons_clicked_anywhere_on_an_x_server_press_and_release_hotkeys() {
        let ctrl = 0xffe3;

        let events = input_on_x(
            &[(HotkeyAction::PushToTalk, "CTRL+Mouse4")],
            &[
                // Without Ctrl, then the wheel and a left click: nothing.
                Input::Button(8, true),
                Input::Button(8, false),
                Input::Button(4, true),
                Input::Button(4, false),
                Input::Button(1, true),
                Input::Button(1, false),
                Input::Key(ctrl, true),
                Input::Button(8, true),
                Input::Button(8, false),
                Input::Key(ctrl, false),
            ],
            2,
        );

        assert_eq!(
            events,
            vec![
                HotkeyEvent::Pressed(HotkeyAction::PushToTalk),
                HotkeyEvent::Released(HotkeyAction::PushToTalk),
            ]
        );
    }

    /// Needs Xvfb; run with `--ignored` where it is installed.
    #[test]
    #[ignore = "needs Xvfb"]
    fn push_to_talk_on_two_keys_of_an_x_server_holds_until_both_are_up() {
        let (grave, right_ctrl) = (0x60, 0xffe4);

        let events = typed_on_x(
            &[
                (HotkeyAction::PushToTalk, "grave"),
                (HotkeyAction::PushToTalk, "Control_R"),
            ],
            &[
                (grave, true),
                (right_ctrl, true),
                (grave, false),
                (right_ctrl, false),
            ],
            2,
        );

        assert_eq!(
            events,
            vec![
                HotkeyEvent::Pressed(HotkeyAction::PushToTalk),
                HotkeyEvent::Released(HotkeyAction::PushToTalk),
            ]
        );
    }
}
