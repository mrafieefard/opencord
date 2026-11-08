// SPDX-License-Identifier: MPL-2.0

//! Plugin config API (plan Task 20, design 2026-11-07 §10.3).

use std::sync::Arc;

use mlua::{Lua, Value};

use super::PluginEnv;
use crate::plugins::ConfigValue;

pub fn install(lua: &Lua, env: &Arc<PluginEnv>) -> mlua::Result<()> {
    let config_table = lua.create_table()?;

    config_table.set(
        "get",
        lua.create_function({
            let env = env.clone();
            move |lua, key: String| {
                let values = env
                    .config_values
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let Some(val) = values.get(&key) else {
                    return Ok(Value::Nil);
                };
                match val {
                    ConfigValue::Number(n) => Ok(Value::Number(*n)),
                    ConfigValue::Boolean(b) => Ok(Value::Boolean(*b)),
                    ConfigValue::Text(s)
                    | ConfigValue::Secret(s)
                    | ConfigValue::Role(s)
                    | ConfigValue::Channel(s)
                    | ConfigValue::User(s) => Ok(Value::String(lua.create_string(s)?)),
                }
            }
        })?,
    )?;

    lua.globals().set("config", config_table)?;
    Ok(())
}
