// SPDX-License-Identifier: MPL-2.0

//! Lua API for messages and typing (spec §10.3).

use std::sync::Arc;

use mlua::{Lua, Value};
use opencord_proto::v1 as proto;
use proto::response::Result as Response;

use super::{async_function, id_arg, raise, to_lua_value};
use crate::db;
use crate::db::messages;
use crate::error::ApiError;
use crate::handlers::messages::require_message_channel;
use crate::handlers::{self, Ctx};
use crate::permissions::require_channel;
use crate::plugins::api::files::check_file_path;
use crate::plugins::events;
use crate::plugins::permissions::PluginPermission;
use crate::plugins::runtime::PluginEnv;
use opencord_common::permissions::Permissions;
use opencord_common::upload::MAX_ATTACHMENTS;

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
                    let (content, files) = parse_send_content_arg(content_val)?;
                    if !files.is_empty() {
                        env.require(PluginPermission::AttachFiles)?;
                        if files.len() > MAX_ATTACHMENTS {
                            return Err(raise(
                                "INVALID_ARGUMENT: a message can have at most 10 files",
                            ));
                        }
                        for path in &files {
                            check_file_path(path)
                                .map_err(|e| raise(format!("INVALID_ARGUMENT: {path}: {e}")))?;
                        }
                    }
                    env.check_rate_limit()?;

                    let state = env.state()?;
                    let plugin_id = env.plugin_id.clone();
                    let user_id = env.user_id;
                    let files_dir = env.files_dir.clone();

                    let resp = env
                        .on_server(async move {
                            let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                            require_message_channel(
                                &ctx,
                                channel_id,
                                Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES,
                            )?;
                            if !files.is_empty() {
                                require_channel(
                                    &state.guild(),
                                    user_id,
                                    channel_id,
                                    Permissions::ATTACH_FILES,
                                )?;
                            }

                            let not_found =
                                |p: &str| ApiError::not_found(&format!("no plugin file at {p}"));
                            let mut prepared = Vec::new();
                            for path in files {
                                let mut conn = state.db.acquire().await?;
                                let _db_size = db::plugins::file_size(&mut conn, &plugin_id, &path)
                                    .await?
                                    .ok_or_else(|| not_found(&path))?;
                                drop(conn);

                                let full_path = files_dir.join(&path);
                                let file = match tokio::fs::File::open(&full_path).await {
                                    Ok(f) => f,
                                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                                        return Err(not_found(&path));
                                    }
                                    Err(e) => {
                                        tracing::error!(?e, "failed to open plugin file");
                                        return Err(ApiError::internal());
                                    }
                                };
                                let metadata = file.metadata().await.map_err(|e| {
                                    tracing::error!(?e, "failed to read plugin file metadata");
                                    ApiError::internal()
                                })?;
                                let size = metadata.len();
                                let filename = opencord_common::upload::sanitize_filename(
                                    std::path::Path::new(&path)
                                        .file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or(&path),
                                );
                                crate::handlers::uploads::check_file_for(
                                    &state.guild(),
                                    user_id,
                                    state.attachments.max_file_bytes(),
                                    &filename,
                                    size,
                                )?;
                                prepared.push((filename, file, size));
                            }

                            let mut created_ids = Vec::new();
                            let attach_res: Result<(), ApiError> = async {
                                for (filename, file, size) in prepared {
                                    let row = crate::attachments::plugin_files::attach(
                                        &state, user_id, channel_id, filename, file, size,
                                    )
                                    .await?;
                                    created_ids.push(row.id);
                                }
                                Ok(())
                            }
                            .await;

                            if let Err(e) = attach_res {
                                if let Ok(mut conn) = state.db.acquire().await {
                                    for id in created_ids {
                                        let _ = db::attachments::delete(&mut conn, id).await;
                                    }
                                }
                                return Err(e);
                            }

                            let request = proto::SendMessage {
                                channel_id,
                                content,
                                nonce: String::new(),
                                attachment_ids: created_ids.clone(),
                            };
                            let resp = handlers::messages::send(&ctx, request).await;
                            if resp.is_err() {
                                match state.db.acquire().await {
                                    Ok(mut conn) => {
                                        for id in created_ids {
                                            if let Err(e) = db::attachments::delete(&mut conn, id).await {
                                                tracing::warn!(?e, id, "failed to delete attachment on rollback");
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!(?e, "failed to acquire db connection to rollback attachments");
                                    }
                                }
                            }
                            resp
                        })
                        .await?
                        .map_err(PluginEnv::api_error)?;

                    match resp {
                        Response::Message(msg) => {
                            let (table, ch_name) = {
                                let state = env.state()?;
                                let guild = state.guild();
                                let ch_name = guild.channel_name(channel_id);
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
                    let content = parse_content_arg(content_val)?;
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

fn parse_send_content_arg(value: Value) -> mlua::Result<(String, Vec<String>)> {
    match value {
        Value::String(s) => {
            let text = s
                .to_str()
                .map(|text| text.to_owned())
                .map_err(|_| raise("INVALID_ARGUMENT: content must be UTF-8 text"))?;
            Ok((text, Vec::new()))
        }
        Value::Table(table) => {
            let files = match table.get::<Value>("files")? {
                Value::Nil => Vec::new(),
                Value::Table(files_tbl) => {
                    let mut list = Vec::new();
                    for pair in files_tbl.pairs::<Value, Value>() {
                        let (_, v) = pair?;
                        match v {
                            Value::String(s) => {
                                let path =
                                    s.to_str().map(|text| text.to_owned()).map_err(|_| {
                                        raise("INVALID_ARGUMENT: file path must be UTF-8 text")
                                    })?;
                                list.push(path);
                            }
                            _ => {
                                return Err(raise(
                                    "INVALID_ARGUMENT: files must be a table of strings",
                                ));
                            }
                        }
                    }
                    list
                }
                _ => return Err(raise("INVALID_ARGUMENT: files must be a table")),
            };

            let content = match table.get::<Value>("content")? {
                Value::Nil if !files.is_empty() => String::new(),
                Value::String(s) => s
                    .to_str()
                    .map(|text| text.to_owned())
                    .map_err(|_| raise("INVALID_ARGUMENT: content must be UTF-8 text"))?,
                Value::Nil => return Err(raise("INVALID_ARGUMENT: content must be a string")),
                _ => return Err(raise("INVALID_ARGUMENT: content must be a string")),
            };

            Ok((content, files))
        }
        _ => Err(raise(
            "INVALID_ARGUMENT: content must be a string or a table",
        )),
    }
}

fn parse_content_arg(value: Value) -> mlua::Result<String> {
    match value {
        Value::String(s) => s
            .to_str()
            .map(|text| text.to_owned())
            .map_err(|_| raise("INVALID_ARGUMENT: content must be UTF-8 text")),
        Value::Table(table) => match table.get::<Value>("content")? {
            Value::String(s) => s
                .to_str()
                .map(|text| text.to_owned())
                .map_err(|_| raise("INVALID_ARGUMENT: content must be UTF-8 text")),
            _ => Err(raise("INVALID_ARGUMENT: content must be a string")),
        },
        _ => Err(raise(
            "INVALID_ARGUMENT: content must be a string or a table",
        )),
    }
}
