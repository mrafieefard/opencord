//! `ui`: the plugin's channel types (spec §14.1) and its views (§10.3,
//! plan M2 Task 10).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, PoisonError};
use std::time::{Duration, Instant};

use mlua::{Function, Lua, LuaSerdeExt as _, Table, Value};
use opencord_plugins::limits::{CHANNEL_TYPES_PER_PLUGIN, VIEW_EVENT_MAX_BYTES};
use opencord_proto::v1 as proto;

use super::{async_function, function, id_arg, raise, string_arg, string_arg_opt};
use crate::guild::Guild;
use crate::handlers::channels::valid_type_key;
use crate::plugins::channel_types::{self, ChannelType, TooMany};
use crate::plugins::permissions::PluginPermission;
use crate::plugins::runtime::{PluginEnv, payload_value};
use crate::plugins::views::{self, ViewInfo};

/// The longest name of a channel type, in characters.
const NAME_MAX_CHARS: usize = 60;
/// The longest description, in characters.
const DESCRIPTION_MAX_CHARS: usize = 200;
/// What an icon may be.
const ICON_EXTENSIONS: [&str; 4] = [".png", ".jpg", ".jpeg", ".webp"];

pub fn install(lua: &Lua, env: &Arc<PluginEnv>) -> mlua::Result<()> {
    let ui = lua.create_table()?;
    let register_env = env.clone();
    ui.raw_set(
        "register_channel_type",
        async_function(lua, move |_, (key, spec): (Value, Value)| {
            let env = register_env.clone();
            async move {
                env.require(PluginPermission::CreateChannelTypes)?;
                let key = string_arg(key, "key")?;
                if !valid_type_key(&key) {
                    return Err(raise(
                        "INVALID_ARGUMENT: key must be 1 to 40 of a-z, 0-9 and _",
                    ));
                }
                let channel_type = channel_type(&env, spec)?;
                let state = env.state()?;
                let (plugin_id, registered) = (env.plugin_id.clone(), key.clone());
                env.on_server(async move {
                    channel_types::register(&state, &plugin_id, &registered, channel_type).await
                })
                .await?
                .map_err(|TooMany| {
                    raise(format!(
                        "PLUGIN_LIMIT: at most {CHANNEL_TYPES_PER_PLUGIN} channel types"
                    ))
                })?;
                env.log.push(
                    proto::PluginLogLevel::Action,
                    &format!("registered channel type {key}"),
                );
                Ok(())
            }
        })?,
    )?;
    install_views(lua, env, &ui)?;
    lua.globals().raw_set("ui", ui)
}

/// Where `ui.on` keeps handlers: event name → array of functions.
pub const UI_HANDLERS: &str = "opencord.ui_handlers";
/// `view:send`, shared by every view table.
const VIEW_SEND: &str = "opencord.view_send";
/// The most `ui.on` handlers a VM holds.
const UI_HANDLERS_MAX: usize = 1_000;
/// Skipped views are logged at most this often.
const SKIPS_LOGGED_EVERY: Duration = Duration::from_secs(60);

/// `ui.on`, `ui.views`, `ui.send_place`, `ui.send_role`, `ui.send_all`
/// and `view:send` (§10.3).
fn install_views(lua: &Lua, env: &Arc<PluginEnv>, ui: &Table) -> mlua::Result<()> {
    let handlers = lua.create_table()?;
    lua.set_named_registry_value(UI_HANDLERS, &handlers)?;
    let count = Arc::new(AtomicUsize::new(0));
    ui.raw_set(
        "on",
        function(lua, move |lua, (name, handler): (Value, Value)| {
            let name = event_name(name)?;
            let Value::Function(handler) = handler else {
                return Err(raise("INVALID_ARGUMENT: ui.on takes a function"));
            };
            if count.load(Ordering::Relaxed) >= UI_HANDLERS_MAX {
                return Err(raise(format!(
                    "PLUGIN_LIMIT: at most {UI_HANDLERS_MAX} view event handlers"
                )));
            }
            let named = match handlers.raw_get::<Option<Table>>(name.as_str())? {
                Some(named) => named,
                None => {
                    let named = lua.create_table()?;
                    handlers.raw_set(name, &named)?;
                    named
                }
            };
            named.raw_push(handler)?;
            count.fetch_add(1, Ordering::Relaxed);
            Ok(())
        })?,
    )?;

    let send_env = env.clone();
    let send = function(
        lua,
        move |lua, (view, name, data): (Value, Value, Value)| {
            let view_id = view_id(view)?;
            let (name, data) = event(lua, name, data)?;
            let state = send_env.state()?;
            views::send(&state, &send_env.plugin_id, view_id, &name, &data)
                .map_err(|_| raise("RATE_LIMITED: too many events to this view"))
        },
    )?;
    lua.set_named_registry_value(VIEW_SEND, send)?;

    let list_env = env.clone();
    ui.raw_set(
        "views",
        function(lua, move |lua, place: Value| {
            let place = match place {
                Value::Nil => None,
                place => Some(place_arg(place)?),
            };
            let state = list_env.state()?;
            let list = lua.create_table()?;
            for view in state.plugins.views.of_plugin(&list_env.plugin_id) {
                if place.is_none_or(|channel_id| view.channel_id == channel_id) {
                    list.raw_push(view_table(lua, &view.to_json(&state))?)?;
                }
            }
            Ok(list)
        })?,
    )?;

    let place_env = env.clone();
    ui.raw_set(
        "send_place",
        function(
            lua,
            move |lua, (place, name, data): (Value, Value, Value)| {
                let channel_id = place_arg(place)?;
                broadcast(lua, &place_env, name, data, |_, view| {
                    view.channel_id == channel_id
                })
            },
        )?,
    )?;

    let role_env = env.clone();
    ui.raw_set(
        "send_role",
        function(
            lua,
            move |lua, (role, name, data): (Value, Value, Value)| {
                let role_id = id_arg(role, "role")?;
                broadcast(lua, &role_env, name, data, |guild, view| {
                    guild.meta.everyone_role_id == role_id
                        || guild
                            .members
                            .get(&view.user_id)
                            .is_some_and(|member| member.role_ids.contains(&role_id))
                })
            },
        )?,
    )?;

    let all_env = env.clone();
    ui.raw_set(
        "send_all",
        function(lua, move |lua, (name, data): (Value, Value)| {
            broadcast(lua, &all_env, name, data, |_, _| true)
        })?,
    )
}

/// A view as handlers get it: its `{ id, user, place, platform }` with
/// the `send` method.
pub fn view_table(lua: &Lua, view: &serde_json::Value) -> mlua::Result<Table> {
    let Value::Table(table) = payload_value(lua, view)? else {
        return Err(raise("INTERNAL: a view isn't a table"));
    };
    let send: Function = lua.named_registry_value(VIEW_SEND)?;
    table.raw_set("send", send)?;
    Ok(table)
}

/// A view table's id.
pub fn view_id(view: Value) -> mlua::Result<i64> {
    let Value::Table(view) = view else {
        return Err(raise("INVALID_ARGUMENT: view must be a view table"));
    };
    id_arg(view.raw_get("id")?, "view.id")
}

/// `{ channel_id = "…" }`: the channel's id.
fn place_arg(place: Value) -> mlua::Result<i64> {
    let Value::Table(place) = place else {
        return Err(raise(
            "INVALID_ARGUMENT: place must be a table such as { channel_id = \"…\" }",
        ));
    };
    id_arg(place.raw_get("channel_id")?, "place.channel_id")
}

fn event_name(name: Value) -> mlua::Result<String> {
    let name = string_arg(name, "name")?;
    if !views::valid_event_name(&name) {
        return Err(raise(
            "INVALID_ARGUMENT: name must be 1 to 64 of a-z, 0-9, _, ., : and -",
        ));
    }
    Ok(name)
}

/// The event's name, and its data as JSON text of at most 64 KiB.
fn event(lua: &Lua, name: Value, data: Value) -> mlua::Result<(String, String)> {
    let name = event_name(name)?;
    let data: serde_json::Value = lua
        .from_value(data)
        .map_err(|error| raise(format!("INVALID_ARGUMENT: data can't be sent: {error}")))?;
    let data = serde_json::to_string(&data)
        .map_err(|error| raise(format!("INVALID_ARGUMENT: data can't be sent: {error}")))?;
    if data.len() > VIEW_EVENT_MAX_BYTES {
        return Err(raise("PLUGIN_LIMIT: an event is at most 64 KiB"));
    }
    Ok((name, data))
}

/// Sends to every open view of the plugin `pick` takes, skipping views
/// over their limit: how many it reached.
fn broadcast(
    lua: &Lua,
    env: &PluginEnv,
    name: Value,
    data: Value,
    pick: impl Fn(&Guild, &ViewInfo) -> bool,
) -> mlua::Result<usize> {
    let (name, data) = event(lua, name, data)?;
    let state = env.state()?;
    let picked: Vec<i64> = {
        let views = state.plugins.views.of_plugin(&env.plugin_id);
        let guild = state.guild();
        views
            .iter()
            .filter(|view| pick(&guild, view))
            .map(|view| view.id)
            .collect()
    };
    let (mut reached, mut skipped) = (0, 0);
    for view_id in picked {
        match views::send(&state, &env.plugin_id, view_id, &name, &data) {
            Ok(true) => reached += 1,
            Ok(false) => {}
            Err(_) => skipped += 1,
        }
    }
    if skipped > 0 {
        let mut logged = env
            .views_skip_logged
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let now = Instant::now();
        if logged.is_none_or(|at| now.saturating_duration_since(at) >= SKIPS_LOGGED_EVERY) {
            *logged = Some(now);
            env.log.push(
                proto::PluginLogLevel::Warn,
                &format!("{skipped} views were skipped: they were sent too many events"),
            );
        }
    }
    Ok(reached)
}

/// `{ name, description?, icon? }`, checked.
fn channel_type(env: &PluginEnv, spec: Value) -> mlua::Result<ChannelType> {
    let Value::Table(spec) = spec else {
        return Err(raise(
            "INVALID_ARGUMENT: a channel type is a table such as { name = \"Tickets\" }",
        ));
    };
    let name = string_arg(field(&spec, "name")?, "name")?;
    if !text_fits(&name, NAME_MAX_CHARS) || name.trim().is_empty() {
        return Err(raise(format!(
            "INVALID_ARGUMENT: name must be 1 to {NAME_MAX_CHARS} characters, without control characters"
        )));
    }
    let description =
        string_arg_opt(field(&spec, "description")?, "description")?.unwrap_or_default();
    if !text_fits(&description, DESCRIPTION_MAX_CHARS) {
        return Err(raise(format!(
            "INVALID_ARGUMENT: description must be at most {DESCRIPTION_MAX_CHARS} characters, without control characters"
        )));
    }
    let icon = string_arg_opt(field(&spec, "icon")?, "icon")?.unwrap_or_default();
    if !icon.is_empty() {
        let lower = icon.to_ascii_lowercase();
        if !icon.starts_with("assets/")
            || !ICON_EXTENSIONS
                .iter()
                .any(|extension| lower.ends_with(extension))
        {
            return Err(raise(
                "INVALID_ARGUMENT: icon must be a .png, .jpg, .jpeg or .webp file under assets/",
            ));
        }
        if !env.assets.contains(&icon) {
            return Err(raise(format!(
                "INVALID_ARGUMENT: icon {icon} isn't in the plugin's package"
            )));
        }
    }
    Ok(ChannelType {
        name,
        description,
        icon,
        plugin_name: env.plugin_name.clone(),
    })
}

fn field(spec: &Table, name: &str) -> mlua::Result<Value> {
    spec.raw_get(name)
}

/// At most `max` characters, none of them control characters.
fn text_fits(text: &str, max: usize) -> bool {
    text.chars().count() <= max && !text.chars().any(char::is_control)
}
