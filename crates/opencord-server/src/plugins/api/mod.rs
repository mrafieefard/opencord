//! The Lua API server scripts call (spec §10.3). Each module installs its
//! globals with `install(lua, env)`; [`install`] calls them all.
//!
//! Errors a plugin may catch are raised with [`raise`] and reach Lua as the
//! plain string `"<CODE>: <message>"` through [`function`] and
//! [`async_function`]: an `Err` from a bare mlua callback would reach Lua
//! as userdata instead.

pub mod commands;
pub mod db;
pub mod files;
pub mod guild;
pub mod http;
pub mod log;
pub mod messages;
pub mod plugin;
pub mod voice;

use std::fmt;
use std::future::Future;
use std::sync::Arc;

use mlua::{ChunkMode, FromLuaMulti, Function, IntoLuaMulti, Lua, MaybeSend, MultiValue, Value};
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
    #[cfg(test)]
    tests::install(&vm.lua, env)?;
    Ok(())
}

/// An error raised into Lua as its text alone.
#[derive(Debug)]
pub struct Raise(pub String);

impl fmt::Display for Raise {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Raise {}

/// An error the plugin sees as `message`, which starts with its code:
/// `raise("INVALID_ARGUMENT: name must be a string")`.
pub fn raise(message: impl Into<String>) -> mlua::Error {
    mlua::Error::external(Raise(message.into()))
}

/// The text of a [`raise`]d error under mlua's wrappers.
fn raised(error: &mlua::Error) -> Option<&str> {
    match error {
        mlua::Error::ExternalError(cause) => cause.downcast_ref::<Raise>().map(|r| r.0.as_str()),
        mlua::Error::CallbackError { cause, .. } | mlua::Error::WithContext { cause, .. } => {
            raised(cause)
        }
        _ => None,
    }
}

/// Turns `(false, message)` from the Rust half into `error(message, 0)`.
const CHECK: &str = "local ok = ...
if not ok then
    error((select(2, ...)), 0)
end
return select(2, ...)";
const WRAPPER: &str = "return check(inner(...))";
const CHECK_KEY: &str = "opencord.check";

fn answer<R: IntoLuaMulti>(lua: &Lua, result: mlua::Result<R>) -> mlua::Result<MultiValue> {
    match result {
        Ok(values) => {
            let mut values = values.into_lua_multi(lua)?;
            values.push_front(Value::Boolean(true));
            Ok(values)
        }
        Err(error) => match raised(&error) {
            Some(message) => (false, message).into_lua_multi(lua),
            None => Err(error),
        },
    }
}

/// Wraps `inner` in Lua. Only compiles, never runs Lua, so it works
/// outside a budget too.
fn raising(lua: &Lua, inner: Function) -> mlua::Result<Function> {
    let check = match lua.named_registry_value::<Option<Function>>(CHECK_KEY)? {
        Some(check) => check,
        None => {
            let env = lua.create_table()?;
            env.raw_set("error", lua.globals().get::<Function>("error")?)?;
            env.raw_set("select", lua.globals().get::<Function>("select")?)?;
            let check = compiled(lua, CHECK, env)?;
            lua.set_named_registry_value(CHECK_KEY, &check)?;
            check
        }
    };
    let env = lua.create_table()?;
    env.raw_set("check", check)?;
    env.raw_set("inner", inner)?;
    compiled(lua, WRAPPER, env)
}

fn compiled(lua: &Lua, source: &str, env: mlua::Table) -> mlua::Result<Function> {
    lua.load(source)
        .set_name("=opencord")
        .set_mode(ChunkMode::Text)
        .set_environment(env)
        .into_function()
}

/// A Lua function for the API whose [`raise`]d errors reach Lua as strings.
pub fn function<A, R, F>(lua: &Lua, f: F) -> mlua::Result<Function>
where
    A: FromLuaMulti,
    R: IntoLuaMulti,
    F: Fn(&Lua, A) -> mlua::Result<R> + MaybeSend + 'static,
{
    let inner = lua.create_function(move |lua, args: A| answer(lua, f(lua, args)))?;
    raising(lua, inner)
}

/// [`function`] for calls that wait. Wrap the waiting in
/// `opencord_plugins::sandbox::paused`, so it isn't counted as running.
pub fn async_function<A, R, F, FR>(lua: &Lua, f: F) -> mlua::Result<Function>
where
    A: FromLuaMulti,
    R: IntoLuaMulti,
    F: Fn(Lua, A) -> FR + MaybeSend + 'static,
    FR: Future<Output = mlua::Result<R>> + MaybeSend + 'static,
{
    let inner = lua.create_async_function(move |lua, args: A| {
        let call = f(lua.clone(), args);
        async move { answer(&lua, call.await) }
    })?;
    raising(lua, inner)
}

/// [`async_function`] for calls that pass a Lua error on: `Ok(Err(value))`
/// raises `value` itself, as `error(value, 0)` does.
pub fn async_function_passing<A, R, F, FR>(lua: &Lua, f: F) -> mlua::Result<Function>
where
    A: FromLuaMulti,
    R: IntoLuaMulti,
    F: Fn(Lua, A) -> FR + MaybeSend + 'static,
    FR: Future<Output = mlua::Result<Result<R, Value>>> + MaybeSend + 'static,
{
    let inner = lua.create_async_function(move |lua, args: A| {
        let call = f(lua.clone(), args);
        async move {
            match call.await {
                Ok(Err(value)) => (false, value).into_lua_multi(&lua),
                Ok(Ok(values)) => answer(&lua, Ok(values)),
                Err(error) => answer::<()>(&lua, Err(error)),
            }
        }
    })?;
    raising(lua, inner)
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

#[cfg(test)]
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
