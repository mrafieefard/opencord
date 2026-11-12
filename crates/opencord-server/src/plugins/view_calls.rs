// SPDX-License-Identifier: MPL-2.0

//! Running a plugin's view handlers: `view_open`, `view_close`, the named
//! events its views send to `ui.on` (plan M2 Task 10) and `file_uploaded`
//! (Task 12).
//!
//! A VM hears each open view's `view_open` once: when its start finishes
//! it announces every view already open, and a `view_open` waiting in the
//! queue for one of them is then skipped. It hears `view_close` only for a
//! view it heard open, and a client's event or upload only while its view
//! is open.
//!
//! Activity sessions' `activity_start` and `activity_end` (plan M3 Task 4)
//! run here too, the same way: each running session's start is heard once,
//! and its end only by a VM that heard its start.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use mlua::{Function, Table, Value};
use opencord_plugins::sandbox::Vm;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use super::PluginEvent;
use super::activities;
use super::api::plugin::handlers_for;
use super::api::ui::{UI_HANDLERS, view_table};
use super::runtime::{Budgeted, PluginEnv, Report, failed, payload_value};
use super::views::ViewInfo;
use crate::state::AppState;

/// The events this module runs.
pub const NAMES: [&str; 6] = [
    "view_open",
    "view_close",
    "view_event",
    "file_uploaded",
    "activity_start",
    "activity_end",
];
/// Past this many, views no longer open (or sessions no longer running)
/// are dropped from what a VM heard.
const HEARD_PRUNE_AT: usize = 256;

/// Delivers `activity_start` for every running session of the plugin, then
/// `view_open` for every view of the plugin open now.
pub(super) fn announce(
    vm: &Vm,
    env: &Arc<PluginEnv>,
    limit: Duration,
    running: &mut JoinSet<()>,
    reports: &mpsc::UnboundedSender<Report>,
) {
    let Ok(state) = env.state() else { return };
    for session in state.plugins.activities.sessions_of(&env.plugin_id) {
        let event = PluginEvent {
            name: "activity_start".to_owned(),
            payload: serde_json::json!({ "session_id": session.id }),
            reply: None,
            command: None,
        };
        dispatch(vm, env, limit, event, running, reports);
    }
    for view in state.plugins.views.of_plugin(&env.plugin_id) {
        let event = PluginEvent {
            name: "view_open".to_owned(),
            payload: serde_json::json!({ "view_id": view.id }),
            reply: None,
            command: None,
        };
        dispatch(vm, env, limit, event, running, reports);
    }
}

/// Runs the handlers of a view event, each in its own task.
pub(super) fn dispatch(
    vm: &Vm,
    env: &Arc<PluginEnv>,
    limit: Duration,
    event: PluginEvent,
    running: &mut JoinSet<()>,
    reports: &mpsc::UnboundedSender<Report>,
) {
    let name = event.name.clone();
    let calls = match calls(vm, env, &event) {
        Ok(calls) => calls,
        Err(error) => return failed(env, reports, &name, error, limit),
    };
    for (handler, arguments) in calls {
        let call = Budgeted::new(
            env.budget.clone(),
            limit,
            handler.call_async::<()>(arguments),
        );
        let (env, reports, name) = (env.clone(), reports.clone(), name.clone());
        running.spawn(async move {
            if let Err(error) = call.await {
                failed(&env, &reports, &name, error, limit);
            }
        });
    }
}

/// The `view_close` events waiting as the plugin stops, as its handlers'
/// arguments, for `stop`'s grace.
pub(super) fn closing(vm: &Vm, env: &Arc<PluginEnv>) -> Vec<Value> {
    env.queue
        .take_named("view_close")
        .iter()
        .filter_map(|event| match close_argument(vm, env, &event.payload) {
            Ok(argument) => argument,
            Err(error) => {
                tracing::warn!(plugin = %env.plugin_id, %error, "a view's close couldn't be built");
                None
            }
        })
        .collect()
}

/// The `activity_end` events waiting as the plugin stops, as its
/// handlers' arguments, for `stop`'s grace.
pub(super) fn ending(vm: &Vm, env: &Arc<PluginEnv>) -> Vec<Value> {
    env.queue
        .take_named("activity_end")
        .iter()
        .filter_map(|event| match end_argument(vm, env, &event.payload) {
            Ok(argument) => argument,
            Err(error) => {
                tracing::warn!(plugin = %env.plugin_id, %error, "a session's end couldn't be built");
                None
            }
        })
        .collect()
}

type Calls = Vec<(Function, mlua::MultiValue)>;

/// Each handler to call for `event`, with its arguments; none when the
/// event is for a view this VM mustn't hear about.
fn calls(vm: &Vm, env: &PluginEnv, event: &PluginEvent) -> mlua::Result<Calls> {
    let lua = &vm.lua;
    match event.name.as_str() {
        "view_open" => {
            let Some(view) = open_view(env, &event.payload) else {
                return Ok(Vec::new());
            };
            if !heard(env, &view) {
                return Ok(Vec::new());
            }
            let argument = lua.create_table()?;
            argument.raw_set("view", view_table(lua, &view_json(env, &view)?)?)?;
            with_each(handlers_for(lua, "view_open")?, &[Value::Table(argument)])
        }
        "view_close" => match close_argument(vm, env, &event.payload)? {
            Some(argument) => with_each(handlers_for(lua, "view_close")?, &[argument]),
            None => Ok(Vec::new()),
        },
        "activity_start" => {
            let Some(state) = env.state().ok() else {
                return Ok(Vec::new());
            };
            let session = event.payload["session_id"]
                .as_i64()
                .and_then(|session_id| state.plugins.activities.session(session_id))
                .filter(|session| session.plugin_id == env.plugin_id);
            let Some(session) = session else {
                return Ok(Vec::new());
            };
            if !heard_start(env, &state, session.id) {
                return Ok(Vec::new());
            }
            let argument = payload_value(lua, &activities::start_payload(&state, &session))?;
            with_each(handlers_for(lua, "activity_start")?, &[argument])
        }
        "activity_end" => match end_argument(vm, env, &event.payload)? {
            Some(argument) => with_each(handlers_for(lua, "activity_end")?, &[argument]),
            None => Ok(Vec::new()),
        },
        "file_uploaded" => {
            let Some(view) = open_view(env, &event.payload).filter(|view| was_heard(env, view))
            else {
                return Ok(Vec::new());
            };
            let argument = lua.create_table()?;
            argument.raw_set("view", view_table(lua, &view_json(env, &view)?)?)?;
            argument.raw_set("file", payload_value(lua, &event.payload["file"])?)?;
            with_each(
                handlers_for(lua, "file_uploaded")?,
                &[Value::Table(argument)],
            )
        }
        _ => {
            let Some(view) = open_view(env, &event.payload) else {
                return Ok(Vec::new());
            };
            let heard = was_heard(env, &view);
            let name = event.payload["name"].as_str().unwrap_or_default();
            let registry: Table = lua.named_registry_value(UI_HANDLERS)?;
            let Some(handlers) = registry.raw_get::<Option<Table>>(name)?.filter(|_| heard) else {
                return Ok(Vec::new());
            };
            let handlers = handlers
                .sequence_values::<Function>()
                .collect::<mlua::Result<_>>()?;
            let view = view_table(lua, &view_json(env, &view)?)?;
            let data = payload_value(lua, &event.payload["data"])?;
            with_each(handlers, &[Value::Table(view), data])
        }
    }
}

/// `{ view, reason }` for a closed view this VM heard open, which it now
/// forgets; `None` for one it never heard.
fn close_argument(
    vm: &Vm,
    env: &PluginEnv,
    payload: &serde_json::Value,
) -> mlua::Result<Option<Value>> {
    let Some(view_id) = payload["view_id"].as_i64() else {
        return Ok(None);
    };
    let was_heard = env
        .views_heard
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&view_id);
    if !was_heard {
        return Ok(None);
    }
    let argument = vm.lua.create_table()?;
    argument.raw_set("view", view_table(&vm.lua, &payload["view"])?)?;
    argument.raw_set("reason", payload["reason"].as_str().unwrap_or_default())?;
    Ok(Some(Value::Table(argument)))
}

/// `{ session, reason }` for an ended session this VM heard start, which
/// it now forgets; `None` for one it never heard.
fn end_argument(
    vm: &Vm,
    env: &PluginEnv,
    payload: &serde_json::Value,
) -> mlua::Result<Option<Value>> {
    let Some(session_id) = payload["session_id"].as_i64() else {
        return Ok(None);
    };
    let was_heard = lock(&env.sessions_heard).remove(&session_id);
    if !was_heard {
        return Ok(None);
    }
    let argument = vm.lua.create_table()?;
    argument.raw_set("session", payload_value(&vm.lua, &payload["session"])?)?;
    argument.raw_set("reason", payload["reason"].as_str().unwrap_or_default())?;
    Ok(Some(Value::Table(argument)))
}

/// Records that this VM heard the session start: false if it already had.
fn heard_start(env: &PluginEnv, state: &AppState, session_id: i64) -> bool {
    let mut heard = lock(&env.sessions_heard);
    if heard.len() >= HEARD_PRUNE_AT {
        // An end dropped from a full queue leaves its id behind.
        heard.retain(|heard_id| state.plugins.activities.session(*heard_id).is_some());
    }
    heard.insert(session_id)
}

fn lock(set: &Mutex<HashSet<i64>>) -> std::sync::MutexGuard<'_, HashSet<i64>> {
    set.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The event's view, if it's still open and the plugin's.
fn open_view(env: &PluginEnv, payload: &serde_json::Value) -> Option<ViewInfo> {
    let view_id = payload["view_id"].as_i64()?;
    let state = env.state().ok()?;
    state
        .plugins
        .views
        .info(view_id)
        .filter(|view| view.plugin_id == env.plugin_id)
}

/// Whether this VM heard `view` open (and not yet close).
fn was_heard(env: &PluginEnv, view: &ViewInfo) -> bool {
    env.views_heard
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .contains(&view.id)
}

/// Records that this VM heard `view` open: false if it already had.
fn heard(env: &PluginEnv, view: &ViewInfo) -> bool {
    let mut heard = env
        .views_heard
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if heard.len() >= HEARD_PRUNE_AT
        && let Ok(state) = env.state()
    {
        // A close dropped from a full queue leaves its id behind.
        heard.retain(|view_id| state.plugins.views.info(*view_id).is_some());
    }
    heard.insert(view.id)
}

fn view_json(env: &PluginEnv, view: &ViewInfo) -> mlua::Result<serde_json::Value> {
    Ok(view.to_json(&*env.state()?))
}

fn with_each(handlers: Vec<Function>, arguments: &[Value]) -> mlua::Result<Calls> {
    Ok(handlers
        .into_iter()
        .map(|handler| (handler, arguments.iter().cloned().collect()))
        .collect())
}
