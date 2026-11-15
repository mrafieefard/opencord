//! `app.on_back` (§11.4): the phone's back, once no modal is open, asks the
//! plugin first.
//!
//! The renderer is told whether a handler is set (`back_handler`), so with
//! none it never asks. A `back` input with an id is answered by one `back`
//! op with that id on every path: the handler's `true` is handled; anything
//! else, an error, a handler over its budget, or a stopped view is not.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use mlua::{Function, Lua, Table, Value};
use opencord_plugins::lua_api::{function, raise};
use opencord_plugins::view::Patch;

use super::api_app::Shared;

/// Registry key of the function `app.on_back` was given.
const BACK_KEY: &str = "opencord.view.back";

#[derive(Default)]
pub(super) struct Back {
    set: AtomicBool,
    /// What the renderer was last told.
    told: AtomicBool,
}

impl Back {
    /// The VM with the handler is gone or stopped.
    pub(super) fn forget(&self) {
        self.set.store(false, Ordering::Relaxed);
    }

    /// The op that brings the renderer up to date, when it's behind.
    pub(super) fn change(&self) -> Option<Patch> {
        let on = self.set.load(Ordering::Relaxed);
        (self.told.swap(on, Ordering::Relaxed) != on).then_some(Patch::BackHandler { on })
    }
}

/// Sets `app.on_back` on `app`.
pub(super) fn install(lua: &Lua, app: &Table, shared: &Arc<Shared>) -> mlua::Result<()> {
    let setter = shared.clone();
    let on_back = function(lua, move |lua, handler: Value| {
        let Value::Function(handler) = handler else {
            return Err(raise("INVALID_ARGUMENT: app.on_back takes a function"));
        };
        lua.set_named_registry_value(BACK_KEY, handler)?;
        setter.back.set.store(true, Ordering::Relaxed);
        Ok(())
    })?;
    app.raw_set("on_back", on_back)
}

/// The function a `back` input runs, if the script set one.
pub(super) fn handler(lua: &Lua) -> mlua::Result<Option<Function>> {
    lua.named_registry_value(BACK_KEY)
}

/// The answer to back `id`, sent with the next batch.
pub(super) fn answer(shared: &Shared, id: u64, handled: bool) {
    shared.push_effect(Patch::Back { id, handled });
}

#[cfg(test)]
#[path = "back_tests.rs"]
mod tests;
