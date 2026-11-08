// SPDX-License-Identifier: MPL-2.0

//! The `commands` global: plugins registering slash commands (§10.3, §17).

use std::sync::Arc;

use mlua::{Lua, Table, Value};
use opencord_proto::v1 as proto;
use proto::CommandOptionKind as Kind;

use super::{function, raise};
use crate::plugins::permissions::PluginPermission;
use crate::plugins::runtime::PluginEnv;

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 32
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn valid_description(desc: &str) -> bool {
    let count = desc.chars().count();
    (1..=100).contains(&count)
}

fn parse_kind(s: &str) -> Result<Kind, mlua::Error> {
    match s {
        "text" => Ok(Kind::Text),
        "integer" => Ok(Kind::Integer),
        "boolean" => Ok(Kind::Boolean),
        "choice" => Ok(Kind::Choice),
        "user" => Ok(Kind::User),
        "channel" => Ok(Kind::Channel),
        _ => Err(raise(
            "INVALID_ARGUMENT: kind must be one of text, integer, boolean, choice, user, channel",
        )),
    }
}

pub fn install(lua: &Lua, env: &Arc<PluginEnv>) -> mlua::Result<()> {
    let table = lua.create_table()?;

    // commands.register(spec)
    let env_reg = env.clone();
    table.raw_set(
        "register",
        function(lua, move |_lua, spec_val: Value| {
            env_reg.require(PluginPermission::RegisterCommands)?;
            let spec_table = match spec_val {
                Value::Table(t) => t,
                _ => return Err(raise("INVALID_ARGUMENT: spec must be a table")),
            };

            let name: String = spec_table
                .get("name")
                .map_err(|_| raise("INVALID_ARGUMENT: name is required"))?;
            let name = name.trim_start_matches('/').to_ascii_lowercase();
            if !valid_name(&name) {
                return Err(raise(
                    "INVALID_ARGUMENT: command name must match ^[a-z0-9-]{1,32}$",
                ));
            }

            let description: String = spec_table
                .get("description")
                .map_err(|_| raise("INVALID_ARGUMENT: description is required"))?;
            if !valid_description(&description) {
                return Err(raise(
                    "INVALID_ARGUMENT: description must be 1 to 100 characters",
                ));
            }

            let mut options = Vec::new();
            if let Ok(opts_table) = spec_table.get::<Table>("options") {
                let seq = opts_table.sequence_values::<Table>();
                for opt in seq {
                    let opt = opt?;
                    if options.len() >= 25 {
                        return Err(raise("INVALID_ARGUMENT: at most 25 options"));
                    }
                    let opt_name: String = opt
                        .get("name")
                        .map_err(|_| raise("INVALID_ARGUMENT: option name is required"))?;
                    let opt_name = opt_name.trim_start_matches('/').to_ascii_lowercase();
                    if !valid_name(&opt_name) {
                        return Err(raise(
                            "INVALID_ARGUMENT: option name must match ^[a-z0-9-]{1,32}$",
                        ));
                    }

                    let opt_desc: String = opt
                        .get("description")
                        .map_err(|_| raise("INVALID_ARGUMENT: option description is required"))?;
                    if !valid_description(&opt_desc) {
                        return Err(raise(
                            "INVALID_ARGUMENT: option description must be 1 to 100 characters",
                        ));
                    }

                    let kind_str: String = opt
                        .get("kind")
                        .map_err(|_| raise("INVALID_ARGUMENT: option kind is required"))?;
                    let kind = parse_kind(&kind_str)?;

                    let required = opt.get::<bool>("required").unwrap_or(false);
                    let autocomplete = opt.get::<bool>("autocomplete").unwrap_or(false);
                    let min = opt.get::<i64>("min").unwrap_or(0);
                    let max = opt.get::<i64>("max").unwrap_or(0);

                    let mut choices = Vec::new();
                    if let Ok(choices_table) = opt.get::<Table>("choices") {
                        for choice_item in choices_table.sequence_values::<Value>() {
                            let item = choice_item?;
                            if choices.len() >= 25 {
                                return Err(raise("INVALID_ARGUMENT: at most 25 choices"));
                            }
                            let ct = match item {
                                Value::Table(ct) => ct,
                                _ => {
                                    return Err(raise("INVALID_ARGUMENT: choice must be a table"));
                                }
                            };
                            let c_name: String = ct
                                .get("name")
                                .or_else(|_| ct.get(1))
                                .map_err(|_| raise("INVALID_ARGUMENT: choice name required"))?;
                            let c_val: String =
                                ct.get("value").or_else(|_| ct.get(2)).map_err(|_| {
                                    raise("INVALID_ARGUMENT: choice value required")
                                })?;
                            let c_name_len = c_name.chars().count();
                            let c_val_len = c_val.chars().count();
                            if !(1..=100).contains(&c_name_len) || !(1..=100).contains(&c_val_len) {
                                return Err(raise(
                                    "INVALID_ARGUMENT: choice name and value must be 1 to 100 characters",
                                ));
                            }
                            choices.push(proto::CommandChoice {
                                name: c_name,
                                value: c_val,
                                artwork_id: String::new(),
                            });
                        }
                    }

                    options.push(proto::CommandOptionSpec {
                        name: opt_name,
                        description: opt_desc,
                        kind: kind as i32,
                        required,
                        autocomplete,
                        choices,
                        min,
                        max,
                    });
                }
            }

            // Check collision
            if crate::commands::find(&name).is_some() {
                return Err(raise(format!("INVALID_ARGUMENT: /{name} is already taken")));
            }

            env_reg.check_rate_limit()?;

            let state = env_reg.state()?;
            if let Some(other) = state.commands.find_plugin(&name)
                && other.plugin_id != env_reg.plugin_id
            {
                return Err(raise(format!("INVALID_ARGUMENT: /{name} is already taken")));
            }

            let spec = proto::CommandSpec {
                name: name.clone(),
                description,
                options,
                feature: format!("plugin:{}", env_reg.plugin_id),
                permission_summary: String::new(),
                aliases: Vec::new(),
                permissions: 0,
                enabled: true,
                channel_ids: Vec::new(),
            };

            state
                .commands
                .register_plugin(&env_reg.plugin_id, spec)
                .map_err(PluginEnv::api_error)?;
            crate::commands::announce(&state);
            env_reg
                .log
                .push(proto::PluginLogLevel::Action, &format!("registered command /{name}"));
            tracing::info!(plugin = %env_reg.plugin_id, %name, "registered command");
            Ok(())
        })?,
    )?;

    // commands.unregister(name)
    let env_unreg = env.clone();
    table.raw_set(
        "unregister",
        function(lua, move |_lua, name: String| {
            env_unreg.require(PluginPermission::RegisterCommands)?;
            let name = name.trim_start_matches('/').to_ascii_lowercase();
            env_unreg.check_rate_limit()?;
            let state = env_unreg.state()?;
            if state
                .commands
                .unregister_plugin(&env_unreg.plugin_id, &name)
            {
                crate::commands::announce(&state);
                env_unreg.log.push(
                    proto::PluginLogLevel::Action,
                    &format!("unregistered command /{name}"),
                );
                tracing::info!(plugin = %env_unreg.plugin_id, %name, "unregistered command");
            }
            Ok(())
        })?,
    )?;

    lua.globals().raw_set("commands", table)?;
    Ok(())
}
