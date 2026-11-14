//! The `app` table (§11.4): the view's state, events both ways with the
//! plugin's server script, `app.copy` and the view's own events.
//!
//! Lua functions a script hands over (`app.on`, `app.listen`) are kept in
//! Lua tables in the registry, never in Rust: a Rust value the API's
//! callbacks hold that held Lua functions would keep the VM alive forever.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use mlua::{ChunkMode, Function, Lua, Table, Value};
use opencord_plugins::limits::{
    VIEW_EVENT_MAX_BYTES, VIEW_LINK_MAX_CHARS, VIEW_MAX_EFFECTS, VIEW_MAX_LISTENERS,
    VIEW_MAX_SOUNDS, VIEW_MAX_TEXT_CHARS,
};
use opencord_plugins::lua_api::{function, raise};
use opencord_plugins::package::is_asset_path;
use opencord_plugins::view::Patch;

use super::back::Back;
use super::call::is_id;
use super::dialogs::Waits;
use super::frames::Frames;
use super::storage::{self, Store};
use super::timers::Timers;
use super::{ViewSink, cut};

/// Registry keys of the tables `app.on` and `app.listen` fill: name → list
/// of functions.
pub(super) const ON_KEY: &str = "opencord.view.on";
pub(super) const LISTEN_KEY: &str = "opencord.view.listen";

/// Registry key of the table of `on_end` functions by sound id.
const SOUNDS_KEY: &str = "opencord.view.sounds";

/// What `app.listen` takes (Claude's call 3).
const LISTEN_NAMES: &[&str] = &[
    "close",
    "pause",
    "resume",
    "resize",
    "theme_change",
    "connection_change",
    "focus",
    "blur",
    "key_down",
    "key_up",
    // An activity's call (Task 6 of milestone 3); a channel's view may
    // listen, and never hears them.
    "call_members_change",
    "speaking_change",
];

/// The longest event name `app.send` and `app.on` take, in bytes.
const EVENT_NAME_MAX: usize = 64;

/// Where the view is in its life, for `app.set`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum Phase {
    /// Loading the script, `init`, `migrate`: there's no state to set yet.
    Starting = 0,
    /// `render` runs: a change now would ask for a render after every one.
    Rendering = 1,
    Running = 2,
}

/// What the API's callbacks and the view's thread share. Holds no Lua
/// values, so a VM's callbacks holding it keep nothing alive.
pub(super) struct Shared {
    pub(super) sink: Arc<dyn ViewSink>,
    dirty: AtomicBool,
    phase: AtomicU8,
    /// Set while a press handler (`press`, `double_press`, `long_press`,
    /// `secondary_press`) is being polled, for `app.copy`.
    in_press: AtomicBool,
    /// Ops other than the tree's, sent with the next batch.
    effects: Mutex<Vec<Patch>>,
    /// Handlers paused on a quick dialog or `app.modal.ask`.
    pub(super) waits: Waits,
    pub(super) timers: Timers,
    pub(super) frames: Frames,
    pub(super) back: Back,
    pub(super) storage: Arc<Store>,
    sounds: Sounds,
}

/// Sounds played; their `on_end` functions are in the registry. An entry
/// leaves on its `sound_end` and with the VM.
#[derive(Default)]
struct Sounds {
    /// Never reset, so a late `sound_end` can't name a new sound.
    last_id: AtomicU64,
    /// How many have an `on_end` waiting.
    waiting: AtomicUsize,
}

impl Shared {
    /// Opens the plugin's storage, reading its file: on the view's thread.
    pub(super) fn new(sink: Arc<dyn ViewSink>, storage_dir: PathBuf) -> Shared {
        Shared {
            sink,
            dirty: AtomicBool::new(false),
            phase: AtomicU8::new(Phase::Starting as u8),
            in_press: AtomicBool::new(false),
            effects: Mutex::new(Vec::new()),
            waits: Waits::default(),
            timers: Timers::default(),
            frames: Frames::default(),
            back: Back::default(),
            storage: storage::open(storage_dir),
            sounds: Sounds::default(),
        }
    }

    /// What called into a VM that's gone or stopped.
    pub(super) fn forget_vm(&self) {
        self.timers.clear();
        self.frames.forget();
        self.back.forget();
        self.sounds.waiting.store(0, Ordering::Relaxed);
    }

    pub(super) fn set_phase(&self, phase: Phase) {
        self.phase.store(phase as u8, Ordering::Relaxed);
    }

    pub(super) fn phase(&self) -> Phase {
        match self.phase.load(Ordering::Relaxed) {
            0 => Phase::Starting,
            1 => Phase::Rendering,
            _ => Phase::Running,
        }
    }

    /// Whether Lua runs in a handler: once started, outside `render`, Lua
    /// runs only in handlers (and `close` listeners), each an async call
    /// that may pause.
    pub(super) fn in_handler(&self) -> bool {
        self.phase() == Phase::Running
    }

    pub(super) fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Relaxed);
    }

    /// Whether anything asked for a redraw since the last call.
    pub(super) fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::Relaxed)
    }

    pub(super) fn set_in_press(&self, pressing: bool) {
        self.in_press.store(pressing, Ordering::Relaxed);
    }

    pub(super) fn push_effect(&self, op: Patch) {
        self.effects
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(op);
    }

    /// [`Self::push_effect`] for what a script asks for, counted.
    fn queue_effect(&self, op: Patch) -> mlua::Result<()> {
        let mut effects = self.effects.lock().unwrap_or_else(PoisonError::into_inner);
        if effects.len() >= VIEW_MAX_EFFECTS {
            return Err(raise(format!(
                "PLUGIN_LIMIT: at most {VIEW_MAX_EFFECTS} side effects at once"
            )));
        }
        effects.push(op);
        Ok(())
    }

    pub(super) fn take_effects(&self) -> Vec<Patch> {
        std::mem::take(&mut *self.effects.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

/// Builds `app` in Lua, around the Rust checks. Globals are read once, here,
/// before any plugin code runs.
const APP: &str = r#"
local mark, check_name, send_text, copy, check_listen, encode, on_table, listen_table, max_handlers = ...
local error, type, rawget, insert = error, type, rawget, table.insert
local app = {}
local count = 0
local function add(list, name, fn, what)
    if type(fn) ~= "function" then
        error("INVALID_ARGUMENT: " .. what .. " takes a function", 0)
    end
    if count >= max_handlers then
        error("PLUGIN_LIMIT: at most " .. max_handlers .. " handlers with app.on and app.listen", 0)
    end
    count += 1
    local handlers = list[name]
    if handlers == nil then
        handlers = {}
        list[name] = handlers
    end
    insert(handlers, fn)
end
function app.set(fn)
    if type(fn) ~= "function" then
        error("INVALID_ARGUMENT: app.set takes a function", 0)
    end
    mark()
    fn(rawget(app, "state"))
end
function app.send(name, data)
    check_name(name)
    local text = if data == nil then "null" else encode(data)
    send_text(name, text)
end
function app.on(name, fn)
    check_name(name)
    add(on_table, name, fn, "app.on")
end
function app.listen(name, fn)
    check_listen(name)
    add(listen_table, name, fn, "app.listen")
end
app.copy = copy
return app
"#;

/// Sets the global `app` and returns it. Runs Lua, so the caller has a
/// budget running.
pub(super) fn install(lua: &Lua, shared: &Arc<Shared>) -> mlua::Result<Table> {
    let marker = shared.clone();
    let mark = function(lua, move |_, ()| match marker.phase() {
        Phase::Running => {
            marker.mark_dirty();
            Ok(())
        }
        Phase::Starting => Err(raise(
            "FORBIDDEN: app.set works once init has returned the view's state",
        )),
        Phase::Rendering => Err(raise(
            "FORBIDDEN: render can't call app.set; change the state in a handler",
        )),
    })?;
    let check_name = function(lua, |_, name: Value| event_name(&name).map(|_| ()))?;
    let sender = shared.clone();
    let send_text = function(lua, move |_, (name, text): (Value, Value)| {
        let name = event_name(&name)?;
        let Value::String(text) = text else {
            return Err(raise("INTERNAL: app.send lost its data"));
        };
        if text.as_bytes().len() > VIEW_EVENT_MAX_BYTES {
            return Err(raise("PLUGIN_LIMIT: an event is at most 64 KiB"));
        }
        // `json.encode` writes only UTF-8, so nothing is lost here.
        sender.sink.send(name, text.to_string_lossy());
        Ok(())
    })?;
    let copier = shared.clone();
    let copy = function(lua, move |_, text: Value| {
        if !copier.in_press.load(Ordering::Relaxed) {
            return Err(raise(
                "FORBIDDEN: app.copy works only while handling a press",
            ));
        }
        let Value::String(text) = text else {
            return Err(raise("INVALID_ARGUMENT: app.copy takes a string"));
        };
        let text = text
            .to_str()
            .map_err(|_| raise("INVALID_ARGUMENT: app.copy takes UTF-8 text"))?
            .to_owned();
        if text.chars().count() > VIEW_MAX_TEXT_CHARS {
            return Err(raise(format!(
                "INVALID_ARGUMENT: app.copy takes at most {VIEW_MAX_TEXT_CHARS} characters"
            )));
        }
        copier.queue_effect(Patch::Copy { text })
    })?;
    let check_listen = function(lua, |_, name: Value| {
        let known = match &name {
            Value::String(name) => LISTEN_NAMES
                .iter()
                .any(|known| known.as_bytes() == &*name.as_bytes()),
            _ => false,
        };
        if known {
            return Ok(());
        }
        let shown = match &name {
            Value::String(name) => name.to_string_lossy(),
            other => other.type_name().to_owned(),
        };
        Err(raise(format!(
            "INVALID_ARGUMENT: no view event called {}",
            cut(&shown, EVENT_NAME_MAX)
        )))
    })?;

    let json: Table = lua.globals().get("json")?;
    let encode: Function = json.get("encode")?;
    let on_table = lua.create_table()?;
    lua.set_named_registry_value(ON_KEY, &on_table)?;
    let listen_table = lua.create_table()?;
    lua.set_named_registry_value(LISTEN_KEY, &listen_table)?;

    let app: Table = lua
        .load(APP)
        .set_name("=app")
        .set_mode(ChunkMode::Text)
        .call((
            mark,
            check_name,
            send_text,
            copy,
            check_listen,
            encode,
            on_table,
            listen_table,
            VIEW_MAX_LISTENERS,
        ))?;
    super::modals::install(lua, &app, shared)?;
    super::dialogs::install(lua, &app, shared)?;
    super::timers::install(lua, &app, shared)?;
    super::frames::install(lua, &app, shared)?;
    super::back::install(lua, &app, shared)?;
    storage::install(lua, &app, shared)?;
    install_effects(lua, &app, shared)?;
    app.set_readonly(true);
    lua.globals().raw_set("app", &app)?;
    Ok(app)
}

/// Sets the calls that only send the renderer an op.
fn install_effects(lua: &Lua, app: &Table, shared: &Arc<Shared>) -> mlua::Result<()> {
    lua.set_named_registry_value(SOUNDS_KEY, lua.create_table()?)?;
    let linker = shared.clone();
    let open_link = function(lua, move |_, url: Value| {
        let url = link(&url)?;
        linker.queue_effect(Patch::OpenLink { url })
    })?;
    let opener = shared.clone();
    let open_channel = function(lua, move |_, id: Value| {
        let channel_id = channel_id(&id)?;
        opener.queue_effect(Patch::OpenChannel { channel_id })
    })?;
    let player = shared.clone();
    let play_sound = function(lua, move |lua, (asset, options): (Value, Value)| {
        let asset = asset_path(&asset)?;
        let (volume, on_end) = sound_options(&options)?;
        let sounds = &player.sounds;
        if on_end.is_some() && sounds.waiting.load(Ordering::Relaxed) >= VIEW_MAX_SOUNDS {
            return Err(raise(format!(
                "PLUGIN_LIMIT: at most {VIEW_MAX_SOUNDS} sounds can wait for on_end"
            )));
        }
        let id = sounds.last_id.fetch_add(1, Ordering::Relaxed) + 1;
        player.queue_effect(Patch::Sound { id, asset, volume })?;
        if let Some(on_end) = on_end {
            let table: Table = lua.named_registry_value(SOUNDS_KEY)?;
            table.raw_set(id, on_end)?;
            sounds.waiting.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    })?;
    app.raw_set("open_link", open_link)?;
    app.raw_set("open_channel", open_channel)?;
    app.raw_set("play_sound", play_sound)
}

/// A `sound_end` input: the sound's `on_end`, if it has one still waiting.
pub(super) fn take_on_end(lua: &Lua, shared: &Shared, id: u64) -> mlua::Result<Option<Function>> {
    let table: Table = lua.named_registry_value(SOUNDS_KEY)?;
    let on_end = table.raw_get::<Option<Function>>(id)?;
    if on_end.is_some() {
        // Not `raw_remove`: on a number key that shifts the entries after it.
        table.raw_set(id, Value::Nil)?;
        shared.sounds.waiting.fetch_sub(1, Ordering::Relaxed);
    }
    Ok(on_end)
}

/// A link as the parser wrote it: http or https, at most
/// [`VIEW_LINK_MAX_CHARS`] characters.
fn link(url: &Value) -> mlua::Result<String> {
    let bad = || raise("INVALID_ARGUMENT: app.open_link takes an http or https address");
    let long = || {
        raise(format!(
            "INVALID_ARGUMENT: a link is at most {VIEW_LINK_MAX_CHARS} characters"
        ))
    };
    let Value::String(url) = url else {
        return Err(bad());
    };
    let url = url.to_str().map_err(|_| bad())?.to_owned();
    if url.chars().count() > VIEW_LINK_MAX_CHARS {
        return Err(long());
    }
    let parsed = reqwest::Url::parse(&url).map_err(|_| bad())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(bad());
    }
    // The parser percent-encodes, which can only lengthen it.
    if parsed.as_str().len() > VIEW_LINK_MAX_CHARS {
        return Err(long());
    }
    Ok(parsed.into())
}

/// A channel id: a decimal string that fits a `u64`.
fn channel_id(id: &Value) -> mlua::Result<String> {
    match id {
        Value::String(id) if id.to_str().is_ok_and(|id| is_id(&id)) => Ok(id.to_string_lossy()),
        _ => Err(raise("INVALID_ARGUMENT: channel_id must be an id string")),
    }
}

fn asset_path(asset: &Value) -> mlua::Result<String> {
    let path = match asset {
        Value::String(asset) => asset.to_str().ok().map(|asset| asset.to_owned()),
        _ => None,
    };
    path.filter(|path| is_asset_path(path))
        .ok_or_else(|| raise("INVALID_ARGUMENT: app.play_sound takes a file in assets/"))
}

/// `app.play_sound`'s options: the volume (1 unless given) and `on_end`.
fn sound_options(options: &Value) -> mlua::Result<(f64, Option<Function>)> {
    let options = match options {
        Value::Nil => return Ok((1.0, None)),
        Value::Table(options) => options,
        _ => {
            return Err(raise(
                "INVALID_ARGUMENT: app.play_sound's options must be a table",
            ));
        }
    };
    let volume = match options.raw_get::<Value>("volume")? {
        Value::Nil => 1.0,
        volume => finite(&volume, "volume")?,
    };
    if !(0.0..=1.0).contains(&volume) {
        return Err(raise("INVALID_ARGUMENT: volume must be from 0 to 1"));
    }
    match options.raw_get::<Value>("on_end")? {
        Value::Nil => Ok((volume, None)),
        Value::Function(on_end) => Ok((volume, Some(on_end))),
        _ => Err(raise("INVALID_ARGUMENT: on_end must be a function")),
    }
}

/// A number from Lua that's checked for being finite before use.
pub(super) fn finite(value: &Value, name: &str) -> mlua::Result<f64> {
    let number = match value {
        Value::Integer(number) => *number as f64,
        Value::Number(number) => *number,
        _ => f64::NAN,
    };
    if number.is_finite() {
        Ok(number)
    } else {
        Err(raise(format!(
            "INVALID_ARGUMENT: {name} must be a finite number"
        )))
    }
}

/// Sets `app.state`, which scripts read but can't replace.
pub(super) fn set_state(app: &Table, state: &Table) -> mlua::Result<()> {
    app.set_readonly(false);
    let set = app.raw_set("state", state);
    app.set_readonly(true);
    set
}

/// The functions `app.on` (with [`ON_KEY`]) or `app.listen` (with
/// [`LISTEN_KEY`]) holds for `name`, in the order they were given.
pub(super) fn handlers_for(lua: &Lua, key: &str, name: &str) -> mlua::Result<Vec<Function>> {
    let table: Table = lua.named_registry_value(key)?;
    match table.raw_get::<Value>(name)? {
        Value::Table(list) => list.sequence_values::<Function>().collect(),
        _ => Ok(Vec::new()),
    }
}

/// Whether `bytes` is a name a script gives an event or a modal: 1 to 64 of
/// `a-z`, `0-9`, `_`, `.`, `:` and `-`.
pub(super) fn is_name(bytes: &[u8]) -> bool {
    !bytes.is_empty()
        && bytes.len() <= EVENT_NAME_MAX
        && bytes
            .iter()
            .all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.:-".contains(&b))
}

/// An event name, as [`is_name`] says.
fn event_name(name: &Value) -> mlua::Result<String> {
    let fine = match name {
        Value::String(name) => is_name(&name.as_bytes()),
        _ => false,
    };
    match name {
        // The check above leaves only ASCII.
        Value::String(name) if fine => Ok(name.to_string_lossy()),
        _ => Err(raise(
            "INVALID_ARGUMENT: an event name is 1 to 64 characters of a-z, 0-9 and _ . : -",
        )),
    }
}
