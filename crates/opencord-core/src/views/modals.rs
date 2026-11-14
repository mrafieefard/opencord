//! Named modals (§11.4, §12.5): `app.modals[name] = fn(state, props)` and
//! `app.modal`.
//!
//! The stack lives in Lua, in the registry, with each modal's props and
//! the results its askers get, so no Lua value is held by Rust. The view's
//! thread reads it at each redraw (`super::draw`). A modal is one opening
//! of a name: it keeps its serial while it's moved to the top by opening
//! the name again, and a handle acts only on the opening it came from.
//! Entries leave on close, `close_all` and a dismiss, and with the VM.

use std::sync::Arc;

use mlua::{ChunkMode, Function, Lua, Table, Value};
use opencord_plugins::limits::VIEW_MAX_MODALS;
use opencord_plugins::lua_api::{async_function, function, raise};

use super::api_app::{Phase, Shared, is_name};
use super::dialogs::{Waiting, check_wait};

/// Registry keys: the stack (bottom to top), `app.modals` and the
/// function a `modal_dismiss` input runs.
const STACK_KEY: &str = "opencord.view.modal_stack";
const FUNCTIONS_KEY: &str = "opencord.view.modals";
const DISMISS_KEY: &str = "opencord.view.modal_dismiss";

/// Builds `app.modal` in Lua around the Rust checks. Globals are read once,
/// here, before any plugin code runs.
const MODALS: &str = r#"
local changed, check_name, may_wait, wait, wake, max_open = ...
local error, type, rawget, insert, remove, freeze = error, type, rawget, table.insert, table.remove, table.freeze
local modals, modal, stack = {}, {}, {}
local askers, answers = {}, {}
local serial = 0
local function find(name)
    for index, entry in stack do
        if entry.name == name then
            return index, entry
        end
    end
    return nil, nil
end
local function close_at(index, result)
    local entry = remove(stack, index)
    if askers[entry.serial] then
        answers[entry.serial] = result
        wake(entry.serial)
    end
end
local function open(what, name, props)
    changed(what)
    check_name(name)
    if type(rawget(modals, name)) ~= "function" then
        error("INVALID_ARGUMENT: app.modals has no function called " .. name, 0)
    end
    local index, entry = find(name)
    if entry then
        remove(stack, index)
        entry.props = props
    else
        if #stack >= max_open then
            error("PLUGIN_LIMIT: at most " .. max_open .. " modals can be open at once", 0)
        end
        serial += 1
        entry = { name = name, props = props, serial = serial }
    end
    insert(stack, entry)
    return entry
end
local function handle(entry)
    local name, mine = entry.name, entry.serial
    local function current()
        local index, found = find(name)
        if found and found.serial == mine then
            return index, found
        end
        return nil, nil
    end
    return freeze({
        name = name,
        close = function(_, result)
            changed("handle:close")
            local index = current()
            if index then
                close_at(index, result)
            end
        end,
        update = function(_, props)
            changed("handle:update")
            local _, found = current()
            if found then
                found.props = props
            end
        end,
    })
end
function modal.open(name, props)
    return handle(open("app.modal.open", name, props))
end
function modal.ask(name, props)
    may_wait("app.modal.ask")
    local mine = open("app.modal.ask", name, props).serial
    askers[mine] = (askers[mine] or 0) + 1
    wait(mine)
    local result = answers[mine]
    local left = askers[mine] - 1
    if left == 0 then
        askers[mine], answers[mine] = nil, nil
    else
        askers[mine] = left
    end
    return result
end
function modal.close(arg)
    changed("app.modal.close")
    if type(arg) == "string" then
        local index = find(arg)
        if index then
            close_at(index, nil)
        end
    elseif #stack > 0 then
        close_at(#stack, arg)
    end
end
function modal.close_all()
    changed("app.modal.close_all")
    while #stack > 0 do
        close_at(#stack, nil)
    end
end
function modal.is_open(name)
    return type(name) == "string" and find(name) ~= nil
end
local function dismiss(name)
    changed("a dismiss")
    local index = find(name)
    if index then
        close_at(index, nil)
    end
end
return freeze(modal), modals, stack, dismiss
"#;

/// Sets `app.modal` and `app.modals` on `app`. Runs Lua, so the caller has
/// a budget running.
pub(super) fn install(lua: &Lua, app: &Table, shared: &Arc<Shared>) -> mlua::Result<()> {
    let marker = shared.clone();
    let changed = function(lua, move |_, what: String| {
        if marker.phase() == Phase::Rendering {
            return Err(raise(format!(
                "FORBIDDEN: {what} can't run while render draws; call it from a handler"
            )));
        }
        marker.mark_dirty();
        Ok(())
    })?;
    let check_name = function(lua, |_, name: Value| match &name {
        Value::String(name) if is_name(&name.as_bytes()) => Ok(()),
        _ => Err(raise(
            "INVALID_ARGUMENT: a modal name is 1 to 64 characters of a-z, 0-9 and _ . : -",
        )),
    })?;
    let checker = shared.clone();
    let may_wait = function(lua, move |_, what: String| check_wait(&checker, &what))?;
    let waiter = shared.clone();
    let wait = async_function(lua, move |_, serial: u64| {
        let shared = waiter.clone();
        async move {
            let waiting = Waiting::begin(&shared, "app.modal.ask", Some(serial))?;
            waiting.answer().await;
            Ok(())
        }
    })?;
    let waker = shared.clone();
    let wake = function(lua, move |_, serial: u64| {
        waker.waits.wake(serial);
        Ok(())
    })?;

    let (modal, modals, stack, dismiss): (Table, Table, Table, Function) = lua
        .load(MODALS)
        .set_name("=app.modal")
        .set_mode(ChunkMode::Text)
        .call((changed, check_name, may_wait, wait, wake, VIEW_MAX_MODALS))?;
    lua.set_named_registry_value(STACK_KEY, stack)?;
    lua.set_named_registry_value(FUNCTIONS_KEY, &modals)?;
    lua.set_named_registry_value(DISMISS_KEY, dismiss)?;
    app.raw_set("modal", modal)?;
    app.raw_set("modals", modals)
}

/// An open modal, as the stack holds it.
pub(super) struct OpenModal {
    pub(super) name: String,
    pub(super) serial: u64,
    pub(super) props: Value,
    /// `app.modals[name]` now, if it's still a function.
    pub(super) function: Option<Function>,
}

/// The open modals, bottom to top. Reads tables only; runs no Lua.
pub(super) fn open_modals(lua: &Lua) -> mlua::Result<Vec<OpenModal>> {
    let stack: Table = lua.named_registry_value(STACK_KEY)?;
    let functions: Table = lua.named_registry_value(FUNCTIONS_KEY)?;
    let mut open = Vec::new();
    for entry in stack.sequence_values::<Table>() {
        let entry = entry?;
        let name: String = entry.raw_get("name")?;
        let function = match functions.raw_get::<Value>(name.as_str())? {
            Value::Function(function) => Some(function),
            _ => None,
        };
        open.push(OpenModal {
            serial: entry.raw_get("serial")?,
            props: entry.raw_get("props")?,
            name,
            function,
        });
    }
    Ok(open)
}

/// What a `modal_dismiss` input runs, as a handler: closes the named modal
/// with no result.
pub(super) fn dismisser(lua: &Lua) -> mlua::Result<Function> {
    lua.named_registry_value(DISMISS_KEY)
}

#[cfg(test)]
#[path = "modals_tests.rs"]
mod tests;
