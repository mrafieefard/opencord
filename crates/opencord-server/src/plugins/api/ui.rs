//! `ui`: the plugin's channel types (spec §14.1). Views come in plan Task 10.

use std::sync::Arc;

use mlua::{Lua, Table, Value};
use opencord_plugins::limits::CHANNEL_TYPES_PER_PLUGIN;
use opencord_proto::v1 as proto;

use super::{async_function, raise, string_arg, string_arg_opt};
use crate::handlers::channels::valid_type_key;
use crate::plugins::channel_types::{self, ChannelType, TooMany};
use crate::plugins::permissions::PluginPermission;
use crate::plugins::runtime::PluginEnv;

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
    lua.globals().raw_set("ui", ui)
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
