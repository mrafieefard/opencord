//! Helpers every Lua API is built with, on the server and in the app's
//! core: errors a plugin may catch, and `require`.
//!
//! Errors a plugin may catch are raised with [`raise`] and reach Lua as the
//! plain string `"<CODE>: <message>"` through [`function`] and
//! [`async_function`]: an `Err` from a bare mlua callback would reach Lua
//! as userdata instead.

use std::fmt;
use std::future::Future;

use mlua::{ChunkMode, FromLuaMulti, Function, IntoLuaMulti, Lua, MaybeSend, MultiValue, Value};

use crate::sandbox::VmError;

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
/// [`crate::sandbox::paused`], so it isn't counted as running.
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

/// The longest name `require` takes, in bytes.
const MODULE_NAME_MAX: usize = 200;

const REQUIRE: &str = r#"
local load = ...
local loaded, loading = {}, {}
local function finish(name, ok, ...)
    loading[name] = nil
    if not ok then
        error((...), 0)
    end
    local module = ...
    if module == nil then
        module = true
    end
    loaded[name] = module
    return module
end
return function(name)
    local module = loaded[name]
    if module ~= nil then
        return module
    end
    local chunk = load(name)
    if loading[name] then
        error("INVALID_ARGUMENT: " .. name .. " requires itself", 0)
    end
    loading[name] = true
    return finish(name, pcall(chunk))
end
"#;

/// Sets the global `require(name)`: it runs `<folder>/<name>.luau` (or
/// `.lua`) once and returns what it returned, every later call the same.
/// `source` gives a file's text by its path in the package.
pub fn install_require(
    lua: &Lua,
    folder: &'static str,
    source: impl Fn(&str) -> Option<String> + MaybeSend + 'static,
) -> mlua::Result<()> {
    let load = function(lua, move |lua, name: Value| {
        let name = module_name(name)?;
        for extension in ["luau", "lua"] {
            let path = format!("{folder}/{name}.{extension}");
            if let Some(text) = source(&path) {
                return lua
                    .load(text)
                    .set_name(format!("={path}"))
                    .set_mode(ChunkMode::Text)
                    .into_function()
                    .map_err(|error| raise(format!("INVALID_ARGUMENT: {}", VmError::from(error))));
            }
        }
        Err(raise(format!(
            "NOT_FOUND: the plugin has no {folder}/{name}.luau or {folder}/{name}.lua"
        )))
    })?;
    let require: Function = lua
        .load(REQUIRE)
        .set_name("=require")
        .set_mode(ChunkMode::Text)
        .call(load)?;
    lua.globals().raw_set("require", require)
}

/// A module name: `/`-separated segments of letters, digits, `_` and `-`,
/// so it can only name a file under the folder `require` reads.
fn module_name(name: Value) -> mlua::Result<String> {
    let bad =
        || raise("INVALID_ARGUMENT: require takes a module name such as \"util\" or \"lib/util\"");
    let Value::String(name) = name else {
        return Err(bad());
    };
    let name = name.to_str().map_err(|_| bad())?.to_owned();
    let fine = !name.is_empty()
        && name.len() <= MODULE_NAME_MAX
        && name.split('/').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        });
    if fine { Ok(name) } else { Err(bad()) }
}
