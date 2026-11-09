//! A running command: the `cmd` table its function gets, the `ac` table an
//! option's autocomplete function gets, and where their answers go (§26.2).

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use mlua::{Lua, Table, Value, Variadic};
use opencord_plugins::sandbox::Vm;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinSet;

use super::PluginEvent;
use super::api::{self, commands as registry, raise};
use super::runtime::{Budgeted, PluginEnv, Report, failed, payload_value};

/// A plugin's answer to the member waiting on a command.
pub enum CommandAnswer {
    Reply {
        content: String,
        only_you: bool,
    },
    /// The command's function raised an error.
    Failed,
}

/// Choices an `ac:suggest` may offer.
const SUGGESTIONS_MAX: usize = 25;

/// The one answer a command may give.
struct CommandCall {
    waiting: Mutex<Option<oneshot::Sender<CommandAnswer>>>,
}

impl CommandCall {
    fn new(waiting: Option<oneshot::Sender<CommandAnswer>>) -> Arc<Self> {
        Arc::new(Self {
            waiting: Mutex::new(waiting),
        })
    }

    fn take(&self) -> Option<oneshot::Sender<CommandAnswer>> {
        self.waiting
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }

    /// The function raised: the member hears at once, if nothing was said.
    fn fail(&self) {
        if let Some(sender) = self.take() {
            // The member may have stopped waiting; nothing is left to tell.
            let _ = sender.send(CommandAnswer::Failed);
        }
    }
}

/// Runs the command's own function.
pub(super) fn run_command(
    vm: &Vm,
    env: &Arc<PluginEnv>,
    limit: Duration,
    event: PluginEvent,
    running: &mut JoinSet<()>,
    reports: &mpsc::UnboundedSender<Report>,
) {
    let name = text_of(&event.payload, "name");
    // Without a function the sender drops here, and its waiter hears.
    let handler = match registry::command_function(&vm.lua, name) {
        Ok(Some(handler)) => handler,
        Ok(None) => return,
        Err(error) => return failed(env, reports, "command", error, limit),
    };
    let call = CommandCall::new(event.command);
    let argument = match command_table(&vm.lua, &event.payload, &call) {
        Ok(argument) => argument,
        Err(error) => {
            call.fail();
            return failed(env, reports, "command", error, limit);
        }
    };
    let budgeted = Budgeted::new(
        env.budget.clone(),
        limit,
        handler.call_async::<()>(argument),
    );
    let (env, reports) = (env.clone(), reports.clone());
    running.spawn(async move {
        if let Err(error) = budgeted.await {
            failed(&env, &reports, "command", error, limit);
            call.fail();
        }
    });
}

/// Runs the option's own autocomplete function.
pub(super) fn run_autocomplete(
    vm: &Vm,
    env: &Arc<PluginEnv>,
    limit: Duration,
    event: PluginEvent,
    running: &mut JoinSet<()>,
    reports: &mpsc::UnboundedSender<Report>,
) {
    let (name, option) = (
        text_of(&event.payload, "name"),
        text_of(&event.payload, "option"),
    );
    let handler = match registry::autocomplete_function(&vm.lua, name, option) {
        Ok(Some(handler)) => handler,
        Ok(None) => return,
        Err(error) => return failed(env, reports, "command_autocomplete", error, limit),
    };
    let argument = match autocomplete_table(&vm.lua, &event.payload, event.reply) {
        Ok(argument) => argument,
        Err(error) => return failed(env, reports, "command_autocomplete", error, limit),
    };
    let budgeted = Budgeted::new(
        env.budget.clone(),
        limit,
        handler.call_async::<()>(argument),
    );
    let (env, reports) = (env.clone(), reports.clone());
    running.spawn(async move {
        if let Err(error) = budgeted.await {
            failed(&env, &reports, "command_autocomplete", error, limit);
        }
    });
}

fn text_of<'a>(payload: &'a serde_json::Value, key: &str) -> &'a str {
    payload
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
}

fn payload_table(lua: &Lua, payload: &serde_json::Value) -> mlua::Result<Table> {
    match payload_value(lua, payload)? {
        Value::Table(table) => Ok(table),
        _ => lua.create_table(),
    }
}

/// `cmd`: the command's name, options, user and channel, and `cmd:reply`.
fn command_table(
    lua: &Lua,
    payload: &serde_json::Value,
    call: &Arc<CommandCall>,
) -> mlua::Result<Table> {
    let table = payload_table(lua, payload)?;
    let call = call.clone();
    let reply = api::function(lua, move |_lua, args: Variadic<Value>| {
        let (content, options) = method_args(args)?;
        let content = reply_content(content)?;
        let only_you = only_you_option(options)?;
        let sender = call
            .take()
            .ok_or_else(|| raise("INVALID_ARGUMENT: already answered"))?;
        if sender
            .send(CommandAnswer::Reply { content, only_you })
            .is_err()
        {
            tracing::debug!("a command's reply came after its member stopped waiting");
        }
        Ok(())
    })?;
    table.raw_set("reply", reply)?;
    Ok(table)
}

/// `(first, second)` of a call made either as `cmd:f(a, b)` or `cmd.f(a, b)`.
fn method_args(args: Variadic<Value>) -> mlua::Result<(Option<Value>, Option<Value>)> {
    let mut iter = args.into_iter();
    let first = iter.next();
    Ok(match first {
        Some(Value::Table(ref table)) if table.contains_key("name")? => (iter.next(), iter.next()),
        other => (other, iter.next()),
    })
}

fn reply_content(value: Option<Value>) -> mlua::Result<String> {
    let value = value.ok_or_else(|| raise("INVALID_ARGUMENT: content must be a string"))?;
    let content = api::string_arg(value, "content")?;
    opencord_common::validation::message_content(&content).map_err(|_| {
        raise(format!(
            "INVALID_ARGUMENT: the reply must be 1 to {} characters",
            opencord_common::limits::MESSAGE_MAX_CHARS
        ))
    })
}

fn only_you_option(options: Option<Value>) -> mlua::Result<bool> {
    match options {
        None | Some(Value::Nil) => Ok(false),
        Some(Value::Table(options)) => match options.raw_get::<Value>("only_you")? {
            Value::Nil => Ok(false),
            Value::Boolean(only_you) => Ok(only_you),
            _ => Err(raise("INVALID_ARGUMENT: only_you must be a boolean")),
        },
        _ => Err(raise("INVALID_ARGUMENT: options must be a table")),
    }
}

/// `ac`: the command, the option, what's typed so far, and `ac:suggest`.
fn autocomplete_table(
    lua: &Lua,
    payload: &serde_json::Value,
    reply: Option<oneshot::Sender<serde_json::Value>>,
) -> mlua::Result<Table> {
    let table = payload_table(lua, payload)?;
    let slot = Mutex::new(reply);
    let suggest = api::function(lua, move |_lua, args: Variadic<Value>| {
        let (choices, _) = method_args(args)?;
        let choices = suggestions(choices)?;
        let sender = slot.lock().unwrap_or_else(PoisonError::into_inner).take();
        let sender = sender.ok_or_else(|| raise("INVALID_ARGUMENT: already answered"))?;
        if sender
            .send(serde_json::json!({ "choices": choices }))
            .is_err()
        {
            tracing::debug!("autocomplete's choices came after its member stopped waiting");
        }
        Ok(())
    })?;
    table.raw_set("suggest", suggest)?;
    Ok(table)
}

fn suggestions(value: Option<Value>) -> mlua::Result<Vec<serde_json::Value>> {
    let Some(Value::Table(list)) = value else {
        return Err(raise("INVALID_ARGUMENT: choices must be an array table"));
    };
    let mut choices = Vec::new();
    for item in list.sequence_values::<Value>() {
        let Value::Table(item) = item? else {
            return Err(raise("INVALID_ARGUMENT: choice must be a table"));
        };
        choices.push(suggestion(&item)?);
    }
    if choices.len() > SUGGESTIONS_MAX {
        return Err(raise("INVALID_ARGUMENT: at most 25 choices"));
    }
    Ok(choices)
}

fn suggestion(item: &Table) -> mlua::Result<serde_json::Value> {
    let name: String = item
        .get("name")
        .or_else(|_| item.get(1))
        .map_err(|_| raise("INVALID_ARGUMENT: choice name required"))?;
    let value: String = item
        .get("value")
        .or_else(|_| item.get(2))
        .map_err(|_| raise("INVALID_ARGUMENT: choice value required"))?;
    if !(1..=100).contains(&name.chars().count()) || !(1..=100).contains(&value.chars().count()) {
        return Err(raise(
            "INVALID_ARGUMENT: choice name and value must be 1 to 100 characters",
        ));
    }
    Ok(serde_json::json!({ "name": name, "value": value }))
}
