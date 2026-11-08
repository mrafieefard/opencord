// SPDX-License-Identifier: MPL-2.0

//! Web hooks API for plugins (design 2026-11-07 §10.8, §21).

use std::sync::Arc;

use mlua::{Lua, Value};

use super::{function, raise};
use crate::plugins::runtime::PluginEnv;

/// Key in Lua registry for the table of hook handlers: path -> Function.
pub const HOOKS: &str = "opencord.hooks";

pub fn install(lua: &Lua, env: &Arc<PluginEnv>) -> mlua::Result<()> {
    let hooks_registry = lua.create_table()?;
    lua.set_named_registry_value(HOOKS, &hooks_registry)?;

    let hooks = lua.create_table()?;
    let env_on = Arc::clone(env);
    let hooks_reg = hooks_registry.clone();
    hooks.raw_set(
        "on",
        function(lua, move |_lua, (path_val, handler_val): (Value, Value)| {
            let path = match path_val {
                Value::String(s) => s.to_str()?.to_owned(),
                _ => return Err(raise("INVALID_ARGUMENT: path must be a string")),
            };
            let path = path.trim_start_matches('/').to_owned();
            if path.is_empty() {
                return Err(raise("INVALID_ARGUMENT: path cannot be empty"));
            }
            let handler = match handler_val {
                Value::Function(f) => f,
                _ => return Err(raise("INVALID_ARGUMENT: handler must be a function")),
            };
            hooks_reg.raw_set(path.as_str(), handler)?;
            env_on
                .hooks
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(path);
            Ok(())
        })?,
    )?;

    lua.globals().raw_set("hooks", hooks)?;
    Ok(())
}
