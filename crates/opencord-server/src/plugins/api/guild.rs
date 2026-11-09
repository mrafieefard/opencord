// SPDX-License-Identifier: MPL-2.0

//! The `channels`, `roles`, `members`, `bans`, `invites` and `server` globals (§10.3).

use std::sync::Arc;

use mlua::{Lua, Table, Value};
use opencord_common::permissions::Permissions;
use opencord_proto::v1 as proto;

use super::{async_function, id_arg, id_arg_opt, raise, string_arg, string_arg_opt, to_lua_value};
use crate::handlers::Ctx;
use crate::handlers::{channels, invites, members, roles};
use crate::plugins::permissions::PluginPermission;
use crate::plugins::runtime::PluginEnv;

fn get_opt_string(table: &Table, key: &'static str) -> mlua::Result<Option<String>> {
    string_arg_opt(table.raw_get::<Value>(key)?, key)
}

fn get_opt_bool(table: &Table, key: &'static str) -> mlua::Result<Option<bool>> {
    match table.raw_get::<Value>(key)? {
        Value::Nil => Ok(None),
        Value::Boolean(b) => Ok(Some(b)),
        _ => Err(raise(format!("INVALID_ARGUMENT: {key} must be a boolean"))),
    }
}

fn get_opt_u32(table: &Table, key: &'static str) -> mlua::Result<Option<u32>> {
    match table.raw_get::<Value>(key)? {
        Value::Nil => Ok(None),
        Value::Integer(i) if i >= 0 && i <= u32::MAX as i64 => Ok(Some(i as u32)),
        Value::Number(n) if n >= 0.0 && n.fract() == 0.0 && n <= u32::MAX as f64 => {
            Ok(Some(n as u32))
        }
        _ => Err(raise(format!("INVALID_ARGUMENT: {key} must be an integer"))),
    }
}

fn get_opt_id(table: &Table, key: &'static str) -> mlua::Result<Option<i64>> {
    id_arg_opt(table.raw_get::<Value>(key)?, key)
}

fn parse_permissions_table(table: Table) -> mlua::Result<Permissions> {
    let mut perms = Permissions::empty();
    for item in table.sequence_values::<Value>() {
        let val = item?;
        let name = string_arg(val, "permission name")?;
        let bit = crate::plugins::events::member_permission_from_name(&name)
            .ok_or_else(|| raise(format!("INVALID_ARGUMENT: unknown permission '{name}'")))?;
        perms |= bit;
    }
    Ok(perms)
}

fn channel_table(lua: &Lua, ch: &proto::Channel) -> mlua::Result<Table> {
    let json_val = crate::plugins::event_tables::channel_table(ch);
    match to_lua_value(lua, &json_val)? {
        Value::Table(t) => Ok(t),
        _ => Err(raise("failed to serialize channel table")),
    }
}

fn role_table(lua: &Lua, r: &proto::Role) -> mlua::Result<Table> {
    let json_val = crate::plugins::event_tables::role_table(r);
    match to_lua_value(lua, &json_val)? {
        Value::Table(t) => Ok(t),
        _ => Err(raise("failed to serialize role table")),
    }
}

fn server_info_table(lua: &Lua, s: &proto::ServerInfo) -> mlua::Result<Table> {
    let json_val = crate::plugins::event_tables::server_info_table(s);
    match to_lua_value(lua, &json_val)? {
        Value::Table(t) => Ok(t),
        _ => Err(raise("failed to serialize server info table")),
    }
}

fn member_table(lua: &Lua, m: &proto::Member) -> mlua::Result<Table> {
    let json_val = crate::plugins::events::member_table(m);
    match to_lua_value(lua, &json_val)? {
        Value::Table(t) => Ok(t),
        _ => Err(raise("failed to serialize member table")),
    }
}

fn invite_table(lua: &Lua, inv: &proto::Invite) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.raw_set("code", inv.code.clone())?;
    if let Some(uses) = inv.max_uses {
        t.raw_set("max_uses", uses)?;
    }
    t.raw_set("uses", inv.uses)?;
    if let Some(exp) = inv.expires_at_ms {
        t.raw_set("expires_at_ms", exp)?;
    }
    if let Some(ref addr) = inv.address {
        t.raw_set("address", addr.host.clone())?;
    }
    Ok(t)
}

pub fn install(lua: &Lua, env: &Arc<PluginEnv>) -> mlua::Result<()> {
    install_channels(lua, env)?;
    install_roles(lua, env)?;
    install_members(lua, env)?;
    install_invites(lua, env)?;
    install_server(lua, env)?;
    Ok(())
}

fn install_channels(lua: &Lua, env: &Arc<PluginEnv>) -> mlua::Result<()> {
    let table = lua.create_table()?;

    // channels.list()
    let env_list = env.clone();
    table.raw_set(
        "list",
        async_function(lua, move |lua, ()| {
            let env = env_list.clone();
            async move {
                env.require(PluginPermission::ReadChannels)?;
                let list = {
                    let state = env.state()?;
                    let guild = state.guild();
                    let user_id = env.user_id;

                    let mut list = Vec::new();
                    for channel in guild.channels.values() {
                        if guild.can_view(user_id, channel.id) {
                            list.push(channel.to_proto());
                        }
                    }
                    list.sort_by_key(|c| c.position);
                    list
                };

                let res_table = lua.create_table()?;
                for (idx, ch) in list.iter().enumerate() {
                    res_table.raw_set(idx + 1, channel_table(&lua, ch)?)?;
                }
                Ok(Value::Table(res_table))
            }
        })?,
    )?;

    // channels.get(id)
    let env_get = env.clone();
    table.raw_set(
        "get",
        async_function(lua, move |lua, id_val: Value| {
            let env = env_get.clone();
            async move {
                env.require(PluginPermission::ReadChannels)?;
                let channel_id = id_arg(id_val, "channel_id")?;
                let ch_opt = {
                    let state = env.state()?;
                    let guild = state.guild();
                    let user_id = env.user_id;

                    if let Some(ch) = guild.channels.get(&channel_id)
                        && guild.can_view(user_id, channel_id)
                    {
                        Some(ch.to_proto())
                    } else {
                        None
                    }
                };

                if let Some(ch) = ch_opt {
                    return Ok(Value::Table(channel_table(&lua, &ch)?));
                }
                Ok(Value::Nil)
            }
        })?,
    )?;

    // channels.create({ kind = "text"|"voice"|"category", name, topic?, parent_id? })
    let env_create = env.clone();
    table.raw_set(
        "create",
        async_function(lua, move |lua, spec_val: Value| {
            let env = env_create.clone();
            async move {
                env.require(PluginPermission::ManageChannels)?;
                let spec = match spec_val {
                    Value::Table(t) => t,
                    _ => return Err(raise("INVALID_ARGUMENT: spec must be a table")),
                };
                let name: String = spec
                    .get("name")
                    .map_err(|_| raise("INVALID_ARGUMENT: channel name is required"))?;
                let kind_str = get_opt_string(&spec, "kind")?.unwrap_or_else(|| "text".to_owned());
                let kind = match kind_str.as_str() {
                    "text" => proto::ChannelKind::Text as i32,
                    "voice" => proto::ChannelKind::Voice as i32,
                    "category" => proto::ChannelKind::Category as i32,
                    _ => return Err(raise("INVALID_ARGUMENT: unknown channel kind")),
                };
                let topic = get_opt_string(&spec, "topic")?;
                let parent_id = get_opt_id(&spec, "parent_id")?;
                env.check_rate_limit()?;

                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                let resp = env
                    .on_server(async move {
                        let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                        let req = proto::CreateChannel {
                            name,
                            kind,
                            topic,
                            parent_id,
                        };
                        channels::create(&ctx, req).await
                    })
                    .await?
                    .map_err(PluginEnv::api_error)?;

                match resp {
                    proto::response::Result::Channel(ch) => {
                        env.log.push(
                            proto::PluginLogLevel::Action,
                            &format!("created channel #{}", ch.name),
                        );
                        Ok(Value::Table(channel_table(&lua, &ch)?))
                    }
                    _ => Err(raise("INTERNAL: unexpected response")),
                }
            }
        })?,
    )?;

    // channels.update(id, { name?, topic?, parent_id? })
    let env_update = env.clone();
    table.raw_set(
        "update",
        async_function(lua, move |lua, (id_val, spec_val): (Value, Value)| {
            let env = env_update.clone();
            async move {
                env.require(PluginPermission::ManageChannels)?;
                let channel_id = id_arg(id_val, "channel_id")?;
                let spec = match spec_val {
                    Value::Table(t) => t,
                    _ => return Err(raise("INVALID_ARGUMENT: spec must be a table")),
                };
                let name = get_opt_string(&spec, "name")?;
                let topic = get_opt_string(&spec, "topic")?;
                let parent_id = get_opt_id(&spec, "parent_id")?;
                env.check_rate_limit()?;

                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                let resp = env
                    .on_server(async move {
                        let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                        let req = proto::UpdateChannel {
                            channel_id,
                            name,
                            topic,
                            parent_id,
                            bitrate: None,
                            user_limit: None,
                            text_in_voice: None,
                        };
                        channels::update(&ctx, req).await
                    })
                    .await?
                    .map_err(PluginEnv::api_error)?;

                match resp {
                    proto::response::Result::Channel(ch) => {
                        env.log.push(
                            proto::PluginLogLevel::Action,
                            &format!("updated channel #{}", ch.name),
                        );
                        Ok(Value::Table(channel_table(&lua, &ch)?))
                    }
                    _ => Err(raise("INTERNAL: unexpected response")),
                }
            }
        })?,
    )?;

    // channels.delete(id)
    let env_del = env.clone();
    table.raw_set(
        "delete",
        async_function(lua, move |_lua, id_val: Value| {
            let env = env_del.clone();
            async move {
                env.require(PluginPermission::ManageChannels)?;
                let channel_id = id_arg(id_val, "channel_id")?;
                env.check_rate_limit()?;
                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                let name = state
                    .guild()
                    .channels
                    .get(&channel_id)
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| channel_id.to_string());

                env.on_server(async move {
                    let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                    channels::delete(&ctx, proto::DeleteChannel { channel_id }).await
                })
                .await?
                .map_err(PluginEnv::api_error)?;

                env.log.push(
                    proto::PluginLogLevel::Action,
                    &format!("deleted channel {name}"),
                );

                Ok(())
            }
        })?,
    )?;

    // channels.reorder({ id... })
    let env_reorder = env.clone();
    table.raw_set(
        "reorder",
        async_function(lua, move |_lua, list_val: Value| {
            let env = env_reorder.clone();
            async move {
                env.require(PluginPermission::ManageChannels)?;
                let list_table = match list_val {
                    Value::Table(t) => t,
                    _ => return Err(raise("INVALID_ARGUMENT: channel ids must be a list")),
                };
                let mut channel_ids = Vec::new();
                for item in list_table.sequence_values::<Value>() {
                    channel_ids.push(id_arg(item?, "channel_id")?);
                }
                env.check_rate_limit()?;

                let positions = channel_ids
                    .into_iter()
                    .enumerate()
                    .map(|(i, id)| proto::ChannelPosition {
                        channel_id: id,
                        position: i as i32,
                    })
                    .collect();

                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                env.on_server(async move {
                    let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                    channels::reorder(&ctx, proto::ReorderChannels { positions }).await
                })
                .await?
                .map_err(PluginEnv::api_error)?;

                env.log
                    .push(proto::PluginLogLevel::Action, "reordered channels");

                Ok(())
            }
        })?,
    )?;

    // channels.set_overwrite(channel_id, { target = "role"|"member", id, allow = {}, deny = {} })
    let env_overwrite = env.clone();
    table.raw_set(
        "set_overwrite",
        async_function(lua, move |_lua, (cid_val, spec_val): (Value, Value)| {
            let env = env_overwrite.clone();
            async move {
                env.require(PluginPermission::ManageChannels)?;
                env.require(PluginPermission::ManageRoles)?;
                let channel_id = id_arg(cid_val, "channel_id")?;
                let spec = match spec_val {
                    Value::Table(t) => t,
                    _ => return Err(raise("INVALID_ARGUMENT: overwrite must be a table")),
                };

                let target_str: String = spec
                    .get("target")
                    .map_err(|_| raise("INVALID_ARGUMENT: target is required"))?;
                let target_kind = match target_str.as_str() {
                    "role" => proto::OverwriteTarget::Role as i32,
                    "member" => proto::OverwriteTarget::Member as i32,
                    _ => return Err(raise("INVALID_ARGUMENT: target must be 'role' or 'member'")),
                };
                let target_id = id_arg(spec.get("id")?, "target_id")?;

                let allow = match spec.get::<Value>("allow")? {
                    Value::Nil => 0,
                    Value::Table(t) => parse_permissions_table(t)?.bits(),
                    _ => return Err(raise("INVALID_ARGUMENT: allow must be a table")),
                };
                let deny = match spec.get::<Value>("deny")? {
                    Value::Nil => 0,
                    Value::Table(t) => parse_permissions_table(t)?.bits(),
                    _ => return Err(raise("INVALID_ARGUMENT: deny must be a table")),
                };
                env.check_rate_limit()?;

                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                let channel_name = state.guild().channel_name(channel_id);

                env.on_server(async move {
                    let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                    let req = proto::SetChannelOverwrite {
                        channel_id,
                        overwrite: Some(proto::PermissionOverwrite {
                            target_kind,
                            target_id,
                            allow,
                            deny,
                        }),
                    };
                    channels::set_overwrite(&ctx, req).await
                })
                .await?
                .map_err(PluginEnv::api_error)?;

                env.log.push(
                    proto::PluginLogLevel::Action,
                    &format!("set overwrite in channel {channel_name}"),
                );

                Ok(())
            }
        })?,
    )?;

    lua.globals().raw_set("channels", table)?;
    Ok(())
}

fn install_roles(lua: &Lua, env: &Arc<PluginEnv>) -> mlua::Result<()> {
    let table = lua.create_table()?;

    // roles.list()
    let env_list = env.clone();
    table.raw_set(
        "list",
        async_function(lua, move |lua, ()| {
            let env = env_list.clone();
            async move {
                env.require(PluginPermission::ReadRoles)?;
                let list = {
                    let state = env.state()?;
                    let guild = state.guild();

                    let mut list: Vec<proto::Role> =
                        guild.roles.values().map(|r| r.to_proto()).collect();
                    list.sort_by_key(|r| r.position);
                    list
                };

                let res_table = lua.create_table()?;
                for (idx, r) in list.iter().enumerate() {
                    res_table.raw_set(idx + 1, role_table(&lua, r)?)?;
                }
                Ok(Value::Table(res_table))
            }
        })?,
    )?;

    // roles.create({ name, color?, permissions = {}?, hoist?, mentionable? })
    let env_create = env.clone();
    table.raw_set(
        "create",
        async_function(lua, move |lua, spec_val: Value| {
            let env = env_create.clone();
            async move {
                env.require(PluginPermission::ManageRoles)?;
                let spec = match spec_val {
                    Value::Table(t) => t,
                    _ => return Err(raise("INVALID_ARGUMENT: spec must be a table")),
                };
                let name: String = spec
                    .get("name")
                    .map_err(|_| raise("INVALID_ARGUMENT: role name is required"))?;
                let color = get_opt_u32(&spec, "color")?.unwrap_or(0);
                let hoist = get_opt_bool(&spec, "hoist")?.unwrap_or(false);
                let mentionable = get_opt_bool(&spec, "mentionable")?.unwrap_or(false);

                let permissions = match spec.get::<Value>("permissions")? {
                    Value::Nil => 0,
                    Value::Table(perms_table) => parse_permissions_table(perms_table)?.bits(),
                    _ => return Err(raise("INVALID_ARGUMENT: permissions must be a table")),
                };
                env.check_rate_limit()?;

                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                let resp = env
                    .on_server(async move {
                        let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                        let req = proto::CreateRole {
                            name,
                            color,
                            permissions,
                            hoist,
                            mentionable,
                            max_upload_bytes: None,
                        };
                        roles::create(&ctx, req).await
                    })
                    .await?
                    .map_err(PluginEnv::api_error)?;

                match resp {
                    proto::response::Result::Role(r) => {
                        env.log.push(
                            proto::PluginLogLevel::Action,
                            &format!("created role @{}", r.name),
                        );
                        Ok(Value::Table(role_table(&lua, &r)?))
                    }
                    _ => Err(raise("INTERNAL: unexpected response")),
                }
            }
        })?,
    )?;

    // roles.update(id, { name?, color?, permissions?, hoist?, mentionable? })
    let env_update = env.clone();
    table.raw_set(
        "update",
        async_function(lua, move |lua, (id_val, spec_val): (Value, Value)| {
            let env = env_update.clone();
            async move {
                env.require(PluginPermission::ManageRoles)?;
                let role_id = id_arg(id_val, "role_id")?;
                let spec = match spec_val {
                    Value::Table(t) => t,
                    _ => return Err(raise("INVALID_ARGUMENT: spec must be a table")),
                };
                let name = get_opt_string(&spec, "name")?;
                let color = get_opt_u32(&spec, "color")?;
                let hoist = get_opt_bool(&spec, "hoist")?;
                let mentionable = get_opt_bool(&spec, "mentionable")?;

                let permissions = match spec.get::<Value>("permissions")? {
                    Value::Nil => None,
                    Value::Table(perms_table) => Some(parse_permissions_table(perms_table)?.bits()),
                    _ => return Err(raise("INVALID_ARGUMENT: permissions must be a table")),
                };
                env.check_rate_limit()?;

                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                let resp = env
                    .on_server(async move {
                        let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                        let req = proto::UpdateRole {
                            role_id,
                            name,
                            color,
                            permissions,
                            hoist,
                            mentionable,
                            max_upload_bytes: None,
                        };
                        roles::update(&ctx, req).await
                    })
                    .await?
                    .map_err(PluginEnv::api_error)?;

                match resp {
                    proto::response::Result::Role(r) => {
                        env.log.push(
                            proto::PluginLogLevel::Action,
                            &format!("updated role @{}", r.name),
                        );
                        Ok(Value::Table(role_table(&lua, &r)?))
                    }
                    _ => Err(raise("INTERNAL: unexpected response")),
                }
            }
        })?,
    )?;

    // roles.delete(id)
    let env_del = env.clone();
    table.raw_set(
        "delete",
        async_function(lua, move |_lua, id_val: Value| {
            let env = env_del.clone();
            async move {
                env.require(PluginPermission::ManageRoles)?;
                let role_id = id_arg(id_val, "role_id")?;
                env.check_rate_limit()?;
                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                let role_name = state.guild().role_name(role_id);

                env.on_server(async move {
                    let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                    roles::delete(&ctx, proto::DeleteRole { role_id }).await
                })
                .await?
                .map_err(PluginEnv::api_error)?;

                env.log.push(
                    proto::PluginLogLevel::Action,
                    &format!("deleted role {role_name}"),
                );

                Ok(())
            }
        })?,
    )?;

    // roles.reorder({ id... })
    let env_reorder = env.clone();
    table.raw_set(
        "reorder",
        async_function(lua, move |_lua, list_val: Value| {
            let env = env_reorder.clone();
            async move {
                env.require(PluginPermission::ManageRoles)?;
                let list_table = match list_val {
                    Value::Table(t) => t,
                    _ => return Err(raise("INVALID_ARGUMENT: role ids must be a list")),
                };
                let mut role_ids = Vec::new();
                for item in list_table.sequence_values::<Value>() {
                    role_ids.push(id_arg(item?, "role_id")?);
                }
                env.check_rate_limit()?;

                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                env.on_server(async move {
                    let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                    roles::reorder(&ctx, proto::ReorderRoles { role_ids }).await
                })
                .await?
                .map_err(PluginEnv::api_error)?;

                env.log
                    .push(proto::PluginLogLevel::Action, "reordered roles");

                Ok(())
            }
        })?,
    )?;

    lua.globals().raw_set("roles", table)?;
    Ok(())
}

fn install_members(lua: &Lua, env: &Arc<PluginEnv>) -> mlua::Result<()> {
    let table = lua.create_table()?;

    // members.list()
    let env_list = env.clone();
    table.raw_set(
        "list",
        async_function(lua, move |lua, ()| {
            let env = env_list.clone();
            async move {
                env.require(PluginPermission::ReadMembers)?;
                let list: Vec<proto::Member> = {
                    let state = env.state()?;
                    let guild = state.guild();
                    guild.members.values().map(|m| m.to_proto()).collect()
                };

                let res_table = lua.create_table()?;
                for (idx, member) in (1..).zip(list.iter()) {
                    res_table.raw_set(idx, member_table(&lua, member)?)?;
                }
                Ok(Value::Table(res_table))
            }
        })?,
    )?;

    // members.get(user_id)
    let env_get = env.clone();
    table.raw_set(
        "get",
        async_function(lua, move |lua, id_val: Value| {
            let env = env_get.clone();
            async move {
                env.require(PluginPermission::ReadMembers)?;
                let target_user_id = id_arg(id_val, "user_id")?;
                let m_opt = {
                    let state = env.state()?;
                    let guild = state.guild();
                    guild.members.get(&target_user_id).map(|m| m.to_proto())
                };

                if let Some(m) = m_opt {
                    return Ok(Value::Table(member_table(&lua, &m)?));
                }
                Ok(Value::Nil)
            }
        })?,
    )?;

    // members.add_role(user_id, role_id)
    let env_add_role = env.clone();
    table.raw_set(
        "add_role",
        async_function(lua, move |_lua, (uid_val, rid_val): (Value, Value)| {
            let env = env_add_role.clone();
            async move {
                env.require(PluginPermission::ManageRoles)?;
                let target_user_id = id_arg(uid_val, "user_id")?;
                let role_id = id_arg(rid_val, "role_id")?;
                env.check_rate_limit()?;
                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                let (role_name, target_name) = {
                    let g = state.guild();
                    (g.role_name(role_id), g.member_name(target_user_id))
                };

                env.on_server(async move {
                    let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                    roles::add_to_member(
                        &ctx,
                        proto::AddMemberRole {
                            user_id: target_user_id,
                            role_id,
                        },
                    )
                    .await
                })
                .await?
                .map_err(PluginEnv::api_error)?;

                env.log.push(
                    proto::PluginLogLevel::Action,
                    &format!("gave @{role_name} to {target_name}"),
                );

                Ok(())
            }
        })?,
    )?;

    // members.remove_role(user_id, role_id)
    let env_rm_role = env.clone();
    table.raw_set(
        "remove_role",
        async_function(lua, move |_lua, (uid_val, rid_val): (Value, Value)| {
            let env = env_rm_role.clone();
            async move {
                env.require(PluginPermission::ManageRoles)?;
                let target_user_id = id_arg(uid_val, "user_id")?;
                let role_id = id_arg(rid_val, "role_id")?;
                env.check_rate_limit()?;
                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                let (role_name, target_name) = {
                    let g = state.guild();
                    (g.role_name(role_id), g.member_name(target_user_id))
                };

                env.on_server(async move {
                    let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                    roles::remove_from_member(
                        &ctx,
                        proto::RemoveMemberRole {
                            user_id: target_user_id,
                            role_id,
                        },
                    )
                    .await
                })
                .await?
                .map_err(PluginEnv::api_error)?;

                env.log.push(
                    proto::PluginLogLevel::Action,
                    &format!("removed @{role_name} from {target_name}"),
                );

                Ok(())
            }
        })?,
    )?;

    // members.set_nickname(user_id, nickname?)
    let env_nick = env.clone();
    table.raw_set(
        "set_nickname",
        async_function(lua, move |_lua, (uid_val, nick_val): (Value, Value)| {
            let env = env_nick.clone();
            async move {
                env.require(PluginPermission::ManageNicknames)?;
                let target_user_id = id_arg(uid_val, "user_id")?;
                let nickname = string_arg_opt(nick_val, "nickname")?;
                env.check_rate_limit()?;

                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                let target_name = state
                    .guild()
                    .members
                    .get(&target_user_id)
                    .map(|m| m.user.display_name.clone())
                    .unwrap_or_else(|| target_user_id.to_string());

                env.on_server(async move {
                    let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                    members::update_nickname(
                        &ctx,
                        proto::UpdateNickname {
                            user_id: target_user_id,
                            nickname,
                        },
                    )
                    .await
                })
                .await?
                .map_err(PluginEnv::api_error)?;

                env.log.push(
                    proto::PluginLogLevel::Action,
                    &format!("updated nickname for {target_name}"),
                );

                Ok(())
            }
        })?,
    )?;

    // members.kick(user_id, reason?)
    let env_kick = env.clone();
    table.raw_set(
        "kick",
        async_function(lua, move |_lua, (uid_val, reason_val): (Value, Value)| {
            let env = env_kick.clone();
            async move {
                env.require(PluginPermission::KickMembers)?;
                let target_user_id = id_arg(uid_val, "user_id")?;
                let reason = string_arg_opt(reason_val, "reason")?;
                env.check_rate_limit()?;
                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                let target_name = state
                    .guild()
                    .members
                    .get(&target_user_id)
                    .map(|m| m.user.display_name.clone())
                    .unwrap_or_else(|| target_user_id.to_string());

                env.on_server(async move {
                    let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                    members::kick(
                        &ctx,
                        proto::KickMember {
                            user_id: target_user_id,
                            reason,
                        },
                    )
                    .await
                })
                .await?
                .map_err(PluginEnv::api_error)?;

                env.log.push(
                    proto::PluginLogLevel::Action,
                    &format!("kicked {target_name}"),
                );

                Ok(())
            }
        })?,
    )?;

    // members.ban(user_id, reason?)
    let env_ban = env.clone();
    table.raw_set(
        "ban",
        async_function(lua, move |_lua, (uid_val, reason_val): (Value, Value)| {
            let env = env_ban.clone();
            async move {
                env.require(PluginPermission::BanMembers)?;
                let target_user_id = id_arg(uid_val, "user_id")?;
                let reason = string_arg_opt(reason_val, "reason")?;
                env.check_rate_limit()?;
                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                let target_name = state
                    .guild()
                    .members
                    .get(&target_user_id)
                    .map(|m| m.user.display_name.clone())
                    .unwrap_or_else(|| target_user_id.to_string());

                env.on_server(async move {
                    let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                    members::ban(
                        &ctx,
                        proto::BanMember {
                            user_id: target_user_id,
                            reason,
                        },
                    )
                    .await
                })
                .await?
                .map_err(PluginEnv::api_error)?;

                env.log.push(
                    proto::PluginLogLevel::Action,
                    &format!("banned {target_name}"),
                );

                Ok(())
            }
        })?,
    )?;

    // members.unban(user_id)
    let env_unban = env.clone();
    table.raw_set(
        "unban",
        async_function(lua, move |_lua, uid_val: Value| {
            let env = env_unban.clone();
            async move {
                env.require(PluginPermission::BanMembers)?;
                let target_user_id = id_arg(uid_val, "user_id")?;
                env.check_rate_limit()?;
                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                env.on_server(async move {
                    let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                    members::unban(
                        &ctx,
                        proto::UnbanMember {
                            user_id: target_user_id,
                        },
                    )
                    .await
                })
                .await?
                .map_err(PluginEnv::api_error)?;

                env.log.push(
                    proto::PluginLogLevel::Action,
                    &format!("unbanned {target_user_id}"),
                );

                Ok(())
            }
        })?,
    )?;

    lua.globals().raw_set("members", table)?;

    // bans.list()
    let bans_table = lua.create_table()?;
    let env_bans = env.clone();
    bans_table.raw_set(
        "list",
        async_function(lua, move |lua, ()| {
            let env = env_bans.clone();
            async move {
                env.require(PluginPermission::BanMembers)?;
                env.check_rate_limit()?;
                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                let resp = env
                    .on_server(async move {
                        let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                        members::fetch_bans(&ctx).await
                    })
                    .await?
                    .map_err(PluginEnv::api_error)?;

                let list = match resp {
                    proto::response::Result::Bans(b) => b.bans,
                    _ => Vec::new(),
                };

                let res_table = lua.create_table()?;
                for (idx, b) in list.into_iter().enumerate() {
                    let item = lua.create_table()?;
                    if let Some(user) = b.user {
                        let user_table = lua.create_table()?;
                        user_table.raw_set("id", user.id.to_string())?;
                        user_table.raw_set("name", user.display_name)?;
                        item.raw_set("user", user_table)?;
                    }
                    if let Some(reason) = b.reason {
                        item.raw_set("reason", reason)?;
                    }
                    res_table.raw_set(idx + 1, item)?;
                }
                Ok(Value::Table(res_table))
            }
        })?,
    )?;
    lua.globals().raw_set("bans", bans_table)?;

    Ok(())
}

fn install_invites(lua: &Lua, env: &Arc<PluginEnv>) -> mlua::Result<()> {
    let table = lua.create_table()?;

    // invites.list()
    let env_list = env.clone();
    table.raw_set(
        "list",
        async_function(lua, move |lua, ()| {
            let env = env_list.clone();
            async move {
                env.require(PluginPermission::ReadInvites)?;
                env.check_rate_limit()?;
                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                let resp = env
                    .on_server(async move {
                        let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                        invites::fetch(&ctx).await
                    })
                    .await?
                    .map_err(PluginEnv::api_error)?;

                let list = match resp {
                    proto::response::Result::Invites(invs) => invs.invites,
                    _ => Vec::new(),
                };

                let res_table = lua.create_table()?;
                for (idx, inv) in list.iter().enumerate() {
                    res_table.raw_set(idx + 1, invite_table(&lua, inv)?)?;
                }
                Ok(Value::Table(res_table))
            }
        })?,
    )?;

    // invites.create({ max_uses?, expires_in_s? })
    let env_create = env.clone();
    table.raw_set(
        "create",
        async_function(lua, move |lua, spec_val: Value| {
            let env = env_create.clone();
            async move {
                env.require(PluginPermission::CreateInvites)?;
                let (max_uses, expires_in_s) = match spec_val {
                    Value::Nil => (None, None),
                    Value::Table(t) => (
                        get_opt_u32(&t, "max_uses")?,
                        get_opt_u32(&t, "expires_in_s")?,
                    ),
                    _ => return Err(raise("INVALID_ARGUMENT: spec must be a table or nil")),
                };
                env.check_rate_limit()?;

                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                let resp = env
                    .on_server(async move {
                        let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                        let req = proto::CreateInvite {
                            domain_id: None,
                            max_uses,
                            expires_in_s,
                        };
                        invites::create(&ctx, req).await
                    })
                    .await?
                    .map_err(PluginEnv::api_error)?;

                match resp {
                    proto::response::Result::Invite(inv) => {
                        env.log.push(
                            proto::PluginLogLevel::Action,
                            &format!("created invite {}", inv.code),
                        );
                        Ok(Value::Table(invite_table(&lua, &inv)?))
                    }
                    _ => Err(raise("INTERNAL: unexpected response")),
                }
            }
        })?,
    )?;

    lua.globals().raw_set("invites", table)?;
    Ok(())
}

fn install_server(lua: &Lua, env: &Arc<PluginEnv>) -> mlua::Result<()> {
    let table = lua.create_table()?;

    // server.get()
    let env_get = env.clone();
    table.raw_set(
        "get",
        async_function(lua, move |lua, ()| {
            let env = env_get.clone();
            async move {
                env.require(PluginPermission::ReadServer)?;
                let info = {
                    let state = env.state()?;
                    let guild = state.guild();
                    guild.server_info()
                };
                Ok(Value::Table(server_info_table(&lua, &info)?))
            }
        })?,
    )?;

    lua.globals().raw_set("server", table)?;
    Ok(())
}
