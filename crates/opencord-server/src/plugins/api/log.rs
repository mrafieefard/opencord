//! `log.info`, `log.warn`, `log.error`, and `print` as `log.info`
//! (spec §10.3). Arguments are joined with tabs, as `print` joins them.

use std::sync::Arc;

use mlua::{Function, Value, Variadic};
use opencord_plugins::limits::LOG_LINE_MAX_CHARS;
use opencord_plugins::sandbox::Vm;
use opencord_proto::v1 as proto;

use super::function;
use crate::plugins::runtime::PluginEnv;

pub fn install(vm: &Vm, env: &Arc<PluginEnv>) -> mlua::Result<()> {
    let lua = &vm.lua;
    let tostring: Function = lua.globals().get("tostring")?;
    let table = lua.create_table()?;
    for (name, level) in [
        ("info", proto::PluginLogLevel::Info),
        ("warn", proto::PluginLogLevel::Warn),
        ("error", proto::PluginLogLevel::Error),
    ] {
        let log = env.log.clone();
        let tostring = tostring.clone();
        table.raw_set(
            name,
            function(lua, move |_, args: Variadic<Value>| {
                log.push(level, &joined(&tostring, args)?);
                Ok(())
            })?,
        )?;
    }
    table.set_readonly(true);
    lua.globals().raw_set("log", table)?;

    let log = env.log.clone();
    vm.on_print(Box::new(move |line| {
        log.push(proto::PluginLogLevel::Info, &line);
    }));
    Ok(())
}

/// The arguments through `tostring`, joined by tabs, read no further than a
/// log line keeps.
fn joined(tostring: &Function, args: Variadic<Value>) -> mlua::Result<String> {
    let mut line = String::new();
    for (index, value) in args.into_iter().enumerate() {
        if index > 0 {
            line.push('\t');
        }
        let text = match value {
            Value::String(text) => text,
            other => tostring.call::<mlua::String>(other)?,
        };
        let bytes = text.as_bytes();
        let room = LOG_LINE_MAX_CHARS.saturating_mul(4);
        line.push_str(&String::from_utf8_lossy(&bytes[..bytes.len().min(room)]));
        if line.len() > room {
            break;
        }
    }
    Ok(line)
}
