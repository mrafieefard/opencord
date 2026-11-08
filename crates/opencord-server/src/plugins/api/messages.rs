// SPDX-License-Identifier: MPL-2.0

//! Lua API for messages and typing (spec §10.3).

use std::sync::Arc;

use mlua::{Lua, LuaSerdeExt, Value};
use opencord_proto::v1 as proto;
use proto::response::Result as Response;

use super::{async_function, id_arg, raise};
use crate::db::messages;
use crate::error::ApiError;
use crate::handlers::{self, Ctx};
use crate::plugins::events;
use crate::plugins::permissions::PluginPermission;
use crate::plugins::runtime::PluginEnv;

pub fn install(lua: &Lua, env: &Arc<PluginEnv>) -> mlua::Result<()> {
    let messages = lua.create_table()?;

    // messages.send(channel_id, content_or_table)
    let send_env = env.clone();
    messages.raw_set(
        "send",
        async_function(
            lua,
            move |lua, (channel_val, content_val): (Value, Value)| {
                let env = send_env.clone();
                async move {
                    env.require(PluginPermission::SendMessages)?;
                    let channel_id = id_arg(channel_val, "channel_id")?;
                    let content = parse_content_arg(content_val, true)?;
                    env.check_rate_limit()?;

                    let state = env.state()?;
                    let plugin_id = env.plugin_id.clone();
                    let user_id = env.user_id;

                    let resp = env
                        .on_server(async move {
                            let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                            let request = proto::SendMessage {
                                channel_id,
                                content,
                                nonce: String::new(),
                                attachment_ids: Vec::new(),
                            };
                            handlers::messages::send(&ctx, request).await
                        })
                        .await?
                        .map_err(PluginEnv::api_error)?;

                    match resp {
                        Response::Message(msg) => {
                            let (table, ch_name) = {
                                let state = env.state()?;
                                let guild = state.guild();
                                let ch_name = guild
                                    .channels
                                    .get(&channel_id)
                                    .map(|c| c.name.clone())
                                    .unwrap_or_else(|| channel_id.to_string());
                                (events::message_table(&msg, &guild), ch_name)
                            };
                            env.log.push(
                                proto::PluginLogLevel::Action,
                                &format!("sent message in #{ch_name}"),
                            );
                            to_lua_value(&lua, &table)
                        }
                        _ => Err(raise("INTERNAL: expected message response")),
                    }
                }
            },
        )?,
    )?;

    // messages.edit(message_id, content_or_table)
    let edit_env = env.clone();
    messages.raw_set(
        "edit",
        async_function(
            lua,
            move |lua, (message_val, content_val): (Value, Value)| {
                let env = edit_env.clone();
                async move {
                    env.require(PluginPermission::SendMessages)?;
                    let message_id = id_arg(message_val, "message_id")?;
                    let content = parse_content_arg(content_val, false)?;
                    env.check_rate_limit()?;

                    let state = env.state()?;
                    let plugin_id = env.plugin_id.clone();
                    let user_id = env.user_id;

                    let resp = env
                        .on_server(async move {
                            let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                            let request = proto::EditMessage {
                                message_id,
                                content,
                            };
                            handlers::messages::edit(&ctx, request).await
                        })
                        .await?
                        .map_err(PluginEnv::api_error)?;

                    match resp {
                        Response::Message(msg) => {
                            let table = {
                                let state = env.state()?;
                                let guild = state.guild();
                                events::message_table(&msg, &guild)
                            };
                            env.log.push(
                                proto::PluginLogLevel::Action,
                                &format!("edited message {message_id}"),
                            );
                            to_lua_value(&lua, &table)
                        }
                        _ => Err(raise("INTERNAL: expected message response")),
                    }
                }
            },
        )?,
    )?;

    // messages.delete(message_id)
    let delete_env = env.clone();
    messages.raw_set(
        "delete",
        async_function(lua, move |_lua, message_val: Value| {
            let env = delete_env.clone();
            async move {
                let message_id = id_arg(message_val, "message_id")?;
                env.check_rate_limit()?;
                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                let has_manage_perm = env.has(PluginPermission::ManageMessages);

                env.on_server(async move {
                    let mut conn = state.db.acquire().await?;
                    let existing = messages::find(&mut conn, message_id)
                        .await?
                        .ok_or_else(|| ApiError::not_found("message"))?;

                    if existing.author_id != user_id && !has_manage_perm {
                        return Err(ApiError::forbidden(
                            "PERMISSION_DENIED: the plugin doesn't have manage_messages",
                        ));
                    }

                    let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                    let request = proto::DeleteMessage { message_id };
                    handlers::messages::delete(&ctx, request).await
                })
                .await?
                .map_err(|err| {
                    if err.message.starts_with("PERMISSION_DENIED:") {
                        raise(err.message)
                    } else {
                        PluginEnv::api_error(err)
                    }
                })?;

                env.log.push(
                    proto::PluginLogLevel::Action,
                    &format!("deleted message {message_id}"),
                );
                Ok(())
            }
        })?,
    )?;

    // messages.fetch(channel_id, options?)
    let fetch_env = env.clone();
    messages.raw_set(
        "fetch",
        async_function(
            lua,
            move |lua, (channel_val, options_val): (Value, Option<Value>)| {
                let env = fetch_env.clone();
                async move {
                    env.require(PluginPermission::ReadMessages)?;
                    let channel_id = id_arg(channel_val, "channel_id")?;
                    env.check_rate_limit()?;

                    let (before, limit) = match options_val {
                        None | Some(Value::Nil) => (None, None),
                        Some(Value::Table(table)) => {
                            let before = match table.get::<Value>("before")? {
                                Value::Nil => None,
                                val => Some(id_arg(val, "before")?),
                            };
                            let limit = match table.get::<Value>("limit")? {
                                Value::Nil => None,
                                Value::Integer(n) => {
                                    if !(1..=100).contains(&n) {
                                        return Err(raise(
                                            "INVALID_ARGUMENT: limit must be between 1 and 100",
                                        ));
                                    }
                                    Some(n as u32)
                                }
                                _ => {
                                    return Err(raise(
                                        "INVALID_ARGUMENT: limit must be between 1 and 100",
                                    ));
                                }
                            };
                            (before, limit)
                        }
                        _ => return Err(raise("INVALID_ARGUMENT: options must be a table")),
                    };

                    let state = env.state()?;
                    let plugin_id = env.plugin_id.clone();
                    let user_id = env.user_id;

                    let resp = env
                        .on_server(async move {
                            let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                            let request = proto::FetchMessages {
                                channel_id,
                                before,
                                limit: limit.unwrap_or(50),
                            };
                            handlers::messages::fetch(&ctx, request).await
                        })
                        .await?
                        .map_err(PluginEnv::api_error)?;

                    match resp {
                        Response::Messages(list) => {
                            let tables = {
                                let state = env.state()?;
                                let guild = state.guild();
                                list.messages
                                    .iter()
                                    .map(|m| events::message_table(m, &guild))
                                    .collect::<Vec<_>>()
                            };
                            to_lua_value(&lua, &serde_json::Value::Array(tables))
                        }
                        _ => Err(raise("INTERNAL: expected message list response")),
                    }
                }
            },
        )?,
    )?;

    lua.globals().raw_set("messages", messages)?;

    // typing.start(channel_id)
    let typing = lua.create_table()?;
    let typing_env = env.clone();
    typing.raw_set(
        "start",
        async_function(lua, move |_lua, channel_val: Value| {
            let env = typing_env.clone();
            async move {
                env.require(PluginPermission::SendMessages)?;
                let channel_id = id_arg(channel_val, "channel_id")?;
                env.check_rate_limit()?;

                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                env.on_server(async move {
                    let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                    let request = proto::StartTyping { channel_id };
                    handlers::messages::start_typing(&ctx, request)
                })
                .await?
                .map_err(PluginEnv::api_error)?;

                Ok(())
            }
        })?,
    )?;

    lua.globals().raw_set("typing", typing)?;

    Ok(())
}

fn parse_content_arg(value: Value, check_files: bool) -> mlua::Result<String> {
    match value {
        Value::String(s) => s
            .to_str()
            .map(|text| text.to_owned())
            .map_err(|_| raise("INVALID_ARGUMENT: content must be UTF-8 text")),
        Value::Table(table) => {
            if check_files && table.contains_key("files")? {
                return Err(raise("INVALID_ARGUMENT: files arrive in a later version"));
            }
            match table.get::<Value>("content")? {
                Value::String(s) => s
                    .to_str()
                    .map(|text| text.to_owned())
                    .map_err(|_| raise("INVALID_ARGUMENT: content must be UTF-8 text")),
                _ => Err(raise("INVALID_ARGUMENT: content must be a string")),
            }
        }
        _ => Err(raise(
            "INVALID_ARGUMENT: content must be a string or a table",
        )),
    }
}

fn to_lua_value(lua: &Lua, val: &serde_json::Value) -> mlua::Result<Value> {
    let options = mlua::SerializeOptions::new()
        .serialize_none_to_null(false)
        .serialize_unit_to_null(false)
        .set_array_metatable(false);
    lua.to_value_with(val, options)
}
