// SPDX-License-Identifier: MPL-2.0

//! Files API for plugins (design 2026-11-07 §10.3, §13.1).

use std::sync::Arc;

use mlua::{Lua, Table, Value};

use super::{async_function, raise};
use crate::db;
use crate::plugins::runtime::PluginEnv;

/// Validates that a file path follows package path rules without the folder rule:
/// at most 200 bytes, segments of `[A-Za-z0-9._-]`, not starting with `.`.
pub fn check_file_path(path: &str) -> Result<(), &'static str> {
    if path.is_empty() || path.len() > 200 {
        return Err("invalid path");
    }
    let segments: Vec<&str> = path.split('/').collect();
    for segment in &segments {
        let allowed = |b: u8| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-');
        if segment.is_empty() || segment.starts_with('.') || !segment.bytes().all(allowed) {
            return Err("invalid path");
        }
    }
    Ok(())
}

pub fn install(lua: &Lua, env: &Arc<PluginEnv>) -> mlua::Result<()> {
    let files = lua.create_table()?;

    let put_env = Arc::clone(env);
    files.raw_set(
        "put",
        async_function(
            lua,
            move |_lua, (path_val, data_val, options_val): (Value, Value, Option<Table>)| {
                let env = Arc::clone(&put_env);
                async move {
                    let path = match path_val {
                        Value::String(s) => s.to_str()?.to_owned(),
                        _ => return Err(raise("INVALID_ARGUMENT: path must be a string")),
                    };
                    let data_bytes = match data_val {
                        Value::String(s) => s.as_bytes().to_vec(),
                        Value::Buffer(b) => b.to_vec(),
                        _ => {
                            return Err(raise("INVALID_ARGUMENT: data must be a string or buffer"));
                        }
                    };
                    let public = match options_val {
                        Some(t) => t.get::<Option<bool>>("public")?.unwrap_or(false),
                        None => false,
                    };

                    check_file_path(&path)
                        .map_err(|e| raise(format!("INVALID_ARGUMENT: {path}: {e}")))?;
                    let state = env.state()?;
                    let plugin_id = env.plugin_id.clone();
                    let files_dir = env.files_dir.clone();
                    let data_len = data_bytes.len() as i64;
                    let files_bytes_quota = env.files_bytes;

                    env.on_server(async move {
                        let mut conn = state
                            .db
                            .acquire()
                            .await
                            .map_err(|e| raise(format!("INTERNAL: {e}")))?;
                        let total = db::plugins::files_bytes(&mut conn, &plugin_id)
                            .await
                            .map_err(|e| raise(format!("INTERNAL: {e}")))?;
                        let old_size = db::plugins::file_size(&mut conn, &plugin_id, &path)
                            .await
                            .map_err(|e| raise(format!("INTERNAL: {e}")))?
                            .unwrap_or(0);
                        let new_total = (total - old_size) + data_len;
                        if new_total as u64 > files_bytes_quota {
                            return Err(raise("PLUGIN_LIMIT: the plugin's files are full"));
                        }

                        let target_path = files_dir.join(&path);
                        if let Some(parent) = target_path.parent() {
                            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                                raise(format!("INTERNAL: failed to create directory: {e}"))
                            })?;
                        }
                        let temp_name = format!(
                            ".tmp.{}.{}",
                            std::process::id(),
                            hex::encode(crate::random::bytes::<8>())
                        );
                        let temp_path = target_path.parent().unwrap_or(&files_dir).join(temp_name);
                        if let Err(e) = tokio::fs::write(&temp_path, &data_bytes).await {
                            let _ = tokio::fs::remove_file(&temp_path).await;
                            return Err(raise(format!("INTERNAL: failed to write file: {e}")));
                        }
                        if let Err(e) = tokio::fs::rename(&temp_path, &target_path).await {
                            let _ = tokio::fs::remove_file(&temp_path).await;
                            return Err(raise(format!("INTERNAL: failed to rename file: {e}")));
                        }

                        db::plugins::upsert_plugin_file(
                            &mut conn,
                            &plugin_id,
                            &path,
                            data_len,
                            public,
                            crate::state::now_ms(),
                        )
                        .await
                        .map_err(|e| raise(format!("INTERNAL: {e}")))?;

                        Ok::<(), mlua::Error>(())
                    })
                    .await??;
                    Ok(())
                }
            },
        )?,
    )?;

    let get_env = Arc::clone(env);
    files.raw_set(
        "get",
        async_function(lua, move |lua, path_val: Value| {
            let env = Arc::clone(&get_env);
            async move {
                let path = match path_val {
                    Value::String(s) => s.to_str()?.to_owned(),
                    _ => return Err(raise("INVALID_ARGUMENT: path must be a string")),
                };
                check_file_path(&path)
                    .map_err(|e| raise(format!("INVALID_ARGUMENT: {path}: {e}")))?;
                let target_path = env.files_dir.join(&path);
                let bytes_opt = env
                    .on_server(async move {
                        match tokio::fs::read(&target_path).await {
                            Ok(bytes) => Ok(Some(bytes)),
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                            Err(e) => Err(raise(format!("INTERNAL: failed to read file: {e}"))),
                        }
                    })
                    .await??;
                match bytes_opt {
                    Some(bytes) => Ok(Value::Buffer(lua.create_buffer(bytes)?)),
                    None => Ok(Value::Nil),
                }
            }
        })?,
    )?;

    let del_env = Arc::clone(env);
    files.raw_set(
        "delete",
        async_function(lua, move |_lua, path_val: Value| {
            let env = Arc::clone(&del_env);
            async move {
                let path = match path_val {
                    Value::String(s) => s.to_str()?.to_owned(),
                    _ => return Err(raise("INVALID_ARGUMENT: path must be a string")),
                };
                check_file_path(&path)
                    .map_err(|e| raise(format!("INVALID_ARGUMENT: {path}: {e}")))?;
                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let target_path = env.files_dir.join(&path);

                env.on_server(async move {
                    let _ = tokio::fs::remove_file(&target_path).await;
                    let mut conn = state
                        .db
                        .acquire()
                        .await
                        .map_err(|e| raise(format!("INTERNAL: {e}")))?;
                    db::plugins::delete_plugin_file(&mut conn, &plugin_id, &path)
                        .await
                        .map_err(|e| raise(format!("INTERNAL: {e}")))?;
                    Ok::<(), mlua::Error>(())
                })
                .await??;
                Ok(())
            }
        })?,
    )?;

    let list_env = Arc::clone(env);
    files.raw_set(
        "list",
        async_function(lua, move |lua, prefix_val: Option<Value>| {
            let env = Arc::clone(&list_env);
            async move {
                let prefix_opt = match prefix_val {
                    Some(Value::String(s)) => Some(s.to_str()?.to_owned()),
                    Some(Value::Nil) | None => None,
                    _ => return Err(raise("INVALID_ARGUMENT: prefix must be a string")),
                };
                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let rows = env
                    .on_server(async move {
                        let mut conn = state
                            .db
                            .acquire()
                            .await
                            .map_err(|e| raise(format!("INTERNAL: {e}")))?;
                        db::plugins::list_plugin_files(&mut conn, &plugin_id, prefix_opt.as_deref())
                            .await
                            .map_err(|e| raise(format!("INTERNAL: {e}")))
                    })
                    .await??;

                let table = lua.create_table()?;
                for (i, row) in rows.into_iter().enumerate() {
                    let item = lua.create_table()?;
                    item.set("path", row.path)?;
                    item.set("size", row.size)?;
                    item.set("public", row.public)?;
                    item.set("updated_at_ms", row.updated_at)?;
                    table.set(i + 1, item)?;
                }
                Ok(Value::Table(table))
            }
        })?,
    )?;

    lua.globals().raw_set("files", files)?;
    Ok(())
}
