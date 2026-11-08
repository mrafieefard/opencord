// SPDX-License-Identifier: MPL-2.0

//! Plugin bus API (plan Task 20, design 2026-11-07 §10.3).

use std::sync::Arc;

use mlua::{Function, Lua, LuaSerdeExt, Table, Value};

use super::PluginEnv;
use crate::plugins::permissions::PluginPermission;

pub const BUS_LISTENERS: &str = "__opencord_bus_listeners";

pub fn install(lua: &Lua, env: &Arc<PluginEnv>) -> mlua::Result<()> {
    lua.set_named_registry_value(BUS_LISTENERS, lua.create_table()?)?;

    let plugins_table = lua.create_table()?;

    plugins_table.set(
        "emit",
        lua.create_function({
            let env = env.clone();
            move |lua, (name, data): (String, Value)| {
                if !env.has(PluginPermission::TalkToPlugins) {
                    return Err(mlua::Error::runtime(
                        "PERMISSION_DENIED: missing talk_to_plugins",
                    ));
                }
                let data_json: serde_json::Value = lua.from_value(data)?;
                let data_bytes = serde_json::to_vec(&data_json)
                    .map_err(|e| mlua::Error::runtime(format!("INVALID_ARGUMENT: {e}")))?;
                if data_bytes.len() > 64 * 1024 {
                    return Err(mlua::Error::runtime(
                        "INVALID_ARGUMENT: data is over 64 KiB",
                    ));
                }
                let state = env.state()?;
                state
                    .plugins
                    .bus
                    .emit(&state.plugins, &env.plugin_id, &name, data_json);
                Ok(())
            }
        })?,
    )?;

    plugins_table.set(
        "on",
        lua.create_function({
            let env = env.clone();
            move |lua, (sender_id, name, handler): (String, String, Function)| {
                if !env.has(PluginPermission::TalkToPlugins) {
                    return Err(mlua::Error::runtime(
                        "PERMISSION_DENIED: missing talk_to_plugins",
                    ));
                }
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

                let state = env.state()?;
                state.plugins.bus.listen(&env.plugin_id, &sender_id, &name);
                Ok(())
            }
        })?,
    )?;

    lua.globals().set("plugins", plugins_table)?;
    Ok(())
}
