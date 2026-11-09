//! The `app` table (§11.4): the view's state, events both ways with the
//! plugin's server script, `app.copy` and the view's own events.
//!
//! Lua functions a script hands over (`app.on`, `app.listen`) are kept in
//! Lua tables in the registry, never in Rust: a Rust value the API's
//! callbacks hold that held Lua functions would keep the VM alive forever.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use mlua::{ChunkMode, Function, Lua, Table, Value};
use opencord_plugins::limits::{VIEW_EVENT_MAX_BYTES, VIEW_MAX_LISTENERS, VIEW_MAX_TEXT_CHARS};
use opencord_plugins::lua_api::{function, raise};
use opencord_plugins::view::Patch;

use super::{ViewSink, cut};

/// Registry keys of the tables `app.on` and `app.listen` fill: name → list
/// of functions.
pub(super) const ON_KEY: &str = "opencord.view.on";
pub(super) const LISTEN_KEY: &str = "opencord.view.listen";

/// What `app.listen` takes. Task 5 adds the surroundings, lifecycle and
/// keyboard events.
const LISTEN_NAMES: &[&str] = &["close"];

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
}

impl Shared {
    pub(super) fn new(sink: Arc<dyn ViewSink>) -> Shared {
        Shared {
            sink,
            dirty: AtomicBool::new(false),
            phase: AtomicU8::new(Phase::Starting as u8),
            in_press: AtomicBool::new(false),
            effects: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn set_phase(&self, phase: Phase) {
        self.phase.store(phase as u8, Ordering::Relaxed);
    }

    fn phase(&self) -> Phase {
        match self.phase.load(Ordering::Relaxed) {
            0 => Phase::Starting,
            1 => Phase::Rendering,
            _ => Phase::Running,
        }
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
        copier.push_effect(Patch::Copy { text });
        Ok(())
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
    app.set_readonly(true);
    lua.globals().raw_set("app", &app)?;
    Ok(app)
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

/// An event name: 1 to 64 of `a-z`, `0-9`, `_`, `.`, `:` and `-`.
fn event_name(name: &Value) -> mlua::Result<String> {
    let fine = match name {
        Value::String(name) => {
            let bytes = name.as_bytes();
            !bytes.is_empty()
                && bytes.len() <= EVENT_NAME_MAX
                && bytes
                    .iter()
                    .all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.:-".contains(&b))
        }
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
