// SPDX-License-Identifier: MPL-2.0

//! Plugin bus API (plan Task 20, design 2026-11-07 §10.3).

use std::sync::Arc;

use mlua::{Lua, LuaSerdeExt, Table, Value};

use super::{PluginEnv, function, raise, string_arg};
use crate::plugins::permissions::PluginPermission;

pub const BUS_LISTENERS: &str = "__opencord_bus_listeners";

pub fn install(lua: &Lua, env: &Arc<PluginEnv>) -> mlua::Result<()> {
    lua.set_named_registry_value(BUS_LISTENERS, lua.create_table()?)?;

    let plugins_table = lua.create_table()?;

    let env_emit = env.clone();
    plugins_table.raw_set(
        "emit",
        function(lua, move |lua, (name_val, data): (Value, Value)| {
            env_emit.require(PluginPermission::TalkToPlugins)?;
            let name = string_arg(name_val, "name")?;
            env_emit.check_rate_limit()?;
            let data_json: serde_json::Value = lua
                .from_value(data)
                .map_err(|e| raise(format!("INVALID_ARGUMENT: {e}")))?;
            let data_bytes = serde_json::to_vec(&data_json)
                .map_err(|e| raise(format!("INVALID_ARGUMENT: {e}")))?;
            if data_bytes.len() > 64 * 1024 {
                return Err(raise("INVALID_ARGUMENT: data is over 64 KiB"));
            }
            let state = env_emit.state()?;
            state
                .plugins
                .bus
                .emit(&state.plugins, &env_emit.plugin_id, &name, data_json);
            Ok(())
        })?,
    )?;

    let env_on = env.clone();
    plugins_table.raw_set(
        "on",
        function(
            lua,
            move |lua, (sender_val, name_val, fn_val): (Value, Value, Value)| {
                env_on.require(PluginPermission::TalkToPlugins)?;
                let sender_id = string_arg(sender_val, "sender_id")?;
                let name = string_arg(name_val, "name")?;
                let handler = match fn_val {
                    Value::Function(f) => f,
                    _ => return Err(raise("INVALID_ARGUMENT: handler must be a function")),
                };
                let listeners: Table = lua.named_registry_value(BUS_LISTENERS)?;
                let key = format!("{sender_id}:{name}");
                let list: Table = match listeners.raw_get(&*key) {
                    Ok(l) => l,
                    Err(_) => {
                        let l = lua.create_table()?;
                        listeners.raw_set(&*key, l.clone())?;
                        l
                    }
                };
                list.raw_push(handler)?;

                let state = env_on.state()?;
                state
                    .plugins
                    .bus
                    .listen(&env_on.plugin_id, &sender_id, &name);
                Ok(())
            },
        )?,
    )?;

    lua.globals().raw_set("plugins", plugins_table)?;
    Ok(())
}
