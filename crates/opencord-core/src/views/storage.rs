//! Device storage (§11.4): `app.storage`, a small key-value store per
//! plugin per server on this device, in `<storage_dir>/storage.json`.
//!
//! - One [`Store`] per folder, shared by every open view of the plugin, so
//!   one view's save can't overwrite what another just set. [`OPEN`] holds
//!   them weakly; a store goes when its last view does, and the dead
//!   entries leave at the next [`open`].
//! - A set changes the memory and the file is written at the end of the
//!   turn, once, outside any handler's budget: a slow disk must not count
//!   as the plugin's time. It's written aside and renamed, so a crash
//!   leaves the old file or the new one.
//! - A file that's there but can't be read stays as it is, and the store
//!   raises instead of starting empty: the next save would destroy it.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::Instant;

use mlua::{ChunkMode, Lua, Table, Value};
use opencord_plugins::limits::{DEVICE_STORAGE_KEY_MAX_CHARS, DEVICE_STORAGE_MAX_BYTES};
use opencord_plugins::lua_api::{function, raise};

use super::api_app::Shared;

const FILE: &str = "storage.json";
/// Written first, then renamed over [`FILE`]. One left by a failed save is
/// replaced by the next.
const ASIDE: &str = "storage.json.new";
/// `{}`: what a file with no entries holds.
const BRACES_BYTES: usize = 2;
const FULL: &str = "PLUGIN_LIMIT: this plugin's device storage is full";
const UNREADABLE: &str = "INTERNAL: this plugin's device storage can't be read";

static OPEN: Mutex<BTreeMap<PathBuf, Weak<Store>>> = Mutex::new(BTreeMap::new());

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The store in `dir`, shared with any view that has it open. Reads the
/// file, so it runs on the view's thread and outside any budget.
pub(super) fn open(dir: PathBuf) -> Arc<Store> {
    let mut open = locked(&OPEN);
    open.retain(|_, store| store.strong_count() > 0);
    if let Some(store) = open.get(&dir).and_then(Weak::upgrade) {
        return store;
    }
    let store = Arc::new(Store::load(dir.clone()));
    open.insert(dir, Arc::downgrade(&store));
    store
}

pub(super) struct Store {
    dir: PathBuf,
    state: Mutex<State>,
    /// Held across a save, so two views' writes can't cross. Never taken
    /// with `state` held.
    saving: Mutex<()>,
}

struct State {
    /// `None` when the file couldn't be read.
    entries: Option<Entries>,
    /// Changed since the last save.
    dirty: bool,
}

#[derive(Default)]
struct Entries {
    /// Key to the value's JSON text.
    values: BTreeMap<String, String>,
    /// What the values cost together, by [`cost`].
    bytes: usize,
}

/// What an entry adds to the file: `"key":value,`.
fn cost(key: &str, value: &str) -> usize {
    json_key(key).len() + value.len() + 2
}

fn json_key(key: &str) -> String {
    serde_json::Value::from(key).to_string()
}

impl Entries {
    fn insert(&mut self, key: String, value: String) {
        self.bytes += cost(&key, &value);
        if let Some(old) = self.values.insert(key.clone(), value) {
            self.bytes -= cost(&key, &old);
        }
    }

    fn text(&self) -> String {
        let mut text = String::with_capacity(self.bytes + BRACES_BYTES);
        text.push('{');
        for (index, (key, value)) in self.values.iter().enumerate() {
            if index > 0 {
                text.push(',');
            }
            text.push_str(&json_key(key));
            text.push(':');
            text.push_str(value);
        }
        text.push('}');
        text
    }
}

fn read_file(dir: &Path) -> io::Result<Entries> {
    let file = match File::open(dir.join(FILE)) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Entries::default()),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    file.take(DEVICE_STORAGE_MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > DEVICE_STORAGE_MAX_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "larger than a plugin's device storage may be",
        ));
    }
    let parsed: BTreeMap<String, serde_json::Value> = serde_json::from_slice(&bytes)?;
    let mut entries = Entries::default();
    for (key, value) in parsed {
        entries.insert(key, value.to_string());
    }
    Ok(entries)
}

impl Store {
    fn load(dir: PathBuf) -> Store {
        let began = Instant::now();
        let entries = match read_file(&dir) {
            Ok(entries) => Some(entries),
            Err(error) => {
                tracing::warn!(
                    kind = ?error.kind(),
                    after_ms = began.elapsed().as_millis() as u64,
                    "a plugin's device storage couldn't be read"
                );
                None
            }
        };
        Store {
            dir,
            state: Mutex::new(State {
                entries,
                dirty: false,
            }),
            saving: Mutex::new(()),
        }
    }

    fn get(&self, key: &str) -> mlua::Result<Option<String>> {
        let state = locked(&self.state);
        let entries = state.entries.as_ref().ok_or_else(|| raise(UNREADABLE))?;
        Ok(entries.values.get(key).cloned())
    }

    fn set(&self, key: String, value: String) -> mlua::Result<()> {
        let mut state = locked(&self.state);
        let entries = state.entries.as_mut().ok_or_else(|| raise(UNREADABLE))?;
        let old = entries.values.get(&key).map_or(0, |old| cost(&key, old));
        let total = entries.bytes - old + cost(&key, &value) + BRACES_BYTES;
        if total > DEVICE_STORAGE_MAX_BYTES {
            return Err(raise(FULL));
        }
        entries.insert(key, value);
        state.dirty = true;
        Ok(())
    }

    fn delete(&self, key: &str) -> mlua::Result<()> {
        let mut state = locked(&self.state);
        let entries = state.entries.as_mut().ok_or_else(|| raise(UNREADABLE))?;
        if let Some(old) = entries.values.remove(key) {
            entries.bytes -= cost(key, &old);
            state.dirty = true;
        }
        Ok(())
    }

    /// Writes the file if anything changed. A failure stays to retry at
    /// the next turn.
    pub(super) fn save(&self) -> io::Result<()> {
        if !locked(&self.state).dirty {
            return Ok(());
        }
        let _saving = locked(&self.saving);
        let text = {
            let mut state = locked(&self.state);
            match (state.dirty, &state.entries) {
                (true, Some(entries)) => {
                    let text = entries.text();
                    state.dirty = false;
                    text
                }
                // Another view's save got here first.
                _ => return Ok(()),
            }
        };
        let written = self.write(text.as_bytes());
        if written.is_err() {
            locked(&self.state).dirty = true;
        }
        written
    }

    fn write(&self, text: &[u8]) -> io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let aside = self.dir.join(ASIDE);
        let mut file = File::create(&aside)?;
        file.write_all(text)?;
        // On disk before the rename, so a crash can't leave an empty file.
        file.sync_all()?;
        drop(file);
        std::fs::rename(aside, self.dir.join(FILE))
    }
}

/// A key, as the plan limits it: 1 to 200 characters of UTF-8 text.
fn key_arg(key: &Value) -> mlua::Result<String> {
    let bad = || {
        raise(format!(
            "INVALID_ARGUMENT: a storage key is 1 to {DEVICE_STORAGE_KEY_MAX_CHARS} characters of text"
        ))
    };
    let Value::String(key) = key else {
        return Err(bad());
    };
    let key = key.to_str().map_err(|_| bad())?.to_owned();
    let chars = key.chars().count();
    if chars == 0 || chars > DEVICE_STORAGE_KEY_MAX_CHARS {
        return Err(bad());
    }
    Ok(key)
}

/// Builds `app.storage` in Lua around the Rust checks. Values cross as JSON
/// text, so only plain data is stored; `set(key, nil)` is `delete(key)`.
const STORAGE: &str = r#"
local get_text, set_text, delete, check_key, encode, decode, null = ...
local freeze = table.freeze
return freeze({
    get = function(key)
        local text = get_text(key)
        if text == nil then
            return nil
        end
        local value = decode(text)
        if value == null then
            return nil
        end
        return value
    end,
    set = function(key, value)
        check_key(key)
        if value == nil then
            delete(key)
        else
            set_text(key, encode(value))
        end
    end,
    delete = delete,
})
"#;

/// Sets `app.storage` on `app`. Runs Lua, so the caller has a budget running.
pub(super) fn install(lua: &Lua, app: &Table, shared: &Arc<Shared>) -> mlua::Result<()> {
    let store = shared.storage.clone();
    let get_text = function(lua, move |_, key: Value| store.get(&key_arg(&key)?))?;
    let store = shared.storage.clone();
    let set_text = function(lua, move |_, (key, text): (Value, Value)| {
        let key = key_arg(&key)?;
        let Value::String(text) = text else {
            return Err(raise("INTERNAL: app.storage.set lost its value"));
        };
        // `json.encode` writes only UTF-8, so nothing is lost here.
        store.set(key, text.to_string_lossy())
    })?;
    let store = shared.storage.clone();
    let delete = function(lua, move |_, key: Value| store.delete(&key_arg(&key)?))?;
    let check_key = function(lua, |_, key: Value| key_arg(&key).map(|_| ()))?;

    let json: Table = lua.globals().get("json")?;
    let storage: Table = lua
        .load(STORAGE)
        .set_name("=app.storage")
        .set_mode(ChunkMode::Text)
        .call((
            get_text,
            set_text,
            delete,
            check_key,
            json.get::<Value>("encode")?,
            json.get::<Value>("decode")?,
            json.get::<Value>("null")?,
        ))?;
    app.raw_set("storage", storage)
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;
