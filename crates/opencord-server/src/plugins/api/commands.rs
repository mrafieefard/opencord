// SPDX-License-Identifier: MPL-2.0

//! The `commands` global: plugins registering slash commands (§10.3, §17).

use std::sync::Arc;

use mlua::{Lua, Value};
use opencord_proto::v1 as proto;
use proto::CommandOptionKind as Kind;

use super::{function, raise, string_arg};
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

            let name = string_arg(spec_table.raw_get("name")?, "name")?;
            let name = name.trim_start_matches('/').to_ascii_lowercase();
            if !valid_name(&name) {
                return Err(raise(
                    "INVALID_ARGUMENT: command name must match ^[a-z0-9-]{1,32}$",
                ));
            }

            let description = string_arg(spec_table.raw_get("description")?, "description")?;
            if !valid_description(&description) {
                return Err(raise(
                    "INVALID_ARGUMENT: description must be 1 to 100 characters",
                ));
            }

            let mut options = Vec::new();
            match spec_table.raw_get::<Value>("options")? {
                Value::Nil => {}
                Value::Table(opts_table) => {
                    for opt_item in opts_table.sequence_values::<Value>() {
                        let opt_val = opt_item?;
                        if options.len() >= 25 {
                            return Err(raise("INVALID_ARGUMENT: at most 25 options"));
                        }
                        let opt = match opt_val {
                            Value::Table(t) => t,
                            _ => return Err(raise("INVALID_ARGUMENT: option must be a table")),
                        };
                        let opt_name = string_arg(opt.raw_get("name")?, "option name")?;
                        let opt_name = opt_name.trim_start_matches('/').to_ascii_lowercase();
                        if !valid_name(&opt_name) {
                            return Err(raise(
                                "INVALID_ARGUMENT: option name must match ^[a-z0-9-]{1,32}$",
                            ));
                        }

                        let opt_desc =
                            string_arg(opt.raw_get("description")?, "option description")?;
                        if !valid_description(&opt_desc) {
                            return Err(raise(
                                "INVALID_ARGUMENT: option description must be 1 to 100 characters",
                            ));
                        }

                        let kind_str = string_arg(opt.raw_get("kind")?, "option kind")?;
                        let kind = parse_kind(&kind_str)?;

                        let required = match opt.raw_get::<Value>("required")? {
                            Value::Nil => false,
                            Value::Boolean(b) => b,
                            _ => {
                                return Err(raise(
                                    "INVALID_ARGUMENT: option required must be a boolean",
                                ));
                            }
                        };
                        let autocomplete = match opt.raw_get::<Value>("autocomplete")? {
                            Value::Nil => false,
                            Value::Boolean(b) => b,
                            _ => {
                                return Err(raise(
                                    "INVALID_ARGUMENT: option autocomplete must be a boolean",
                                ));
                            }
                        };
                        let min = match opt.raw_get::<Value>("min")? {
                            Value::Nil => 0,
                            Value::Integer(i) => i,
                            Value::Number(n) if n.is_finite() && n.floor() == n => n as i64,
                            _ => {
                                return Err(raise(
                                    "INVALID_ARGUMENT: option min must be an integer",
                                ));
                            }
                        };
                        let max = match opt.raw_get::<Value>("max")? {
                            Value::Nil => 0,
                            Value::Integer(i) => i,
                            Value::Number(n) if n.is_finite() && n.floor() == n => n as i64,
                            _ => {
                                return Err(raise(
                                    "INVALID_ARGUMENT: option max must be an integer",
                                ));
                            }
                        };

                        let mut choices = Vec::new();
                        match opt.raw_get::<Value>("choices")? {
                            Value::Nil => {}
                            Value::Table(choices_table) => {
                                for choice_item in choices_table.sequence_values::<Value>() {
                                    let item = choice_item?;
                                    if choices.len() >= 25 {
                                        return Err(raise("INVALID_ARGUMENT: at most 25 choices"));
                                    }
                                    let ct = match item {
                                        Value::Table(ct) => ct,
                                        _ => {
                                            return Err(raise(
                                                "INVALID_ARGUMENT: choice must be a table",
                                            ));
                                        }
                                    };
                                    let c_name_val = match ct.raw_get::<Value>("name")? {
                                        Value::Nil => ct.raw_get::<Value>(1)?,
                                        v => v,
                                    };
                                    let c_name = string_arg(c_name_val, "choice name")?;
                                    let c_val_val = match ct.raw_get::<Value>("value")? {
                                        Value::Nil => ct.raw_get::<Value>(2)?,
                                        v => v,
                                    };
                                    let c_val = string_arg(c_val_val, "choice value")?;
                                    let c_name_len = c_name.chars().count();
                                    let c_val_len = c_val.chars().count();
                                    if !(1..=100).contains(&c_name_len)
                                        || !(1..=100).contains(&c_val_len)
                                    {
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
                            _ => {
                                return Err(raise(
                                    "INVALID_ARGUMENT: option choices must be a table",
                                ));
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
                _ => return Err(raise("INVALID_ARGUMENT: options must be a table")),
            }

            env_reg.check_rate_limit()?;

            let state = env_reg.state()?;

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
        function(lua, move |_lua, name_val: Value| {
            env_unreg.require(PluginPermission::RegisterCommands)?;
            let name = string_arg(name_val, "name")?;
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
