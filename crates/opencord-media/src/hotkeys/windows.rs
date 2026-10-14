//! Windows' global hotkeys (docs/plans/windows.md W2): a low-level keyboard
//! hook on a thread of its own. `WH_KEYBOARD_LL` shows every key press
//! wherever the focus is and passes it on untouched, so Opencord sees a key
//! without taking it from the app the user is in. `RegisterHotKey` would take
//! it, even for the toggles, so it isn't used at all.
//!
//! The contract, as `linux.rs` keeps it: `bind` binds `bindings`
//! system-wide, replacing nothing (the caller drops the old `Keys` first), and
//! sends each press and release to `events` through [`super::Held`], so an
//! action with several keys presses when the first goes down and releases
//! when the last comes up. The keys stay bound while the returned `Keys` is
//! kept and unbind when it is dropped. Accelerators name keys as xkb keysyms
//! (`CTRL+SHIFT+m`, `Control_R`, `F13`, `KP_5`). The hook procedure must
//! never block or swallow a key.
//!
//! Keys typed into a window of a process running as administrator never reach
//! a hook in a normal one (UIPI), so push-to-talk inside such a game needs
//! Opencord started as administrator too.

use std::cell::RefCell;
use std::collections::HashSet;
use std::thread::JoinHandle;
use std::time::Duration;

use tokio::sync::mpsc;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyboardLayout, HKL, VIRTUAL_KEY, VK_ADD, VK_APPS, VK_BACK, VK_CAPITAL, VK_CONTROL,
    VK_DECIMAL, VK_DELETE, VK_DIVIDE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1, VK_HOME, VK_INSERT,
    VK_LCONTROL, VK_LEFT, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_MULTIPLY, VK_NEXT, VK_NUMLOCK,
    VK_NUMPAD0, VK_PAUSE, VK_PRIOR, VK_RCONTROL, VK_RETURN, VK_RIGHT, VK_RMENU, VK_RSHIFT, VK_RWIN,
    VK_SCROLL, VK_SHIFT, VK_SNAPSHOT, VK_SUBTRACT, VK_TAB, VK_UP, VkKeyScanExW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, HC_ACTION, HHOOK,
    KBDLLHOOKSTRUCT, KillTimer, LLKHF_EXTENDED, MSG, PostThreadMessageW, SetTimer,
    SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYUP, WM_QUIT, WM_SYSKEYUP,
    WM_TIMER,
};

use super::{Held, HotkeyAction, HotkeyBinding, HotkeyEvent, HotkeySupport};

/// How often the hook thread looks for a new keyboard layout. Printable keys
/// sit on different virtual-key codes in different layouts, so the table has
/// to follow whatever the user is typing in.
const LAYOUT_CHECK: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Logo,
}

/// The modifier keys, left and right. The hook says which half was pressed;
/// the codes that don't say (`VK_CONTROL` and friends) only arrive from
/// injected input, and count all the same.
const MODIFIER_KEYS: [(VIRTUAL_KEY, Modifier); 11] = [
    (VK_LCONTROL, Modifier::Ctrl),
    (VK_RCONTROL, Modifier::Ctrl),
    (VK_CONTROL, Modifier::Ctrl),
    (VK_LSHIFT, Modifier::Shift),
    (VK_RSHIFT, Modifier::Shift),
    (VK_SHIFT, Modifier::Shift),
    (VK_LMENU, Modifier::Alt),
    (VK_RMENU, Modifier::Alt),
    (VK_MENU, Modifier::Alt),
    (VK_LWIN, Modifier::Logo),
    (VK_RWIN, Modifier::Logo),
];

/// The modifier `vk` is, if it is one.
fn modifier_of(vk: u16) -> Option<Modifier> {
    MODIFIER_KEYS
        .iter()
        .find(|(key, _)| key.0 == vk)
        .map(|(_, modifier)| *modifier)
}

/// A key a binding watches: a virtual-key code, and where on the keyboard it
/// has to come from when the code alone doesn't say. `extended` is `Some` for
/// the keys that share a code with a keypad key, which the hook tells apart
/// by the extended-key flag, and `None` where either half will do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Watch {
    vk: u16,
    extended: Option<bool>,
}

impl Watch {
    /// Either half of the keyboard.
    const fn either(vk: VIRTUAL_KEY) -> Self {
        Self {
            vk: vk.0,
            extended: None,
        }
    }

    fn matches(self, vk: u16, extended: bool) -> bool {
        self.vk == vk && self.extended.is_none_or(|wanted| wanted == extended)
    }
}

/// The keys that aren't printable, as (keysym, virtual key, extended?).
///
/// `Some(true)` is a key the hook reports as extended and `Some(false)` one it
/// never does: that is how the navigation cluster, the arrows and the keypad's
/// Enter are told apart from the keypad keys that share their virtual-key code
/// while Num Lock is off, and from `Return`. X11 gives those a keysym of their
/// own (`KP_Left`), which no accelerator can name, so a keypad key with Num
/// Lock off should match nothing — not the arrow the user bound.
const FIXED_KEYS: &[(u32, VIRTUAL_KEY, Option<bool>)] = &[
    (0xff09, VK_TAB, None),
    (0xff13, VK_PAUSE, None),
    (0xff14, VK_SCROLL, None),
    (0xff50, VK_HOME, Some(true)),
    (0xff55, VK_PRIOR, Some(true)),
    (0xff56, VK_NEXT, Some(true)),
    (0xff57, VK_END, Some(true)),
    (0xff63, VK_INSERT, Some(true)),
    (0xffff, VK_DELETE, Some(true)),
    (0xff08, VK_BACK, None),
    (0xff0d, VK_RETURN, Some(false)),
    (0xff1b, VK_ESCAPE, None),
    (0xff51, VK_LEFT, Some(true)),
    (0xff52, VK_UP, Some(true)),
    (0xff53, VK_RIGHT, Some(true)),
    (0xff54, VK_DOWN, Some(true)),
    (0xff61, VK_SNAPSHOT, None),
    (0xff67, VK_APPS, None),
    (0xff7f, VK_NUMLOCK, None),
    // The keypad's own Enter: the same code as Return, extended.
    (0xff8d, VK_RETURN, Some(true)),
    (0xffaa, VK_MULTIPLY, None),
    (0xffab, VK_ADD, None),
    (0xffad, VK_SUBTRACT, None),
    (0xffae, VK_DECIMAL, None),
    (0xffaf, VK_DIVIDE, None),
    (0xffe1, VK_LSHIFT, None),
    (0xffe2, VK_RSHIFT, None),
    // A real left Ctrl is never extended; see `is_altgr_ctrl`.
    (0xffe3, VK_LCONTROL, Some(false)),
    (0xffe4, VK_RCONTROL, None),
    (0xffe5, VK_CAPITAL, None),
    (0xffe9, VK_LMENU, None),
    (0xffea, VK_RMENU, None),
    (0xffeb, VK_LWIN, None),
    (0xffec, VK_RWIN, None),
];

/// The key a keysym names, in `layout`. `None` is a key this keyboard cannot
/// press, which then matches nothing.
fn watch(keysym: u32, layout: HKL) -> Option<Watch> {
    // F1 to F24, and the keypad's digits, are each a run of codes.
    if (0xffbe..=0xffd5).contains(&keysym) {
        return Some(Watch::either(VIRTUAL_KEY(
            VK_F1.0 + (keysym - 0xffbe) as u16,
        )));
    }
    if (0xffb0..=0xffb9).contains(&keysym) {
        return Some(Watch::either(VIRTUAL_KEY(
            VK_NUMPAD0.0 + (keysym - 0xffb0) as u16,
        )));
    }
    if let Some((_, vk, extended)) = FIXED_KEYS.iter().find(|(name, _, _)| *name == keysym) {
        return Some(Watch {
            vk: vk.0,
            extended: *extended,
        });
    }
    // Everything left is a printable key, whose keysym is its ASCII
    // character. Which key types it depends on the layout, so ask the layout.
    // The shift state it comes back with is dropped: the accelerator says for
    // itself whether Shift is held, exactly as X11 takes every keycode that
    // carries the keysym at any shift level.
    let character = u16::try_from(keysym).ok().filter(|_| keysym < 0x80)?;
    let typed = unsafe { VkKeyScanExW(character, layout) };
    if typed != -1 {
        return Some(Watch::either(VIRTUAL_KEY(typed as u16 & 0xff)));
    }
    // A layout that cannot type it at all: the letters and digits are still
    // where the virtual-key codes came from, so fall back to those.
    match character {
        0x61..=0x7a => Some(Watch::either(VIRTUAL_KEY(character - 0x20))),
        0x30..=0x39 => Some(Watch::either(VIRTUAL_KEY(character))),
        _ => None,
    }
}

/// AltGr's phantom Ctrl. A layout with an AltGr key sends a left-Ctrl event
/// just before the right-Alt one, so that an app which knows nothing of AltGr
/// still sees CTRL+ALT. It carries the extended-key flag, which a real left
/// Ctrl never does, and dropping it keeps AltGr+m out of a `CTRL+ALT+m`
/// binding — X11 avoids the same trap by making AltGr `ISO_Level3_Shift`,
/// which is no modifier of ours at all.
fn is_altgr_ctrl(vk: u16, extended: bool) -> bool {
    vk == VK_LCONTROL.0 && extended
}

/// One binding, in virtual-key codes.
struct Watched {
    action: HotkeyAction,
    key: Option<Watch>,
    modifiers: Vec<Modifier>,
}

impl Watched {
    fn new(binding: &HotkeyBinding, layout: HKL) -> Self {
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
        Self {
            action: binding.action,
            key: watch(accelerator.keysym, layout),
            modifiers,
        }
    }
}

/// Matches key presses and releases against the bindings, as `x11.rs` does.
struct Matcher {
    /// Kept so the table can be built again for another layout.
    bindings: Vec<HotkeyBinding>,
    layout: HKL,
    watched: Vec<Watched>,
    /// Which virtual keys are down.
    held: HashSet<u16>,
    /// Bindings down, by index.
    down: HashSet<usize>,
    actions: Held,
}

impl Matcher {
    fn new(bindings: &[HotkeyBinding], layout: HKL) -> Self {
        let mut matcher = Self {
            bindings: bindings.to_vec(),
            layout,
            watched: Vec::new(),
            held: HashSet::new(),
            down: HashSet::new(),
            actions: Held::default(),
        };
        matcher.rebuild();
        matcher
    }

    /// The table again, for `layout`. Nothing held is forgotten: `held` is in
    /// virtual-key codes, which the hook reports whatever the layout is.
    fn relayout(&mut self, layout: HKL) {
        if layout == self.layout {
            return;
        }
        self.layout = layout;
        self.rebuild();
    }

    fn rebuild(&mut self) {
        let layout = self.layout;
        let watched = self
            .bindings
            .iter()
            .map(|binding| Watched::new(binding, layout))
            .collect();
        self.watched = watched;
    }

    /// A key went down or came up, as the hook saw it.
    fn key(&mut self, vk: u16, extended: bool, up: bool) -> Vec<HotkeyEvent> {
        if is_altgr_ctrl(vk, extended) {
            return Vec::new();
        }
        if up {
            self.release(vk, extended)
        } else {
            self.press(vk, extended)
        }
    }

    fn holding(&self, modifier: Modifier) -> bool {
        self.held
            .iter()
            .any(|vk| modifier_of(*vk) == Some(modifier))
    }

    fn press(&mut self, vk: u16, extended: bool) -> Vec<HotkeyEvent> {
        self.held.insert(vk);
        let mut events = Vec::new();
        for (index, watched) in self.watched.iter().enumerate() {
            if watched.key.is_some_and(|key| key.matches(vk, extended))
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

    fn release(&mut self, vk: u16, extended: bool) -> Vec<HotkeyEvent> {
        self.held.remove(&vk);
        let mut events = Vec::new();
        for (index, watched) in self.watched.iter().enumerate() {
            if watched.key.is_some_and(|key| key.matches(vk, extended)) && self.down.remove(&index)
            {
                events.extend(self.actions.release(index, watched.action));
            }
        }
        events
    }
}

/// What the hook procedure works on. It lives on the hook's own thread,
/// because that is the only thread the procedure is ever called from.
struct Hooked {
    matcher: Matcher,
    events: mpsc::UnboundedSender<HotkeyEvent>,
}

thread_local! {
    static HOOKED: RefCell<Option<Hooked>> = const { RefCell::new(None) };
}

/// The hook procedure. It maps the key, counts it, sends what it means and
/// hands the key straight on: it never swallows one, and never does anything
/// that could block, because Windows quietly drops a hook that takes longer
/// than `LowLevelHooksTimeout`.
unsafe extern "system" fn hook(code: i32, event: WPARAM, data: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let key = unsafe { *(data.0 as *const KBDLLHOOKSTRUCT) };
        let up = matches!(event.0 as u32, WM_KEYUP | WM_SYSKEYUP);
        HOOKED.with(|hooked| {
            // Only this thread ever borrows it, so a refusal would mean the
            // procedure ran inside itself; losing the key beats blocking.
            if let Ok(mut hooked) = hooked.try_borrow_mut()
                && let Some(hooked) = hooked.as_mut()
            {
                let extended = key.flags.0 & LLKHF_EXTENDED.0 != 0;
                for event in hooked.matcher.key(key.vkCode as u16, extended, up) {
                    let _ = hooked.events.send(event);
                }
            }
        });
    }
    unsafe { CallNextHookEx(None, code, event, data) }
}

/// The keyboard layout of whatever window has the focus: the layout the keys
/// the user is pressing are typed in. A thread id of 0 asks for this thread's
/// own, which is the right answer when no window has the focus.
fn foreground_layout() -> HKL {
    let thread = unsafe { GetWindowThreadProcessId(GetForegroundWindow(), None) };
    unsafe { GetKeyboardLayout(thread) }
}

#[derive(Debug, thiserror::Error)]
enum HookError {
    #[error(transparent)]
    Install(#[from] windows::core::Error),
    #[error("the hook thread stopped before it had the keyboard")]
    Gone,
}

/// The keys bound now; they stay bound while this is kept.
pub(super) struct Keys {
    /// The hook thread, to post `WM_QUIT` to.
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl Keys {
    /// Installs the hook on a thread of its own and watches for `bindings`.
    fn start(
        bindings: &[HotkeyBinding],
        events: mpsc::UnboundedSender<HotkeyEvent>,
    ) -> Result<Self, HookError> {
        let bindings = bindings.to_vec();
        // The hook belongs to the thread that installs it, so whether it
        // installed has to come back from there.
        let (ready, installed) = std::sync::mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("opencord-hotkeys".to_owned())
            .spawn(move || run(&bindings, events, &ready))
            .expect("the system can start a thread");
        match installed.recv() {
            Ok(Ok(thread_id)) => Ok(Self {
                thread_id,
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => {
                let _ = thread.join();
                Err(HookError::Gone)
            }
        }
    }
}

impl Drop for Keys {
    fn drop(&mut self) {
        // `WM_QUIT` ends the message loop, which unhooks on its way out.
        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The hook thread: install, pump messages until `WM_QUIT`, unhook.
fn run(
    bindings: &[HotkeyBinding],
    events: mpsc::UnboundedSender<HotkeyEvent>,
    ready: &std::sync::mpsc::Sender<Result<u32, HookError>>,
) {
    let matcher = Matcher::new(bindings, foreground_layout());
    HOOKED.with(|hooked| *hooked.borrow_mut() = Some(Hooked { matcher, events }));
    let installed = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook), None, 0) };
    let hook = match installed {
        Ok(hook) => hook,
        Err(error) => {
            HOOKED.with(|hooked| *hooked.borrow_mut() = None);
            let _ = ready.send(Err(HookError::Install(error)));
            return;
        }
    };
    // The layout is looked at here, in the message loop, and not in the hook
    // procedure, which must not call into anything that might be slow.
    let timer = unsafe { SetTimer(None, 0, LAYOUT_CHECK.as_millis() as u32, None) };
    if ready.send(Ok(unsafe { GetCurrentThreadId() })).is_err() {
        // Nobody kept the `Keys`, so nothing is listening; unhook and go.
        unhook(hook, timer);
        return;
    }
    let mut message = MSG::default();
    // `GetMessageW` returns 0 on `WM_QUIT`, and -1 on an error.
    while unsafe { GetMessageW(&mut message, None, 0, 0) }.0 > 0 {
        if message.message == WM_TIMER {
            let layout = foreground_layout();
            HOOKED.with(|hooked| {
                if let Some(hooked) = hooked.borrow_mut().as_mut() {
                    hooked.matcher.relayout(layout);
                }
            });
        }
    }
    unhook(hook, timer);
}

fn unhook(hook: HHOOK, timer: usize) {
    if timer != 0 {
        let _ = unsafe { KillTimer(None, timer) };
    }
    let _ = unsafe { UnhookWindowsHookEx(hook) };
    HOOKED.with(|hooked| *hooked.borrow_mut() = None);
}

/// Binds `bindings` through a keyboard hook.
pub(super) async fn bind(
    _app_id: &str,
    bindings: &[HotkeyBinding],
    events: &mpsc::UnboundedSender<HotkeyEvent>,
) -> (Option<Keys>, HotkeySupport) {
    match Keys::start(bindings, events.clone()) {
        Ok(keys) => (
            Some(keys),
            HotkeySupport::Global {
                method: "a Windows keyboard hook".to_owned(),
            },
        ),
        Err(error) => (
            None,
            HotkeySupport::FocusedOnly {
                reason: format!("the keyboard cannot be watched ({error})"),
                settings_url: None,
            },
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkeys::Accelerator;

    fn matcher(bindings: &[(HotkeyAction, &str)]) -> Matcher {
        let bindings: Vec<HotkeyBinding> = bindings
            .iter()
            .map(|(action, text)| HotkeyBinding {
                action: *action,
                accelerator: text.parse::<Accelerator>().unwrap(),
            })
            .collect();
        Matcher::new(&bindings, foreground_layout())
    }

    /// The accelerator's key, in virtual-key codes.
    fn key_of(text: &str) -> Watch {
        watch(
            text.parse::<Accelerator>().unwrap().keysym,
            foreground_layout(),
        )
        .unwrap()
    }

    #[test]
    fn keysyms_name_the_virtual_keys_windows_reports() {
        assert_eq!(key_of("F1").vk, VK_F1.0);
        assert_eq!(key_of("F24").vk, VK_F1.0 + 23);
        assert_eq!(key_of("KP_0").vk, VK_NUMPAD0.0);
        assert_eq!(key_of("KP_9").vk, VK_NUMPAD0.0 + 9);
        assert_eq!(key_of("Control_R").vk, VK_RCONTROL.0);
        assert_eq!(key_of("Super_L").vk, VK_LWIN.0);
        assert_eq!(key_of("Escape").vk, VK_ESCAPE.0);
        assert_eq!(key_of("KP_Divide").vk, VK_DIVIDE.0);
        // Printable keys come from the layout. Every Latin layout types the
        // letters and digits on the keys the codes are named after.
        assert_eq!(key_of("m").vk, u16::from(b'M'));
        assert_eq!(key_of("7").vk, u16::from(b'7'));
        assert_eq!(key_of("space").vk, 0x20);
    }

    #[test]
    fn the_keypads_enter_and_return_share_a_code_and_are_told_apart() {
        let (enter, newline) = (key_of("KP_Enter"), key_of("Return"));

        assert_eq!(enter.vk, VK_RETURN.0);
        assert_eq!(newline.vk, VK_RETURN.0);
        assert_eq!(enter.extended, Some(true));
        assert_eq!(newline.extended, Some(false));

        let mut keys = matcher(&[
            (HotkeyAction::PushToTalk, "KP_Enter"),
            (HotkeyAction::ToggleMute, "Return"),
        ]);
        let from_the_keypad = keys.key(VK_RETURN.0, true, false);
        keys.key(VK_RETURN.0, true, true);
        let from_the_main_keyboard = keys.key(VK_RETURN.0, false, false);

        assert_eq!(
            from_the_keypad,
            vec![HotkeyEvent::Pressed(HotkeyAction::PushToTalk)]
        );
        assert_eq!(
            from_the_main_keyboard,
            vec![HotkeyEvent::Pressed(HotkeyAction::ToggleMute)]
        );
    }

    #[test]
    fn a_keypad_key_with_num_lock_off_is_not_the_arrow_it_reports() {
        let mut keys = matcher(&[(HotkeyAction::PushToTalk, "Left")]);

        // The keypad's 4 with Num Lock off: `VK_LEFT`, but not extended.
        let keypad = keys.key(VK_LEFT.0, false, false);
        let arrow = keys.key(VK_LEFT.0, true, false);

        assert!(keypad.is_empty(), "the keypad is not the arrow cluster");
        assert_eq!(arrow, vec![HotkeyEvent::Pressed(HotkeyAction::PushToTalk)]);
    }

    #[test]
    fn left_and_right_modifiers_are_told_apart() {
        let mut keys = matcher(&[(HotkeyAction::PushToTalk, "Control_R")]);

        let left = keys.key(VK_LCONTROL.0, false, false);
        let right = keys.key(VK_RCONTROL.0, true, false);
        let right_up = keys.key(VK_RCONTROL.0, true, true);

        assert!(left.is_empty(), "the left Ctrl is a different key");
        assert_eq!(right, vec![HotkeyEvent::Pressed(HotkeyAction::PushToTalk)]);
        assert_eq!(
            right_up,
            vec![HotkeyEvent::Released(HotkeyAction::PushToTalk)]
        );
    }

    #[test]
    fn altgr_does_not_press_a_ctrl_alt_hotkey() {
        let mut keys = matcher(&[(HotkeyAction::PushToTalk, "CTRL+ALT+m")]);
        let m = key_of("m").vk;

        // AltGr, as the hook reports it: a left Ctrl with the extended flag,
        // then the right Alt.
        let phantom = keys.key(VK_LCONTROL.0, true, false);
        keys.key(VK_RMENU.0, true, false);
        let typed = keys.key(m, false, false);

        assert!(phantom.is_empty());
        assert!(typed.is_empty(), "AltGr+m is not CTRL+ALT+m");
        assert!(!keys.holding(Modifier::Ctrl), "AltGr holds no Ctrl");

        // A real Ctrl, which is never extended, still works with it.
        keys.key(VK_LCONTROL.0, false, false);
        let with_ctrl = keys.key(m, false, false);

        assert_eq!(
            with_ctrl,
            vec![HotkeyEvent::Pressed(HotkeyAction::PushToTalk)]
        );
    }

    #[test]
    fn a_key_with_its_modifiers_presses_and_releases_its_action() {
        let mut keys = matcher(&[(HotkeyAction::PushToTalk, "CTRL+m")]);
        let m = key_of("m").vk;

        let alone = keys.key(m, false, false);
        keys.key(m, false, true);
        keys.key(VK_LCONTROL.0, false, false);
        let with_ctrl = keys.key(m, false, false);
        let repeated = keys.key(m, false, false);
        let let_go = keys.key(m, false, true);

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

        keys.key(VK_LSHIFT.0, false, false);
        let pressed = keys.key(key_of("F12").vk, false, false);

        assert_eq!(
            pressed,
            vec![HotkeyEvent::Pressed(HotkeyAction::PrioritySpeaker)]
        );
    }

    #[test]
    fn releasing_the_modifier_first_keeps_the_hotkey_until_the_key_goes() {
        let mut keys = matcher(&[(HotkeyAction::PushToTalk, "CTRL+m")]);
        let m = key_of("m").vk;
        keys.key(VK_LCONTROL.0, false, false);
        keys.key(m, false, false);

        let modifier_up = keys.key(VK_LCONTROL.0, false, true);
        let key_up = keys.key(m, false, true);

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
        let (m, f12) = (key_of("m").vk, key_of("F12").vk);
        keys.key(VK_LCONTROL.0, false, false);

        let first = keys.key(m, false, false);
        let second = keys.key(f12, false, false);
        let one_up = keys.key(m, false, true);
        let last_up = keys.key(f12, false, true);

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

        let down = keys.key(VK_LCONTROL.0, false, false);
        let up = keys.key(VK_LCONTROL.0, false, true);

        assert_eq!(down, vec![HotkeyEvent::Pressed(HotkeyAction::PushToTalk)]);
        assert_eq!(up, vec![HotkeyEvent::Released(HotkeyAction::PushToTalk)]);
    }

    #[test]
    fn a_key_this_keyboard_cannot_press_matches_nothing() {
        let unknown = Watched {
            action: HotkeyAction::PushToTalk,
            key: None,
            modifiers: Vec::new(),
        };
        let mut keys = matcher(&[]);
        keys.watched.push(unknown);

        assert!(keys.key(key_of("m").vk, false, false).is_empty());
        assert!(keys.key(0, false, false).is_empty());
    }

    /// Types `keys` (virtual key, extended?, up?) with `SendInput` while a
    /// hook watches for `bindings`, and returns the hotkey events that came.
    ///
    /// The keys really are pressed, on whatever has the focus, which is why
    /// the tests that use this are ignored: run them deliberately. One at a
    /// time, too — every hook in the session sees injected keys, so two of
    /// these at once would read each other's.
    fn typed(
        bindings: &[(HotkeyAction, &str)],
        keys: &[(VIRTUAL_KEY, bool, bool)],
        expected: usize,
    ) -> Vec<HotkeyEvent> {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
            KEYEVENTF_KEYUP, SendInput,
        };

        static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _typist = ONE_AT_A_TIME
            .lock()
            .unwrap_or_else(|held| held.into_inner());

        let (sender, mut received) = mpsc::unbounded_channel();
        let bindings: Vec<HotkeyBinding> = bindings
            .iter()
            .map(|(action, text)| HotkeyBinding {
                action: *action,
                accelerator: text.parse().unwrap(),
            })
            .collect();
        let bound = Keys::start(&bindings, sender).unwrap();

        let typing: Vec<INPUT> = keys
            .iter()
            .map(|(vk, extended, up)| {
                let mut flags = KEYBD_EVENT_FLAGS(0);
                if *extended {
                    flags |= KEYEVENTF_EXTENDEDKEY;
                }
                if *up {
                    flags |= KEYEVENTF_KEYUP;
                }
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: *vk,
                            dwFlags: flags,
                            ..Default::default()
                        },
                    },
                }
            })
            .collect();
        let sent = unsafe { SendInput(&typing, size_of::<INPUT>() as i32) };
        assert_eq!(sent as usize, typing.len(), "the keys were injected");

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
        drop(bound);
        events
    }

    /// Presses real keys; run with `--ignored --test-threads=1`.
    #[test]
    #[ignore = "presses real keys on the whole desktop"]
    fn keys_typed_anywhere_press_and_release_a_hotkey() {
        let f13 = VIRTUAL_KEY(VK_F1.0 + 12);

        let events = typed(
            &[(HotkeyAction::PushToTalk, "F13")],
            &[(f13, false, false), (f13, false, true)],
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

    /// Presses real keys; run with `--ignored --test-threads=1`.
    #[test]
    #[ignore = "presses real keys on the whole desktop"]
    fn push_to_talk_on_two_typed_keys_holds_until_both_are_up() {
        let (f13, f14) = (VIRTUAL_KEY(VK_F1.0 + 12), VIRTUAL_KEY(VK_F1.0 + 13));

        let events = typed(
            &[
                (HotkeyAction::PushToTalk, "F13"),
                (HotkeyAction::PushToTalk, "F14"),
            ],
            &[
                (f13, false, false),
                (f14, false, false),
                (f13, false, true),
                (f14, false, true),
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

    /// Presses real keys; run with `--ignored --test-threads=1`.
    #[test]
    #[ignore = "presses real keys on the whole desktop"]
    fn the_right_ctrl_alone_is_a_hotkey_wherever_it_is_typed() {
        let events = typed(
            &[(HotkeyAction::PushToTalk, "Control_R")],
            &[(VK_RCONTROL, true, false), (VK_RCONTROL, true, true)],
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

    /// Print Screen reaches the hook, even though a focused window is only
    /// sent its key-up and the app's own key recorder therefore cannot record
    /// it (docs/plans/windows.md W2, step 6). So `Print` is a usable keybind;
    /// it just has to be typed in rather than recorded.
    ///
    /// Presses real keys; run with `--ignored --test-threads=1`. This one
    /// also puts a screenshot on the clipboard, which is what Print Screen
    /// does.
    #[test]
    #[ignore = "presses real keys on the whole desktop"]
    fn print_screen_reaches_the_hook() {
        let events = typed(
            &[(HotkeyAction::PushToTalk, "Print")],
            &[(VK_SNAPSHOT, false, false), (VK_SNAPSHOT, false, true)],
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

    /// Presses real keys; run with `--ignored --test-threads=1`.
    #[test]
    #[ignore = "presses real keys on the whole desktop"]
    fn a_toggle_fires_once_for_each_press() {
        let f15 = VIRTUAL_KEY(VK_F1.0 + 14);

        let events = typed(
            &[(HotkeyAction::ToggleMute, "F15")],
            &[
                (f15, false, false),
                (f15, false, true),
                (f15, false, false),
                (f15, false, true),
            ],
            4,
        );

        assert_eq!(
            events,
            vec![
                HotkeyEvent::Pressed(HotkeyAction::ToggleMute),
                HotkeyEvent::Released(HotkeyAction::ToggleMute),
                HotkeyEvent::Pressed(HotkeyAction::ToggleMute),
                HotkeyEvent::Released(HotkeyAction::ToggleMute),
            ]
        );
    }
}
