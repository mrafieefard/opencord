//! A running command: the `cmd` table its function gets, the `ac` table an
//! option's autocomplete function gets, and where their answers go (§26.2).

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use mlua::{Lua, Table, Value, Variadic};
use opencord_plugins::sandbox::{Vm, paused};
use opencord_proto::v1 as proto;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinSet;

use super::PluginEvent;
use super::api::{self, commands as registry, raise};
use super::runtime::{Budgeted, PluginEnv, Report, failed, payload_value};
use crate::commands::thinking::{self, GaveUp, ThinkingId, TooLate};
use crate::error::ApiError;

/// A plugin's answer to the member waiting on a command.
pub enum CommandAnswer {
    Reply {
        content: String,
        only_you: bool,
    },
    /// The plugin needs longer; `started` hears once the thinking line is up.
    Defer {
        only_you: bool,
        started: oneshot::Sender<Result<ThinkingId, ApiError>>,
    },
    /// The command's function raised an error.
    Failed,
}

/// Choices an `ac:suggest` may offer.
const SUGGESTIONS_MAX: usize = 25;

enum CallState {
    /// The member's request is waiting for the first answer.
    Waiting(oneshot::Sender<CommandAnswer>),
    /// `cmd:defer` is waiting for the thinking line.
    Deferring,
    Thinking(ThinkingId),
    Done,
}

/// The one answer a command may give.
struct CommandCall {
    state: Mutex<CallState>,
    command: String,
    started: Instant,
}

impl CommandCall {
    fn new(command: &str, waiting: Option<oneshot::Sender<CommandAnswer>>) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(waiting.map_or(CallState::Done, CallState::Waiting)),
            command: command.to_owned(),
            started: Instant::now(),
        })
    }

    /// The answer came after the wait: the plugin hears and its log says so.
    fn too_late(&self, env: &PluginEnv) -> mlua::Error {
        env.log.push(
            proto::PluginLogLevel::Warn,
            &format!(
                "/{} answered after {}, too late",
                self.command,
                waited_text(self.started.elapsed())
            ),
        );
        raise("TIMEOUT: too late to reply")
    }

    fn replace(&self, next: CallState) -> CallState {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        std::mem::replace(&mut *state, next)
    }

    /// The function raised: the member hears at once, if nothing was said.
    fn fail(&self, env: &Arc<PluginEnv>) {
        match self.replace(CallState::Done) {
            CallState::Waiting(sender) => {
                // The member may have stopped waiting; nothing is left to tell.
                let _ = sender.send(CommandAnswer::Failed);
            }
            CallState::Thinking(id) => {
                if let Some(state) = env.state.upgrade() {
                    env.server.spawn(async move {
                        thinking::give_up(&state, id, GaveUp::RanIntoError).await;
                    });
                }
            }
            CallState::Deferring | CallState::Done => {}
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
    let call = CommandCall::new(name, event.command);
    let argument = match command_table(&vm.lua, &event.payload, &call, env) {
        Ok(argument) => argument,
        Err(error) => {
            call.fail(env);
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
            call.fail(&env);
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

/// How long a command took, as its log line says it.
fn waited_text(waited: Duration) -> String {
    let seconds = waited.as_secs();
    if seconds < 60 {
        format!("{:.1} s", waited.as_secs_f64())
    } else {
        format!("{} min {} s", seconds / 60, seconds % 60)
    }
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

/// `cmd`: the command's name, options, user and channel, `cmd:reply` and
/// `cmd:defer`.
fn command_table(
    lua: &Lua,
    payload: &serde_json::Value,
    call: &Arc<CommandCall>,
    env: &Arc<PluginEnv>,
) -> mlua::Result<Table> {
    let table = payload_table(lua, payload)?;
    let (reply_call, reply_env) = (call.clone(), env.clone());
    let reply = api::async_function(lua, move |_lua, args: Variadic<Value>| {
        let (call, env) = (reply_call.clone(), reply_env.clone());
        async move { reply(&call, &env, args).await }
    })?;
    table.raw_set("reply", reply)?;
    let (defer_call, defer_env) = (call.clone(), env.clone());
    let defer = api::async_function(lua, move |_lua, args: Variadic<Value>| {
        let (call, env) = (defer_call.clone(), defer_env.clone());
        async move { defer(&call, &env, args).await }
    })?;
    table.raw_set("defer", defer)?;
    Ok(table)
}

async fn reply(
    call: &CommandCall,
    env: &Arc<PluginEnv>,
    args: Variadic<Value>,
) -> mlua::Result<()> {
    let (content, options) = method_args(args)?;
    let content = reply_content(content)?;
    let only_you = only_you_option(options)?;
    match call.replace(CallState::Done) {
        CallState::Waiting(sender) => sender
            .send(CommandAnswer::Reply { content, only_you })
            .map_err(|_| call.too_late(env)),
        CallState::Thinking(id) => {
            let state = env.state()?;
            env.on_server(async move { thinking::answer(&state, id, content).await })
                .await?
                .map_err(|_: TooLate| call.too_late(env))
        }
        other @ CallState::Deferring => {
            call.replace(other);
            Err(raise("INVALID_ARGUMENT: already answered"))
        }
        CallState::Done => Err(raise("INVALID_ARGUMENT: already answered")),
    }
}

async fn defer(
    call: &CommandCall,
    env: &Arc<PluginEnv>,
    args: Variadic<Value>,
) -> mlua::Result<()> {
    let (options, second) = method_args(args)?;
    let only_you = only_you_option(options.or(second))?;
    let sender = match call.replace(CallState::Deferring) {
        CallState::Waiting(sender) => sender,
        other => {
            call.replace(other);
            return Err(raise("INVALID_ARGUMENT: already answered"));
        }
    };
    let (started, line_up) = oneshot::channel();
    if sender
        .send(CommandAnswer::Defer { only_you, started })
        .is_err()
    {
        call.replace(CallState::Done);
        return Err(call.too_late(env));
    }
    match paused(&env.budget, line_up).await {
        Ok(Ok(id)) => {
            call.replace(CallState::Thinking(id));
            Ok(())
        }
        Ok(Err(error)) => {
            call.replace(CallState::Done);
            Err(PluginEnv::api_error(error))
        }
        Err(_) => {
            call.replace(CallState::Done);
            Err(raise("STOPPING: the server is stopping"))
        }
    }
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
        // Past its second the member has typed on: false, and nothing logged.
        Ok(sender
            .send(serde_json::json!({ "choices": choices }))
            .is_ok())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waited_text_reads_well() {
        assert_eq!(waited_text(Duration::from_millis(4_210)), "4.2 s");
        assert_eq!(waited_text(Duration::from_millis(59_940)), "59.9 s");
        assert_eq!(waited_text(Duration::from_secs(60)), "1 min 0 s");
        assert_eq!(waited_text(Duration::from_secs(15 * 60 + 2)), "15 min 2 s");
    }
}
