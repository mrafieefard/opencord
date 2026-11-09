//! `opencord`: the plugin's id and version, its permissions, and its
//! event handlers (spec §10.3).

use std::sync::Arc;

use mlua::{Function, Lua, Table, Value};

use super::{function, raise, string_arg};
use crate::plugins::permissions::PluginPermission;
use crate::plugins::runtime::PluginEnv;

/// Where `opencord.on` keeps handlers: event name → array of functions.
pub const HANDLERS: &str = "opencord.handlers";

/// Every event `opencord.on` listens to (§10.4), the later milestones'
/// included. Commands aren't here: each has its own function (§26.2).
pub const KNOWN_EVENTS: &[&str] = &[
    "start",
    "stop",
    "update",
    "settings_change",
    "permission_change",
    "message_create",
    "message_update",
    "message_delete",
    "typing_start",
    "channel_create",
    "channel_update",
    "channel_delete",
    "plugin_channel_create",
    "plugin_channel_delete",
    "role_create",
    "role_update",
    "role_delete",
    "member_join",
    "member_leave",
    "member_update",
    "member_unban",
    "presence_update",
    "invite_create",
    "invite_revoke",
    "server_update",
    "voice_settings_update",
    "upload_settings_update",
    "voice_state_update",
    "stream_create",
    "stream_update",
    "stream_delete",
    "stream_viewers_update",
    "soundboard_play",
    "soundboard_sound_create",
    "soundboard_sound_update",
    "soundboard_sound_delete",
    "music_now_playing",
    "music_queue_update",
    "music_notice",
    "view_open",
    "view_close",
    "activity_start",
    "activity_end",
    "file_uploaded",
];

/// The longest event name `opencord.on` takes, in bytes.
const EVENT_NAME_MAX: usize = 64;

pub fn install(lua: &Lua, env: &Arc<PluginEnv>) -> mlua::Result<()> {
    let handlers = lua.create_table()?;
    lua.set_named_registry_value(HANDLERS, &handlers)?;

    let opencord = lua.create_table()?;
    opencord.raw_set("id", env.plugin_id.as_str())?;
    opencord.raw_set("version", env.version.as_str())?;

    let has = env.clone();
    opencord.raw_set(
        "has_permission",
        function(lua, move |_, name: Value| {
            let name = string_arg(name, "the permission")?;
            let permission = PluginPermission::from_name(&name).ok_or_else(|| {
                raise(format!(
                    "INVALID_ARGUMENT: there's no permission named {name}"
                ))
            })?;
            Ok(has.has(permission))
        })?,
    )?;

    let list = handlers.clone();
    opencord.raw_set(
        "on",
        function(lua, move |lua, (event, handler): (Value, Value)| {
            let (event, handler) = event_and_handler(event, handler, "opencord.on")?;
            if !KNOWN_EVENTS.contains(&event.as_str()) {
                return Err(raise(format!("INVALID_ARGUMENT: no event called {event}")));
            }
            let named = match list.raw_get::<Option<Table>>(event.as_str())? {
                Some(named) => named,
                None => {
                    let named = lua.create_table()?;
                    list.raw_set(event, &named)?;
                    named
                }
            };
            named.raw_push(handler)
        })?,
    )?;

    opencord.raw_set(
        "off",
        function(lua, move |_, (event, handler): (Value, Value)| {
            let (event, handler) = event_and_handler(event, handler, "opencord.off")?;
            let Some(named) = handlers.raw_get::<Option<Table>>(event.as_str())? else {
                return Ok(false);
            };
            for index in 1..=named.raw_len() {
                if named.raw_get::<Value>(index)? == Value::Function(handler.clone()) {
                    named.raw_remove(index)?;
                    return Ok(true);
                }
            }
            Ok(false)
        })?,
    )?;

    opencord.set_readonly(true);
    lua.globals().raw_set("opencord", opencord)
}

fn event_and_handler(event: Value, handler: Value, call: &str) -> mlua::Result<(String, Function)> {
    let event = string_arg(event, "the event")?;
    if event.is_empty()
        || event.len() > EVENT_NAME_MAX
        || !event
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        return Err(raise(format!(
            "INVALID_ARGUMENT: {call} takes an event name such as \"message_create\""
        )));
    }
    match handler {
        Value::Function(handler) => Ok((event, handler)),
        _ => Err(raise(format!("INVALID_ARGUMENT: {call} takes a function"))),
    }
}

/// The handlers registered for `event`, in the order they were added.
pub fn handlers_for(lua: &Lua, event: &str) -> mlua::Result<Vec<Function>> {
    let handlers: Table = lua.named_registry_value(HANDLERS)?;
    let Some(named) = handlers.raw_get::<Option<Table>>(event)? else {
        return Ok(Vec::new());
    };
    named.sequence_values::<Function>().collect()
}
