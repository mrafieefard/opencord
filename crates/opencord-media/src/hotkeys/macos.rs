//! macOS's global hotkeys (docs/plans/macos.md M2): a listen-only event tap
//! sees every key press wherever the focus is, without taking keys away from
//! other apps. It needs Input Monitoring; until the user allows it, hotkeys
//! work only while Opencord is focused, and the app binds again when its
//! window is focused, so a later grant takes effect.
//!
//! The contract, as `linux.rs` keeps it: `bind` binds `bindings`
//! system-wide, replacing nothing (the caller drops the old `Keys` first), and
//! sends each press and release to `events` through [`super::Held`], so an
//! action with several keys presses when the first goes down and releases
//! when the last comes up. The keys stay bound while the returned `Keys` is
//! kept and unbind when it is dropped. Accelerators name keys as xkb keysyms
//! (`CTRL+SHIFT+m`, `Control_R`, `F13`, `KP_5`): printable keys are found
//! on the current keyboard layout, the others in a fixed table of macOS
//! virtual keycodes. CTRL is Control, ALT is Option, LOGO is Command.
//!
//! While any app has Secure Event Input on (a password field is focused),
//! no event tap gets keys, so hotkeys don't work then.

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Once};
use std::thread::JoinHandle;
use std::time::Duration;

use dispatch2::DispatchQueue;
use objc2_core_foundation::{
    CFData, CFDictionary, CFMachPort, CFNotificationCenter, CFNotificationName,
    CFNotificationSuspensionBehavior, CFRetained, CFRunLoop, CFString, CFType,
    kCFRunLoopCommonModes, kCFRunLoopDefaultMode,
};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventTapProxy, CGEventType, CGPreflightListenEventAccess, CGRequestListenEventAccess,
};
use tokio::sync::{mpsc, oneshot};

use super::{Held, HotkeyAction, HotkeyBinding, HotkeyEvent, HotkeySupport};

/// What the Keybinds page says while Input Monitoring is missing.
const NOT_ALLOWED: &str =
    "allow Opencord under System Settings → Privacy & Security → Input Monitoring";
/// The Input Monitoring pane of System Settings.
const SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent";

/// The keys that are the same whatever the layout: (keysym, macOS virtual
/// keycode, `kVK_*` in HIToolbox's `Events.h`). Help sits where Insert is,
/// and Clear where Num Lock is. Print, Scroll Lock, Pause and Menu have no
/// Mac key.
const FIXED_KEYS: &[(u32, u16)] = &[
    (0x20, 0x31),   // space
    (0xff09, 0x30), // Tab
    (0xff0d, 0x24), // Return
    (0xff08, 0x33), // BackSpace: kVK_Delete
    (0xff1b, 0x35), // Escape
    (0xffff, 0x75), // Delete: kVK_ForwardDelete
    (0xff63, 0x72), // Insert: kVK_Help
    (0xff50, 0x73), // Home
    (0xff57, 0x77), // End
    (0xff55, 0x74), // Prior: kVK_PageUp
    (0xff56, 0x79), // Next: kVK_PageDown
    (0xff51, 0x7b), // Left
    (0xff53, 0x7c), // Right
    (0xff54, 0x7d), // Down
    (0xff52, 0x7e), // Up
    (0xffbe, 0x7a), // F1
    (0xffbf, 0x78), // F2
    (0xffc0, 0x63), // F3
    (0xffc1, 0x76), // F4
    (0xffc2, 0x60), // F5
    (0xffc3, 0x61), // F6
    (0xffc4, 0x62), // F7
    (0xffc5, 0x64), // F8
    (0xffc6, 0x65), // F9
    (0xffc7, 0x6d), // F10
    (0xffc8, 0x67), // F11
    (0xffc9, 0x6f), // F12
    (0xffca, 0x69), // F13
    (0xffcb, 0x6b), // F14
    (0xffcc, 0x71), // F15
    (0xffcd, 0x6a), // F16
    (0xffce, 0x40), // F17
    (0xffcf, 0x4f), // F18
    (0xffd0, 0x50), // F19
    (0xffd1, 0x5a), // F20
    (0xffb0, 0x52), // KP_0
    (0xffb1, 0x53), // KP_1
    (0xffb2, 0x54), // KP_2
    (0xffb3, 0x55), // KP_3
    (0xffb4, 0x56), // KP_4
    (0xffb5, 0x57), // KP_5
    (0xffb6, 0x58), // KP_6
    (0xffb7, 0x59), // KP_7
    (0xffb8, 0x5b), // KP_8
    (0xffb9, 0x5c), // KP_9
    (0xff8d, 0x4c), // KP_Enter
    (0xffaa, 0x43), // KP_Multiply
    (0xffab, 0x45), // KP_Add
    (0xffad, 0x4e), // KP_Subtract
    (0xffae, 0x41), // KP_Decimal
    (0xffaf, 0x4b), // KP_Divide
    (0xff7f, 0x47), // Num_Lock: kVK_ANSI_KeypadClear
    (0xffe1, 0x38), // Shift_L
    (0xffe2, 0x3c), // Shift_R
    (0xffe3, 0x3b), // Control_L
    (0xffe4, 0x3e), // Control_R
    (0xffe5, 0x39), // Caps_Lock
    (0xffe9, 0x3a), // Alt_L: Option
    (0xffea, 0x3d), // Alt_R
    (0xffeb, 0x37), // Super_L: Command
    (0xffec, 0x36), // Super_R
];

/// The character keys of a US (ANSI) keyboard, `kVK_ANSI_*`: the layout
/// when the current one has no key layout data.
const ANSI_KEYS: &[(u8, u16)] = &[
    (b'a', 0x00),
    (b's', 0x01),
    (b'd', 0x02),
    (b'f', 0x03),
    (b'h', 0x04),
    (b'g', 0x05),
    (b'z', 0x06),
    (b'x', 0x07),
    (b'c', 0x08),
    (b'v', 0x09),
    (b'b', 0x0b),
    (b'q', 0x0c),
    (b'w', 0x0d),
    (b'e', 0x0e),
    (b'r', 0x0f),
    (b'y', 0x10),
    (b't', 0x11),
    (b'1', 0x12),
    (b'2', 0x13),
    (b'3', 0x14),
    (b'4', 0x15),
    (b'6', 0x16),
    (b'5', 0x17),
    (b'=', 0x18),
    (b'9', 0x19),
    (b'7', 0x1a),
    (b'-', 0x1b),
    (b'8', 0x1c),
    (b'0', 0x1d),
    (b']', 0x1e),
    (b'o', 0x1f),
    (b'u', 0x20),
    (b'[', 0x21),
    (b'i', 0x22),
    (b'p', 0x23),
    (b'l', 0x25),
    (b'j', 0x26),
    (b'\'', 0x27),
    (b'k', 0x28),
    (b';', 0x29),
    (b'\\', 0x2a),
    (b',', 0x2b),
    (b'/', 0x2c),
    (b'n', 0x2d),
    (b'm', 0x2e),
    (b'.', 0x2f),
    (b'`', 0x32),
];

/// The keycodes whose character depends on the layout: the ANSI character
/// keys, ISO's extra key (`kVK_ISO_Section`) and JIS's two
/// (`kVK_JIS_Yen`, `kVK_JIS_Underscore`).
fn character_keycodes() -> impl Iterator<Item = u16> {
    ANSI_KEYS
        .iter()
        .map(|(_, keycode)| *keycode)
        .chain([0x0a, 0x5d, 0x5e])
}

// Modifier flags: the device-independent ones say a modifier is held; the
// device-dependent ones (IOKit's `NX_DEVICE*KEYMASK`) say which side.
const CONTROL: u64 = 0x4_0000;
const SHIFT: u64 = 0x2_0000;
const OPTION: u64 = 0x8_0000;
const COMMAND: u64 = 0x10_0000;
const CAPS_LOCK: u64 = 0x1_0000;
const LEFT_CONTROL: u64 = 0x0001;
const LEFT_SHIFT: u64 = 0x0002;
const RIGHT_SHIFT: u64 = 0x0004;
const LEFT_COMMAND: u64 = 0x0008;
const RIGHT_COMMAND: u64 = 0x0010;
const LEFT_OPTION: u64 = 0x0020;
const RIGHT_OPTION: u64 = 0x0040;
const RIGHT_CONTROL: u64 = 0x2000;
const EITHER_SIDE: u64 = LEFT_CONTROL
    | LEFT_SHIFT
    | RIGHT_SHIFT
    | LEFT_COMMAND
    | RIGHT_COMMAND
    | LEFT_OPTION
    | RIGHT_OPTION
    | RIGHT_CONTROL;

/// The modifier keys: (keycode, its side's flag, its modifier's flag).
/// Caps Lock has no side; its flag is its lock state.
const MODIFIER_KEYS: [(u16, u64, u64); 9] = [
    (0x3b, LEFT_CONTROL, CONTROL),
    (0x3e, RIGHT_CONTROL, CONTROL),
    (0x38, LEFT_SHIFT, SHIFT),
    (0x3c, RIGHT_SHIFT, SHIFT),
    (0x3a, LEFT_OPTION, OPTION),
    (0x3d, RIGHT_OPTION, OPTION),
    (0x37, LEFT_COMMAND, COMMAND),
    (0x36, RIGHT_COMMAND, COMMAND),
    (0x39, 0, CAPS_LOCK),
];

/// Whether the modifier key `keycode` is down after a `flagsChanged`
/// event with `flags`, or `None` when it isn't a modifier key. Events
/// posted by a program may carry no side flags; then the modifier's own
/// flag decides.
fn modifier_down(keycode: u16, flags: u64) -> Option<bool> {
    let (_, side, modifier) = MODIFIER_KEYS.iter().find(|(code, _, _)| *code == keycode)?;
    Some(if *side != 0 && flags & EITHER_SIDE != 0 {
        flags & side != 0
    } else {
        flags & modifier != 0
    })
}

/// The modifiers held, or wanted by a binding.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Modifiers {
    ctrl: bool,
    alt: bool,
    shift: bool,
    logo: bool,
}

impl Modifiers {
    fn of(flags: u64) -> Self {
        Self {
            ctrl: flags & CONTROL != 0,
            alt: flags & OPTION != 0,
            shift: flags & SHIFT != 0,
            logo: flags & COMMAND != 0,
        }
    }

    /// Whether every one of `wanted` is held; extra ones don't matter.
    fn cover(self, wanted: Self) -> bool {
        (self.ctrl || !wanted.ctrl)
            && (self.alt || !wanted.alt)
            && (self.shift || !wanted.shift)
            && (self.logo || !wanted.logo)
    }
}

/// Which keycodes type each printable keysym on a keyboard layout.
struct Layout {
    printable: HashMap<u32, Vec<u16>>,
}

impl Layout {
    /// A US keyboard.
    fn ansi() -> Self {
        let mut printable: HashMap<u32, Vec<u16>> = HashMap::new();
        for (character, keycode) in ANSI_KEYS {
            printable
                .entry(u32::from(*character))
                .or_default()
                .push(*keycode);
        }
        Self { printable }
    }

    /// The layout in use now: each character key's character, unshifted
    /// and shifted (a digit is shifted on AZERTY), as X lists a keycode's
    /// keysyms. A US keyboard if the layout has no key layout data.
    fn current() -> Self {
        let Some(layout) = KeyLayout::current() else {
            return Self::ansi();
        };
        let mut printable: HashMap<u32, Vec<u16>> = HashMap::new();
        for keycode in character_keycodes() {
            for shifted in [false, true] {
                // The keysym of an ASCII character is its code.
                if let Some(character) = layout.character(keycode, shifted)
                    && (0x20..0x7f).contains(&character)
                {
                    let keycodes = printable.entry(character).or_default();
                    if !keycodes.contains(&keycode) {
                        keycodes.push(keycode);
                    }
                }
            }
        }
        if printable.is_empty() {
            return Self::ansi();
        }
        Self { printable }
    }

    /// The keycodes of `keysym`'s key.
    fn keycodes(&self, keysym: u32) -> Vec<u16> {
        let fixed: Vec<u16> = FIXED_KEYS
            .iter()
            .filter(|(sym, _)| *sym == keysym)
            .map(|(_, keycode)| *keycode)
            .collect();
        if !fixed.is_empty() {
            return fixed;
        }
        self.printable.get(&keysym).cloned().unwrap_or_default()
    }
}

/// One binding, in keycodes.
struct Watched {
    action: HotkeyAction,
    keys: Vec<u16>,
    modifiers: Modifiers,
}

/// Matches key presses and releases against the bindings.
struct Matcher {
    watched: Vec<Watched>,
    /// Bindings down, by index.
    down: HashSet<usize>,
    actions: Held,
}

impl Matcher {
    fn new(bindings: &[HotkeyBinding], layout: &Layout) -> Self {
        let watched = bindings
            .iter()
            .map(|binding| {
                let accelerator = &binding.accelerator;
                Watched {
                    action: binding.action,
                    keys: layout.keycodes(accelerator.keysym),
                    modifiers: Modifiers {
                        ctrl: accelerator.ctrl,
                        alt: accelerator.alt,
                        shift: accelerator.shift,
                        logo: accelerator.logo,
                    },
                }
            })
            .collect();
        Self {
            watched,
            down: HashSet::new(),
            actions: Held::default(),
        }
    }

    /// An event from the tap: a key down or up, or a modifier key's change.
    fn event(&mut self, kind: CGEventType, keycode: u16, flags: u64) -> Vec<HotkeyEvent> {
        let down = match kind {
            CGEventType::KeyDown => true,
            CGEventType::KeyUp => false,
            CGEventType::FlagsChanged => match modifier_down(keycode, flags) {
                Some(down) => down,
                None => return Vec::new(),
            },
            _ => return Vec::new(),
        };
        if down {
            self.press(keycode, Modifiers::of(flags))
        } else {
            self.release(keycode)
        }
    }

    fn press(&mut self, keycode: u16, held: Modifiers) -> Vec<HotkeyEvent> {
        let mut events = Vec::new();
        for (index, watched) in self.watched.iter().enumerate() {
            // A held key repeats: only its first press counts.
            if watched.keys.contains(&keycode)
                && held.cover(watched.modifiers)
                && self.down.insert(index)
            {
                events.extend(self.actions.press(index, watched.action));
            }
        }
        events
    }

    fn release(&mut self, keycode: u16) -> Vec<HotkeyEvent> {
        let mut events = Vec::new();
        for (index, watched) in self.watched.iter().enumerate() {
            if watched.keys.contains(&keycode) && self.down.remove(&index) {
                events.extend(self.actions.release(index, watched.action));
            }
        }
        events
    }

    /// Releases every binding that is down, before the keys change.
    fn release_all(&mut self) -> Vec<HotkeyEvent> {
        let mut events = Vec::new();
        for index in std::mem::take(&mut self.down) {
            events.extend(self.actions.release(index, self.watched[index].action));
        }
        events
    }
}

// HIToolbox's keyboard layouts, declared by hand (decision A).
#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    static kTISPropertyUnicodeKeyLayoutData: &'static CFString;
    static kTISNotifySelectedKeyboardInputSourceChanged: &'static CFString;
    /// Returns a retained `TISInputSourceRef`, or null.
    fn TISCopyCurrentKeyboardLayoutInputSource() -> *mut CFType;
    /// Returns an unretained property of the input source, or null.
    fn TISGetInputSourceProperty(source: *const CFType, key: &CFString) -> *const CFType;
    fn LMGetKbdType() -> u8;
    fn UCKeyTranslate(
        layout: *const c_void,
        virtual_key_code: u16,
        key_action: u16,
        modifier_key_state: u32,
        keyboard_type: u32,
        key_translate_options: u32,
        dead_key_state: *mut u32,
        max_string_length: usize,
        actual_string_length: *mut usize,
        unicode_string: *mut u16,
    ) -> i32;
}

/// `kUCKeyActionDisplay`: the character a key shows, not one it types.
const KEY_ACTION_DISPLAY: u16 = 3;
/// `kUCKeyTranslateNoDeadKeysMask`.
const NO_DEAD_KEYS: u32 = 1;
/// `shiftKey >> 8`, as `UCKeyTranslate` takes modifiers.
const SHIFT_KEY_STATE: u32 = 0x02;

/// The current keyboard layout's key layout data.
struct KeyLayout {
    /// Keeps the data alive.
    _source: CFRetained<CFType>,
    data: NonNull<CFData>,
}

impl KeyLayout {
    fn current() -> Option<Self> {
        // SAFETY: the copy is ours to release, which `CFRetained` does; the
        // property belongs to the source, which `_source` keeps.
        unsafe {
            let source =
                CFRetained::from_raw(NonNull::new(TISCopyCurrentKeyboardLayoutInputSource())?);
            let data =
                TISGetInputSourceProperty(&*source, kTISPropertyUnicodeKeyLayoutData).cast_mut();
            Some(Self {
                data: NonNull::new(data)?.cast(),
                _source: source,
            })
        }
    }

    /// The character `keycode` shows, if it is one character.
    fn character(&self, keycode: u16, shifted: bool) -> Option<u32> {
        // SAFETY: `data` is alive while `_source` is.
        let layout = unsafe { self.data.as_ref() }.byte_ptr();
        let mut dead_keys = 0;
        let mut length = 0;
        let mut text = [0u16; 4];
        // SAFETY: the buffers are as long as said.
        let status = unsafe {
            UCKeyTranslate(
                layout.cast(),
                keycode,
                KEY_ACTION_DISPLAY,
                if shifted { SHIFT_KEY_STATE } else { 0 },
                u32::from(LMGetKbdType()),
                NO_DEAD_KEYS,
                &mut dead_keys,
                text.len(),
                &mut length,
                text.as_mut_ptr(),
            )
        };
        (status == 0 && length == 1).then(|| u32::from(text[0]))
    }
}

unsafe extern "C" {
    fn pthread_main_np() -> i32;
}

/// How long `bind` waits for the main thread to read the layout. A process
/// whose main thread doesn't run its dispatch queue (a test) gets a US
/// keyboard.
const LAYOUT_WAIT: Duration = Duration::from_secs(2);

/// Reads the current layout and hands it to `done`, on the main thread: in
/// an app, HIToolbox stops the process when its input sources are read on
/// any other thread.
fn read_layout(done: impl FnOnce(Layout) + Send + 'static) {
    // SAFETY: no arguments; it only asks.
    if unsafe { pthread_main_np() } != 0 {
        done(Layout::current());
    } else {
        DispatchQueue::main().exec_async(move || done(Layout::current()));
    }
}

/// A layout the main thread read after the user picked another one, for
/// the tap's next event.
type NextLayout = Mutex<Option<Layout>>;

/// What the tap's callback works with, on the tap's thread.
struct Tap {
    matcher: Matcher,
    bindings: Vec<HotkeyBinding>,
    events: mpsc::UnboundedSender<HotkeyEvent>,
    /// The tap itself, to enable it again when the system turns it off.
    port: Option<CFRetained<CFMachPort>>,
    next_layout: Arc<NextLayout>,
}

/// The event tap's callback. Listen-only: the event goes on unchanged.
unsafe extern "C-unwind" fn on_event(
    _proxy: CGEventTapProxy,
    kind: CGEventType,
    event: NonNull<CGEvent>,
    context: *mut c_void,
) -> *mut CGEvent {
    // SAFETY: `context` is the thread's `Tap`, which outlives the tap, and
    // only this thread's run loop calls here.
    let tap = unsafe { &mut *context.cast::<Tap>() };
    if kind == CGEventType::TapDisabledByTimeout || kind == CGEventType::TapDisabledByUserInput {
        if let Some(port) = &tap.port {
            CGEvent::tap_enable(port, true);
        }
        return event.as_ptr();
    }
    let mut found = Vec::new();
    let next_layout = tap.next_layout.lock().ok().and_then(|mut next| next.take());
    if let Some(layout) = next_layout {
        found.extend(tap.matcher.release_all());
        tap.matcher = Matcher::new(&tap.bindings, &layout);
    }
    // SAFETY: the event is valid for the call.
    let event_ref = unsafe { event.as_ref() };
    let keycode = CGEvent::integer_value_field(Some(event_ref), CGEventField::KeyboardEventKeycode);
    let flags = CGEvent::flags(Some(event_ref)).0;
    found.extend(tap.matcher.event(kind, keycode as u16, flags));
    for found in found {
        let _ = tap.events.send(found);
    }
    event.as_ptr()
}

/// The keyboard layout changed: the main thread reads it, and the tap's
/// next event rebuilds its keys.
unsafe extern "C-unwind" fn on_layout_changed(
    _center: *mut CFNotificationCenter,
    observer: *mut c_void,
    _name: *const CFNotificationName,
    _object: *const c_void,
    _info: *const CFDictionary,
) {
    let observer = observer.cast_const().cast::<NextLayout>();
    // SAFETY: the observer is the tap's `next_layout`, which outlives the
    // observation; this takes one more reference to it.
    let next = unsafe {
        Arc::increment_strong_count(observer);
        Arc::from_raw(observer)
    };
    read_layout(move |layout| {
        if let Ok(mut next) = next.lock() {
            *next = Some(layout);
        }
    });
}

/// The tap thread's run loop, to stop it from elsewhere.
struct RunLoop(CFRetained<CFRunLoop>);

// SAFETY: only `CFRunLoopStop` and `CFRunLoopWakeUp` are called on it from
// other threads, and both may be.
unsafe impl Send for RunLoop {}
unsafe impl Sync for RunLoop {}

/// The keys bound now; they stay bound while this is kept.
pub(super) struct Keys {
    stop: Arc<AtomicBool>,
    run_loop: RunLoop,
    thread: Option<JoinHandle<()>>,
}

impl Keys {
    /// Taps the session's keyboard events for `bindings`, on a thread of
    /// its own.
    async fn start(
        bindings: &[HotkeyBinding],
        events: mpsc::UnboundedSender<HotkeyEvent>,
    ) -> Result<Self, String> {
        let (read, reading) = oneshot::channel();
        read_layout(move |layout| {
            let _ = read.send(layout);
        });
        let layout = match tokio::time::timeout(LAYOUT_WAIT, reading).await {
            Ok(Ok(layout)) => layout,
            _ => Layout::ansi(),
        };
        let (started, starting) = oneshot::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        let bindings = bindings.to_vec();
        let thread = std::thread::Builder::new()
            .name("opencord-hotkeys".to_owned())
            .spawn(move || run_tap(bindings, &layout, events, &stopped, started))
            .expect("the system can start a thread");
        match starting.await {
            Ok(Ok(run_loop)) => Ok(Self {
                stop,
                run_loop,
                thread: Some(thread),
            }),
            Ok(Err(reason)) => {
                let _ = thread.join();
                Err(reason)
            }
            Err(_) => Err("the hotkey thread stopped".to_owned()),
        }
    }
}

impl Drop for Keys {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.run_loop.0.stop();
        self.run_loop.0.wake_up();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The tap thread: taps, says so on `started`, and runs until `stop`.
fn run_tap(
    bindings: Vec<HotkeyBinding>,
    layout: &Layout,
    events: mpsc::UnboundedSender<HotkeyEvent>,
    stop: &AtomicBool,
    started: oneshot::Sender<Result<RunLoop, String>>,
) {
    let next_layout = Arc::new(NextLayout::default());
    let tap = Box::into_raw(Box::new(Tap {
        matcher: Matcher::new(&bindings, layout),
        bindings,
        events,
        port: None,
        next_layout: Arc::clone(&next_layout),
    }));
    let keys = [
        CGEventType::KeyDown,
        CGEventType::KeyUp,
        CGEventType::FlagsChanged,
    ]
    .into_iter()
    .fold(0u64, |mask, kind| mask | 1 << kind.0);
    // SAFETY: `on_event` takes the `Tap` that `tap` points to, which lives
    // until the tap is invalidated below.
    let port = unsafe {
        CGEvent::tap_create(
            CGEventTapLocation::SessionEventTap,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::ListenOnly,
            keys,
            Some(on_event),
            tap.cast(),
        )
    };
    let Some(port) = port else {
        // SAFETY: nothing else has `tap`.
        drop(unsafe { Box::from_raw(tap) });
        let _ = started.send(Err(
            "macOS refused to watch the keyboard (is Input Monitoring allowed?)".to_owned(),
        ));
        return;
    };
    let (Some(source), Some(run_loop)) = (
        CFMachPort::new_run_loop_source(None, Some(&port), 0),
        CFRunLoop::current(),
    ) else {
        port.invalidate();
        // SAFETY: the tap is gone.
        drop(unsafe { Box::from_raw(tap) });
        let _ = started.send(Err("the hotkey thread has no run loop".to_owned()));
        return;
    };
    // SAFETY: the run loop isn't running yet, so the callback can't be.
    unsafe { (*tap).port = Some(port.clone()) };
    // SAFETY: the modes are Core Foundation's constants.
    run_loop.add_source(Some(&source), unsafe { kCFRunLoopCommonModes });
    let center = CFNotificationCenter::distributed_center();
    let observer = Arc::as_ptr(&next_layout).cast::<c_void>();
    if let Some(center) = &center {
        // SAFETY: `observer` stays alive until it is removed below.
        unsafe {
            center.add_observer(
                observer,
                Some(on_layout_changed),
                Some(kTISNotifySelectedKeyboardInputSourceChanged),
                ptr::null(),
                CFNotificationSuspensionBehavior::DeliverImmediately,
            );
        }
    }
    if started.send(Ok(RunLoop(run_loop.clone()))).is_ok() {
        while !stop.load(Ordering::Relaxed) {
            // Checks `stop` at least each second, in case it was set before
            // the run loop started and `CFRunLoopStop` was missed.
            CFRunLoop::run_in_mode(unsafe { kCFRunLoopDefaultMode }, 1.0, false);
        }
    }
    if let Some(center) = &center {
        // SAFETY: removes the observer added above.
        unsafe { center.remove_observer(observer, None, ptr::null()) };
    }
    CGEvent::tap_enable(&port, false);
    port.invalidate();
    source.invalidate();
    // SAFETY: the tap is gone, so nothing calls back with `tap` any more.
    drop(unsafe { Box::from_raw(tap) });
}

/// Binds `bindings` through an event tap, once Input Monitoring is
/// allowed.
pub(super) async fn bind(
    _app_id: &str,
    bindings: &[HotkeyBinding],
    events: &mpsc::UnboundedSender<HotkeyEvent>,
) -> (Option<Keys>, HotkeySupport) {
    let allowed = CGPreflightListenEventAccess();
    if bindings.is_empty() {
        // Nothing to watch: no tap, and no prompt until there is a key.
        let support = if allowed {
            HotkeySupport::Global {
                method: "Input Monitoring".to_owned(),
            }
        } else {
            HotkeySupport::FocusedOnly {
                reason: NOT_ALLOWED.to_owned(),
                settings_url: Some(SETTINGS_URL.to_owned()),
            }
        };
        return (None, support);
    }
    if !allowed {
        // The system's prompt, once per run; after that, the settings pane.
        static ASKED: Once = Once::new();
        ASKED.call_once(|| {
            CGRequestListenEventAccess();
        });
        return (
            None,
            HotkeySupport::FocusedOnly {
                reason: NOT_ALLOWED.to_owned(),
                settings_url: Some(SETTINGS_URL.to_owned()),
            },
        );
    }
    match Keys::start(bindings, events.clone()).await {
        Ok(keys) => (
            Some(keys),
            HotkeySupport::Global {
                method: "Input Monitoring".to_owned(),
            },
        ),
        Err(reason) => (
            None,
            HotkeySupport::FocusedOnly {
                reason,
                settings_url: Some(SETTINGS_URL.to_owned()),
            },
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkeys::Accelerator;

    fn bindings(bindings: &[(HotkeyAction, &str)]) -> Vec<HotkeyBinding> {
        bindings
            .iter()
            .map(|(action, text)| HotkeyBinding {
                action: *action,
                accelerator: text.parse::<Accelerator>().unwrap(),
            })
            .collect()
    }

    /// A US keyboard, so the tests don't depend on the Mac's layout.
    fn matcher(watched: &[(HotkeyAction, &str)]) -> Matcher {
        Matcher::new(&bindings(watched), &Layout::ansi())
    }

    const M: u16 = 0x2e;
    const F12: u16 = 0x6f;
    const LEFT_CTRL: u16 = 0x3b;
    const RIGHT_CTRL: u16 = 0x3e;

    fn key(matcher: &mut Matcher, keycode: u16, down: bool, flags: u64) -> Vec<HotkeyEvent> {
        let kind = if down {
            CGEventType::KeyDown
        } else {
            CGEventType::KeyUp
        };
        matcher.event(kind, keycode, flags)
    }

    fn flags_changed(matcher: &mut Matcher, keycode: u16, flags: u64) -> Vec<HotkeyEvent> {
        matcher.event(CGEventType::FlagsChanged, keycode, flags)
    }

    #[test]
    fn keysyms_find_their_mac_keys() {
        let layout = Layout::ansi();
        let keycode = |name: &str| layout.keycodes(name.parse::<Accelerator>().unwrap().keysym);

        assert_eq!(keycode("Control_R"), [0x3e]);
        assert_eq!(keycode("Control_L"), [0x3b]);
        assert_eq!(keycode("Super_L"), [0x37], "LOGO is Command");
        assert_eq!(keycode("Alt_R"), [0x3d], "ALT is Option");
        assert_eq!(keycode("F1"), [0x7a]);
        assert_eq!(keycode("F13"), [0x69]);
        assert_eq!(keycode("F20"), [0x5a]);
        assert_eq!(keycode("KP_5"), [0x57]);
        assert_eq!(keycode("KP_Enter"), [0x4c]);
        assert_eq!(keycode("Left"), [0x7b]);
        assert_eq!(keycode("space"), [0x31]);
        assert_eq!(keycode("m"), [M]);
        assert_eq!(keycode("grave"), [0x32]);
        assert_eq!(keycode("5"), [0x17], "the digit row, not the keypad");
        assert_eq!(keycode("F24"), Vec::<u16>::new(), "no Mac key");
        assert_eq!(keycode("Print"), Vec::<u16>::new(), "no Mac key");
    }

    #[test]
    fn every_fixed_key_is_one_the_accelerators_know() {
        for (keysym, keycode) in FIXED_KEYS {
            let others: Vec<_> = FIXED_KEYS
                .iter()
                .filter(|(sym, code)| sym != keysym && code == keycode)
                .collect();
            assert!(others.is_empty(), "keycode {keycode:#x} twice: {others:?}");
        }
        for (keycode, _, _) in MODIFIER_KEYS {
            assert!(FIXED_KEYS.iter().any(|(_, code)| *code == keycode));
        }
    }

    #[test]
    fn the_current_layout_has_the_letters() {
        let layout = Layout::current();

        for letter in b'a'..=b'z' {
            assert_eq!(
                layout.keycodes(u32::from(letter)).len(),
                1,
                "{}",
                letter as char
            );
        }
    }

    #[test]
    fn modifier_flags_tell_left_from_right() {
        let both_controls = CONTROL | LEFT_CONTROL | RIGHT_CONTROL;

        assert_eq!(
            modifier_down(RIGHT_CTRL, CONTROL | RIGHT_CONTROL),
            Some(true)
        );
        assert_eq!(
            modifier_down(LEFT_CTRL, CONTROL | RIGHT_CONTROL),
            Some(false)
        );
        assert_eq!(modifier_down(LEFT_CTRL, both_controls), Some(true));
        assert_eq!(modifier_down(RIGHT_CTRL, 0), Some(false));
        assert_eq!(
            modifier_down(RIGHT_CTRL, CONTROL),
            Some(true),
            "a posted event without side flags"
        );
        assert_eq!(modifier_down(0x36, COMMAND | RIGHT_COMMAND), Some(true));
        assert_eq!(modifier_down(0x39, CAPS_LOCK), Some(true));
        assert_eq!(modifier_down(M, CONTROL), None, "not a modifier key");
        assert_eq!(
            Modifiers::of(CONTROL | OPTION | LEFT_CONTROL),
            Modifiers {
                ctrl: true,
                alt: true,
                shift: false,
                logo: false
            }
        );
    }

    #[test]
    fn a_key_with_its_modifiers_presses_and_releases_its_action() {
        let mut keys = matcher(&[(HotkeyAction::PushToTalk, "CTRL+m")]);
        let ctrl = CONTROL | LEFT_CONTROL;

        let alone = key(&mut keys, M, true, 0);
        key(&mut keys, M, false, 0);
        flags_changed(&mut keys, LEFT_CTRL, ctrl);
        let with_ctrl = key(&mut keys, M, true, ctrl);
        let repeated = key(&mut keys, M, true, ctrl);
        let let_go = key(&mut keys, M, false, ctrl);

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

        let pressed = key(&mut keys, F12, true, SHIFT | LEFT_SHIFT);

        assert_eq!(
            pressed,
            vec![HotkeyEvent::Pressed(HotkeyAction::PrioritySpeaker)]
        );
    }

    #[test]
    fn releasing_the_modifier_first_keeps_the_hotkey_until_the_key_goes() {
        let mut keys = matcher(&[(HotkeyAction::PushToTalk, "CTRL+m")]);
        let ctrl = CONTROL | LEFT_CONTROL;
        flags_changed(&mut keys, LEFT_CTRL, ctrl);
        key(&mut keys, M, true, ctrl);

        let modifier_up = flags_changed(&mut keys, LEFT_CTRL, 0);
        let key_up = key(&mut keys, M, false, 0);

        assert!(modifier_up.is_empty());
        assert_eq!(
            key_up,
            vec![HotkeyEvent::Released(HotkeyAction::PushToTalk)]
        );
    }

    #[test]
    fn push_to_talk_on_two_keys_lasts_until_both_are_up() {
        let mut keys = matcher(&[
            (HotkeyAction::PushToTalk, "grave"),
            (HotkeyAction::PushToTalk, "Control_R"),
        ]);
        let right = CONTROL | RIGHT_CONTROL;

        let first = key(&mut keys, 0x32, true, 0);
        let second = flags_changed(&mut keys, RIGHT_CTRL, right);
        let one_up = key(&mut keys, 0x32, false, right);
        let last_up = flags_changed(&mut keys, RIGHT_CTRL, 0);

        assert_eq!(first, vec![HotkeyEvent::Pressed(HotkeyAction::PushToTalk)]);
        assert!(second.is_empty());
        assert!(one_up.is_empty(), "Right Ctrl is still down");
        assert_eq!(
            last_up,
            vec![HotkeyEvent::Released(HotkeyAction::PushToTalk)]
        );
    }

    #[test]
    fn a_modifier_key_alone_is_a_hotkey_and_only_its_side() {
        let mut keys = matcher(&[(HotkeyAction::PushToTalk, "Control_R")]);

        let left = flags_changed(&mut keys, LEFT_CTRL, CONTROL | LEFT_CONTROL);
        let left_up = flags_changed(&mut keys, LEFT_CTRL, 0);
        let down = flags_changed(&mut keys, RIGHT_CTRL, CONTROL | RIGHT_CONTROL);
        let up = flags_changed(&mut keys, RIGHT_CTRL, 0);

        assert!(left.is_empty() && left_up.is_empty());
        assert_eq!(down, vec![HotkeyEvent::Pressed(HotkeyAction::PushToTalk)]);
        assert_eq!(up, vec![HotkeyEvent::Released(HotkeyAction::PushToTalk)]);
    }

    #[test]
    fn a_toggle_fires_once_per_press() {
        let mut keys = matcher(&[(HotkeyAction::ToggleMute, "CTRL+m")]);
        let ctrl = CONTROL | LEFT_CONTROL;

        let mut events = Vec::new();
        for _ in 0..2 {
            events.extend(key(&mut keys, M, true, ctrl));
            events.extend(key(&mut keys, M, true, ctrl));
            events.extend(key(&mut keys, M, false, ctrl));
        }

        assert_eq!(
            events,
            [
                HotkeyEvent::Pressed(HotkeyAction::ToggleMute),
                HotkeyEvent::Released(HotkeyAction::ToggleMute),
                HotkeyEvent::Pressed(HotkeyAction::ToggleMute),
                HotkeyEvent::Released(HotkeyAction::ToggleMute),
            ]
        );
    }

    #[test]
    fn a_new_layout_releases_what_was_down() {
        let mut keys = matcher(&[(HotkeyAction::PushToTalk, "F12")]);
        key(&mut keys, F12, true, 0);

        let released = keys.release_all();

        assert_eq!(
            released,
            vec![HotkeyEvent::Released(HotkeyAction::PushToTalk)]
        );
        assert!(keys.release_all().is_empty());
    }

    /// Posts `keys` (keycode, down?, flags) as real key events and returns
    /// the hotkey events the tap got for `watched`.
    fn posted(
        watched: &[(HotkeyAction, &str)],
        keys: &[(u16, bool, u64)],
        expected: usize,
    ) -> Vec<HotkeyEvent> {
        use objc2_core_graphics::{CGEventSource, CGEventSourceStateID};

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (sender, mut received) = mpsc::unbounded_channel();
        let (keys_bound, support) = runtime.block_on(bind("opencord", &bindings(watched), &sender));
        assert!(
            matches!(support, HotkeySupport::Global { .. }),
            "{support:?}: allow the terminal app under Input Monitoring"
        );
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState);
        for (keycode, down, flags) in keys {
            let modifier = MODIFIER_KEYS.iter().any(|(code, _, _)| code == keycode);
            let event = CGEvent::new_keyboard_event(source.as_deref(), *keycode, *down)
                .expect("a key event");
            if modifier {
                CGEvent::set_type(Some(&event), CGEventType::FlagsChanged);
            }
            CGEvent::set_flags(Some(&event), objc2_core_graphics::CGEventFlags(*flags));
            CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        let mut events = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while events.len() < expected && std::time::Instant::now() < deadline {
            match received.try_recv() {
                Ok(event) => events.push(event),
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(10)),
            }
        }
        // Anything more would come at once.
        std::thread::sleep(std::time::Duration::from_millis(200));
        while let Ok(event) = received.try_recv() {
            events.push(event);
        }
        drop(keys_bound);
        events
    }

    const F13: u16 = 0x69;
    const F14: u16 = 0x6b;
    const F15: u16 = 0x71;

    /// Needs Input Monitoring and Accessibility for the terminal app, and
    /// types F13–F15 and Control into the focused app: run with
    /// `--ignored --test-threads=1` with TextEdit focused.
    #[test]
    #[ignore = "posts real keys; needs Input Monitoring and Accessibility"]
    fn posted_keys_press_and_release_hotkeys() {
        let ctrl = CONTROL | LEFT_CONTROL;

        let events = posted(
            &[(HotkeyAction::PushToTalk, "CTRL+F13")],
            &[
                (LEFT_CTRL, true, ctrl),
                (F13, true, ctrl),
                (F13, false, ctrl),
                (LEFT_CTRL, false, 0),
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

    #[test]
    #[ignore = "posts real keys; needs Input Monitoring and Accessibility"]
    fn posted_push_to_talk_on_two_keys_holds_until_both_are_up() {
        let right = CONTROL | RIGHT_CONTROL;

        let events = posted(
            &[
                (HotkeyAction::PushToTalk, "F14"),
                (HotkeyAction::PushToTalk, "Control_R"),
            ],
            &[
                (F14, true, 0),
                (RIGHT_CTRL, true, right),
                (F14, false, right),
                (RIGHT_CTRL, false, 0),
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

    #[test]
    #[ignore = "posts real keys; needs Input Monitoring and Accessibility"]
    fn posted_right_ctrl_alone_is_a_hotkey() {
        let events = posted(
            &[(HotkeyAction::PushToTalk, "Control_R")],
            &[
                (LEFT_CTRL, true, CONTROL | LEFT_CONTROL),
                (LEFT_CTRL, false, 0),
                (RIGHT_CTRL, true, CONTROL | RIGHT_CONTROL),
                (RIGHT_CTRL, false, 0),
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

    #[test]
    #[ignore = "posts real keys; needs Input Monitoring and Accessibility"]
    fn a_posted_toggle_fires_once_per_press() {
        let events = posted(
            &[(HotkeyAction::ToggleMute, "F15")],
            &[
                (F15, true, 0),
                (F15, true, 0),
                (F15, true, 0),
                (F15, false, 0),
            ],
            2,
        );

        assert_eq!(
            events,
            vec![
                HotkeyEvent::Pressed(HotkeyAction::ToggleMute),
                HotkeyEvent::Released(HotkeyAction::ToggleMute),
            ]
        );
    }
}
