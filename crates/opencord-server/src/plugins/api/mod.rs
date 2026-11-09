//! The Lua API server scripts call (spec §10.3). Each module installs its
//! globals with `install(lua, env)`; [`install`] calls them all.
//!
//! Errors a plugin may catch are raised with [`raise`] and reach Lua as the
//! plain string `"<CODE>: <message>"` through [`function`] and
//! [`async_function`] (shared with the app's core, in
//! `opencord_plugins::lua_api`): an `Err` from a bare mlua callback would
//! reach Lua as userdata instead.

pub mod bus;
pub mod commands;
pub mod config;
pub mod db;
pub mod files;
pub mod guild;
pub mod hooks;
pub mod http;
pub mod log;
pub mod messages;
pub mod plugin;
pub mod timers;
pub mod voice;

use std::sync::Arc;

use mlua::{Lua, LuaSerdeExt, Value};
pub use opencord_plugins::lua_api::{
    Raise, async_function, async_function_passing, function, raise,
};
use opencord_plugins::sandbox::Vm;

use super::runtime::PluginEnv;

/// Installs every API module into a new VM. Runs Lua, so the caller has a
/// budget running.
pub fn install(vm: &Vm, env: &Arc<PluginEnv>) -> mlua::Result<()> {
    plugin::install(&vm.lua, env)?;
    log::install(vm, env)?;
    http::install(&vm.lua, env)?;
    db::install(&vm.lua, env)?;
    messages::install(&vm.lua, env)?;
    commands::install(&vm.lua, env)?;
    guild::install(&vm.lua, env)?;
    voice::install(&vm.lua, env)?;
    files::install(&vm.lua, env)?;
    hooks::install(&vm.lua, env)?;
    timers::install(&vm.lua, env)?;
    bus::install(&vm.lua, env)?;
    config::install(&vm.lua, env)?;
    #[cfg(any(test, debug_assertions))]
    tests::install(&vm.lua, env)?;
    Ok(())
}

/// Parses a required Snowflake ID string argument. Must consist strictly of ASCII digits and fit in an i64.
pub fn id_arg(val: Value, name: &str) -> mlua::Result<i64> {
    match val {
        Value::String(s) => {
            let bytes = s.as_bytes();
            let s_str = s
                .to_str()
                .map_err(|_| raise(format!("INVALID_ARGUMENT: {name} must be a valid string")))?;
            if s_str.is_empty() || !bytes.iter().all(|b| b.is_ascii_digit()) {
                return Err(raise(format!(
                    "INVALID_ARGUMENT: {name} must be numeric digits only"
                )));
            }
            s_str
                .parse::<i64>()
                .map_err(|_| raise(format!("INVALID_ARGUMENT: {name} is out of range")))
        }
        _ => Err(raise(format!(
            "INVALID_ARGUMENT: {name} must be an id string"
        ))),
    }
}

/// Parses an optional Snowflake ID string argument.
pub fn id_arg_opt(val: Value, name: &str) -> mlua::Result<Option<i64>> {
    match val {
        Value::Nil => Ok(None),
        val => id_arg(val, name).map(Some),
    }
}

/// Parses an optional string argument.
pub fn string_arg_opt(val: Value, what: &str) -> mlua::Result<Option<String>> {
    match val {
        Value::Nil => Ok(None),
        val => string_arg(val, what).map(Some),
    }
}

/// Parses a boolean argument.
pub fn bool_arg(val: Value, what: &str) -> mlua::Result<bool> {
    match val {
        Value::Boolean(b) => Ok(b),
        _ => Err(raise(format!("INVALID_ARGUMENT: {what} must be a boolean"))),
    }
}

/// A string argument, or `INVALID_ARGUMENT: {what} must be a string`.
pub fn string_arg(value: Value, what: &str) -> mlua::Result<String> {
    match value {
        Value::String(text) => text
            .to_str()
            .map(|text| text.to_owned())
            .map_err(|_| raise(format!("INVALID_ARGUMENT: {what} must be UTF-8 text"))),
        _ => Err(raise(format!("INVALID_ARGUMENT: {what} must be a string"))),
    }
}

/// Converts a serde_json::Value into an mlua::Value without userdata metatables.
pub fn to_lua_value(lua: &Lua, val: &serde_json::Value) -> mlua::Result<Value> {
    let options = mlua::SerializeOptions::new()
        .serialize_none_to_null(false)
        .serialize_unit_to_null(false)
        .set_array_metatable(false);
    lua.to_value_with(val, options)
}

#[cfg(any(test, debug_assertions))]
mod tests {
    //! `wait(seconds)`, for the runtime's tests of cancelling waits.

    use std::sync::Arc;
    use std::time::Duration;

    use opencord_plugins::sandbox::paused;
    use opencord_proto::v1 as proto;

    use super::super::logbook::PluginLog;
    use super::super::runtime::PluginEnv;

    /// Logs "wait cancelled" when dropped before the wait ended.
    struct CancelNote(Option<Arc<PluginLog>>);

    impl Drop for CancelNote {
        fn drop(&mut self) {
            if let Some(log) = self.0.take() {
                log.push(proto::PluginLogLevel::Warn, "wait cancelled");
            }
        }
    }

    pub(super) fn install(lua: &mlua::Lua, env: &Arc<PluginEnv>) -> mlua::Result<()> {
        let env = env.clone();
        let wait = super::async_function(lua, move |_, seconds: f64| {
            let log = env.log.clone();
            let budget = env.budget.clone();
            async move {
                let mut note = CancelNote(Some(log));
                paused(
                    &budget,
                    tokio::time::sleep(Duration::from_secs_f64(seconds)),
                )
                .await;
                note.0.take();
                Ok(())
            }
        })?;
        lua.globals().raw_set("wait", wait)
    }
}
