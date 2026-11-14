//! The frame loop (§11.5): `app.on_frame` and `app.off_frame`.
//!
//! The renderer sends `frame` inputs only between a `frames { on = true }`
//! op and the `frames { on = false }` after it, and the frames are on while
//! a script wants them and the view isn't paused. The function is in the
//! registry, so it goes with its VM.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use mlua::{Function, Lua, Table, Value};
use opencord_plugins::lua_api::{function, raise};
use opencord_plugins::view::Patch;

use super::api_app::Shared;

/// Registry key of the function `app.on_frame` was given.
const FRAME_KEY: &str = "opencord.view.frame";

#[derive(Default)]
pub(super) struct Frames {
    wanted: AtomicBool,
    paused: AtomicBool,
    /// What the renderer was last told.
    told: AtomicBool,
}

impl Frames {
    pub(super) fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    /// The VM with the frame function is gone or stopped.
    pub(super) fn forget(&self) {
        self.wanted.store(false, Ordering::Relaxed);
    }

    /// The op that brings the renderer up to date, when it's behind.
    pub(super) fn change(&self) -> Option<Patch> {
        let on = self.wanted.load(Ordering::Relaxed) && !self.paused.load(Ordering::Relaxed);
        (self.told.swap(on, Ordering::Relaxed) != on).then_some(Patch::Frames { on })
    }
}

/// Sets `app.on_frame` and `app.off_frame` on `app`.
pub(super) fn install(lua: &Lua, app: &Table, shared: &Arc<Shared>) -> mlua::Result<()> {
    let starter = shared.clone();
    let on_frame = function(lua, move |lua, handler: Value| {
        let Value::Function(handler) = handler else {
            return Err(raise("INVALID_ARGUMENT: app.on_frame takes a function"));
        };
        lua.set_named_registry_value(FRAME_KEY, handler)?;
        starter.frames.wanted.store(true, Ordering::Relaxed);
        Ok(())
    })?;
    let stopper = shared.clone();
    let off_frame = function(lua, move |lua, ()| stop(lua, &stopper))?;
    app.raw_set("on_frame", on_frame)?;
    app.raw_set("off_frame", off_frame)
}

/// Ends the frame loop: `app.off_frame`, or a frame over its budget.
pub(super) fn stop(lua: &Lua, shared: &Shared) -> mlua::Result<()> {
    shared.frames.forget();
    lua.unset_named_registry_value(FRAME_KEY)
}

/// The function a `frame` input runs, if the script set one.
pub(super) fn handler(lua: &Lua) -> mlua::Result<Option<Function>> {
    lua.named_registry_value(FRAME_KEY)
}

#[cfg(test)]
#[path = "frames_tests.rs"]
mod tests;
