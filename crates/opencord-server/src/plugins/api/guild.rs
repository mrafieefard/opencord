// SPDX-License-Identifier: MPL-2.0

//! The `channels`, `roles`, `members`, `bans`, `invites` and `server` globals (§10.3).

use std::sync::Arc;

use mlua::{Lua, Table, Value};
use opencord_common::permissions::Permissions;
use opencord_proto::v1 as proto;

use super::{async_function, raise};
use crate::handlers::Ctx;
use crate::handlers::{channels, invites, members, roles};
use crate::plugins::permissions::PluginPermission;
use crate::plugins::runtime::PluginEnv;

fn parse_id(val: Value, name: &'static str) -> mlua::Result<i64> {
    match val {
        Value::String(s) => s
            .to_str()?
            .parse::<i64>()
            .map_err(|_| raise(format!("INVALID_ARGUMENT: {name} must be an id string"))),
        _ => Err(raise(format!(
            "INVALID_ARGUMENT: {name} must be an id string"
        ))),
    }
}

fn member_permission_from_name(name: &str) -> Option<Permissions> {
    match name.to_ascii_lowercase().as_str() {
        "view_channel" => Some(Permissions::VIEW_CHANNEL),
        "send_messages" => Some(Permissions::SEND_MESSAGES),
        "read_history" => Some(Permissions::READ_HISTORY),
        "manage_messages" => Some(Permissions::MANAGE_MESSAGES),
        "manage_channels" => Some(Permissions::MANAGE_CHANNELS),
        "manage_roles" => Some(Permissions::MANAGE_ROLES),
        "kick_members" => Some(Permissions::KICK_MEMBERS),
        "ban_members" => Some(Permissions::BAN_MEMBERS),
        "create_invite" => Some(Permissions::CREATE_INVITE),
        "manage_server" => Some(Permissions::MANAGE_SERVER),
        "attach_files" => Some(Permissions::ATTACH_FILES),
        "mention_everyone" => Some(Permissions::MENTION_EVERYONE),
        "change_nickname" => Some(Permissions::CHANGE_NICKNAME),
        "manage_nicknames" => Some(Permissions::MANAGE_NICKNAMES),
        "send_voice_messages" => Some(Permissions::SEND_VOICE_MESSAGES),
        "manage_plugins" => Some(Permissions::MANAGE_PLUGINS),
        "connect" => Some(Permissions::CONNECT),
        "speak" => Some(Permissions::SPEAK),
        "video" => Some(Permissions::VIDEO),
        "screenshare" => Some(Permissions::SCREENSHARE),
        "mute_members" => Some(Permissions::MUTE_MEMBERS),
        "deafen_members" => Some(Permissions::DEAFEN_MEMBERS),
        "move_members" => Some(Permissions::MOVE_MEMBERS),
        "priority_speaker" => Some(Permissions::PRIORITY_SPEAKER),
        "use_voice_activity" => Some(Permissions::USE_VOICE_ACTIVITY),
        "use_soundboard" => Some(Permissions::USE_SOUNDBOARD),
        "use_external_sounds" => Some(Permissions::USE_EXTERNAL_SOUNDS),
        "manage_soundboard" => Some(Permissions::MANAGE_SOUNDBOARD),
        "music_add" => Some(Permissions::MUSIC_ADD),
        "music_control" => Some(Permissions::MUSIC_CONTROL),
        "music_manage_queue" => Some(Permissions::MUSIC_MANAGE_QUEUE),
        "music_manage" => Some(Permissions::MUSIC_MANAGE),
        "music_all_local_songs" => Some(Permissions::MUSIC_ALL_LOCAL_SONGS),
        "administrator" => Some(Permissions::ADMINISTRATOR),
        _ => None,
    }
}

fn parse_permissions_table(table: Table) -> mlua::Result<Permissions> {
    let mut perms = Permissions::empty();
    for item in table.sequence_values::<Value>() {
        let val = item?;
        if let Value::String(s) = val {
            let name = s.to_str()?;
            let bit = member_permission_from_name(&name)
                .ok_or_else(|| raise(format!("INVALID_ARGUMENT: unknown permission '{name}'")))?;
            perms |= bit;
        }
    }
    Ok(perms)
}

fn permissions_to_names(perms: Permissions) -> Vec<String> {
    let all = [
        (Permissions::VIEW_CHANNEL, "view_channel"),
        (Permissions::SEND_MESSAGES, "send_messages"),
        (Permissions::READ_HISTORY, "read_history"),
        (Permissions::MANAGE_MESSAGES, "manage_messages"),
        (Permissions::MANAGE_CHANNELS, "manage_channels"),
        (Permissions::MANAGE_ROLES, "manage_roles"),
        (Permissions::KICK_MEMBERS, "kick_members"),
        (Permissions::BAN_MEMBERS, "ban_members"),
        (Permissions::CREATE_INVITE, "create_invite"),
        (Permissions::MANAGE_SERVER, "manage_server"),
        (Permissions::ATTACH_FILES, "attach_files"),
        (Permissions::MENTION_EVERYONE, "mention_everyone"),
        (Permissions::CHANGE_NICKNAME, "change_nickname"),
        (Permissions::MANAGE_NICKNAMES, "manage_nicknames"),
        (Permissions::SEND_VOICE_MESSAGES, "send_voice_messages"),
        (Permissions::MANAGE_PLUGINS, "manage_plugins"),
        (Permissions::CONNECT, "connect"),
        (Permissions::SPEAK, "speak"),
        (Permissions::VIDEO, "video"),
        (Permissions::SCREENSHARE, "screenshare"),
        (Permissions::MUTE_MEMBERS, "mute_members"),
        (Permissions::DEAFEN_MEMBERS, "deafen_members"),
        (Permissions::MOVE_MEMBERS, "move_members"),
        (Permissions::PRIORITY_SPEAKER, "priority_speaker"),
        (Permissions::USE_VOICE_ACTIVITY, "use_voice_activity"),
        (Permissions::USE_SOUNDBOARD, "use_soundboard"),
        (Permissions::USE_EXTERNAL_SOUNDS, "use_external_sounds"),
        (Permissions::MANAGE_SOUNDBOARD, "manage_soundboard"),
        (Permissions::MUSIC_ADD, "music_add"),
        (Permissions::MUSIC_CONTROL, "music_control"),
        (Permissions::MUSIC_MANAGE_QUEUE, "music_manage_queue"),
        (Permissions::MUSIC_MANAGE, "music_manage"),
        (Permissions::MUSIC_ALL_LOCAL_SONGS, "music_all_local_songs"),
        (Permissions::ADMINISTRATOR, "administrator"),
    ];
    let mut names = Vec::new();
    for (bit, name) in all {
        if perms.contains(bit) {
            names.push(name.to_owned());
        }
    }
    names
}

fn channel_table(lua: &Lua, ch: &proto::Channel) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.raw_set("id", ch.id.to_string())?;
    t.raw_set("name", ch.name.clone())?;
    let kind_str = match proto::ChannelKind::try_from(ch.kind).unwrap_or(proto::ChannelKind::Text) {
        proto::ChannelKind::Text => "text",
        proto::ChannelKind::Voice => "voice",
        proto::ChannelKind::Category => "category",
        proto::ChannelKind::Unspecified => "text",
    };
    t.raw_set("kind", kind_str)?;
    if let Some(ref topic) = ch.topic
        && !topic.is_empty()
    {
        t.raw_set("topic", topic.clone())?;
    }
    if let Some(parent_id) = ch.parent_id {
        t.raw_set("parent_id", parent_id.to_string())?;
    }
    t.raw_set("position", ch.position)?;
    Ok(t)
}

fn role_table(lua: &Lua, r: &proto::Role) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.raw_set("id", r.id.to_string())?;
    t.raw_set("name", r.name.clone())?;
    t.raw_set("color", r.color)?;
    t.raw_set("hoist", r.hoist)?;
    t.raw_set("mentionable", r.mentionable)?;
    t.raw_set("position", r.position)?;
    let perms = Permissions::from_bits_truncate(r.permissions);
    let perm_names = permissions_to_names(perms);
    let perms_table = lua.create_sequence_from(perm_names)?;
    t.raw_set("permissions", perms_table)?;
    Ok(t)
}

fn member_table(lua: &Lua, m: &proto::Member) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    let user = m.user.as_ref();
    let id_str = user.map_or(0, |u| u.id).to_string();
    t.raw_set("id", id_str)?;
    let display_name = user.map(|u| u.display_name.clone()).unwrap_or_default();
    t.raw_set("name", display_name)?;
    if let Some(ref nick) = m.nickname {
        t.raw_set("nickname", nick.clone())?;
    }
    let roles_table = lua.create_sequence_from(m.role_ids.iter().map(|id| id.to_string()))?;
    t.raw_set("roles", roles_table)?;
    if let Some(user) = user
        && !user.avatar_hash.is_empty()
    {
        t.raw_set("avatar_hash", user.avatar_hash.clone())?;
    }
    t.raw_set("joined_at_ms", m.joined_at_ms)?;
    Ok(t)
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
                let channel_id = parse_id(id_val, "channel_id")?;
                let state = env.state()?;
                let guild = state.guild();
                let user_id = env.user_id;

                if let Some(ch) = guild.channels.get(&channel_id)
                    && guild.can_view(user_id, channel_id)
                {
                    return Ok(Value::Table(channel_table(&lua, &ch.to_proto())?));
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
                let kind_str: String = spec.get("kind").unwrap_or_else(|_| "text".to_owned());
                let kind = match kind_str.as_str() {
                    "text" => proto::ChannelKind::Text as i32,
                    "voice" => proto::ChannelKind::Voice as i32,
                    "category" => proto::ChannelKind::Category as i32,
                    _ => return Err(raise("INVALID_ARGUMENT: unknown channel kind")),
                };
                let topic: Option<String> = spec.get("topic").ok();
                let parent_id = if let Ok(pid_val) = spec.get::<Value>("parent_id") {
                    if pid_val == Value::Nil {
                        None
                    } else {
                        Some(parse_id(pid_val, "parent_id")?)
                    }
                } else {
                    None
                };

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
                let channel_id = parse_id(id_val, "channel_id")?;
                let spec = match spec_val {
                    Value::Table(t) => t,
                    _ => return Err(raise("INVALID_ARGUMENT: spec must be a table")),
                };
                let name: Option<String> = spec.get("name").ok();
                let topic: Option<String> = spec.get("topic").ok();
                let parent_id = if let Ok(pid_val) = spec.get::<Value>("parent_id") {
                    if pid_val == Value::Nil {
                        None
                    } else {
                        Some(parse_id(pid_val, "parent_id")?)
                    }
                } else {
                    None
                };

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
                let channel_id = parse_id(id_val, "channel_id")?;
                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                env.on_server(async move {
                    let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                    channels::delete(&ctx, proto::DeleteChannel { channel_id }).await
                })
                .await?
                .map_err(PluginEnv::api_error)?;

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
                    channel_ids.push(parse_id(item?, "channel_id")?);
                }

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
                let channel_id = parse_id(cid_val, "channel_id")?;
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
                let target_id = parse_id(spec.get("id")?, "target_id")?;

                let allow = if let Ok(allow_table) = spec.get::<Table>("allow") {
                    parse_permissions_table(allow_table)?.bits()
                } else {
                    0
                };
                let deny = if let Ok(deny_table) = spec.get::<Table>("deny") {
                    parse_permissions_table(deny_table)?.bits()
                } else {
                    0
                };

                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

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
                let state = env.state()?;
                let guild = state.guild();

                let mut list: Vec<proto::Role> =
                    guild.roles.values().map(|r| r.to_proto()).collect();
                list.sort_by_key(|r| r.position);

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
                let color = spec.get::<u32>("color").unwrap_or(0);
                let hoist = spec.get::<bool>("hoist").unwrap_or(false);
                let mentionable = spec.get::<bool>("mentionable").unwrap_or(false);

                let permissions = if let Ok(perms_table) = spec.get::<Table>("permissions") {
                    parse_permissions_table(perms_table)?.bits()
                } else {
                    0
                };

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
                    proto::response::Result::Role(r) => Ok(Value::Table(role_table(&lua, &r)?)),
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
                let role_id = parse_id(id_val, "role_id")?;
                let spec = match spec_val {
                    Value::Table(t) => t,
                    _ => return Err(raise("INVALID_ARGUMENT: spec must be a table")),
                };
                let name: Option<String> = spec.get("name").ok();
                let color: Option<u32> = spec.get("color").ok();
                let hoist: Option<bool> = spec.get("hoist").ok();
                let mentionable: Option<bool> = spec.get("mentionable").ok();

                let permissions = if let Ok(perms_table) = spec.get::<Table>("permissions") {
                    Some(parse_permissions_table(perms_table)?.bits())
                } else {
                    None
                };

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
                    proto::response::Result::Role(r) => Ok(Value::Table(role_table(&lua, &r)?)),
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
                let role_id = parse_id(id_val, "role_id")?;
                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                env.on_server(async move {
                    let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                    roles::delete(&ctx, proto::DeleteRole { role_id }).await
                })
                .await?
                .map_err(PluginEnv::api_error)?;

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
                    role_ids.push(parse_id(item?, "role_id")?);
                }

                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                env.on_server(async move {
                    let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                    roles::reorder(&ctx, proto::ReorderRoles { role_ids }).await
                })
                .await?
                .map_err(PluginEnv::api_error)?;

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
                let state = env.state()?;
                let guild = state.guild();

                let res_table = lua.create_table()?;
                for (idx, member) in (1..).zip(guild.members.values()) {
                    res_table.raw_set(idx, member_table(&lua, &member.to_proto())?)?;
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
                let target_user_id = parse_id(id_val, "user_id")?;
                let state = env.state()?;
                let guild = state.guild();

                if let Some(m) = guild.members.get(&target_user_id) {
                    return Ok(Value::Table(member_table(&lua, &m.to_proto())?));
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
                let target_user_id = parse_id(uid_val, "user_id")?;
                let role_id = parse_id(rid_val, "role_id")?;
                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

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
                let target_user_id = parse_id(uid_val, "user_id")?;
                let role_id = parse_id(rid_val, "role_id")?;
                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

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
                let target_user_id = parse_id(uid_val, "user_id")?;
                let nickname = match nick_val {
                    Value::String(s) => Some(s.to_str()?.to_owned()),
                    Value::Nil => None,
                    _ => return Err(raise("INVALID_ARGUMENT: nickname must be a string or nil")),
                };

                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

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

                Ok(())
            }
        })?,
    )?;

    // members.kick(user_id, reason?)
    let env_kick = env.clone();
    table.raw_set(
        "kick",
        async_function(
            lua,
            move |_lua, (uid_val, reason_val): (Value, Option<String>)| {
                let env = env_kick.clone();
                async move {
                    env.require(PluginPermission::KickMembers)?;
                    let target_user_id = parse_id(uid_val, "user_id")?;
                    let state = env.state()?;
                    let plugin_id = env.plugin_id.clone();
                    let user_id = env.user_id;

                    env.on_server(async move {
                        let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                        members::kick(
                            &ctx,
                            proto::KickMember {
                                user_id: target_user_id,
                                reason: reason_val,
                            },
                        )
                        .await
                    })
                    .await?
                    .map_err(PluginEnv::api_error)?;

                    Ok(())
                }
            },
        )?,
    )?;

    // members.ban(user_id, reason?)
    let env_ban = env.clone();
    table.raw_set(
        "ban",
        async_function(
            lua,
            move |_lua, (uid_val, reason_val): (Value, Option<String>)| {
                let env = env_ban.clone();
                async move {
                    env.require(PluginPermission::BanMembers)?;
                    let target_user_id = parse_id(uid_val, "user_id")?;
                    let state = env.state()?;
                    let plugin_id = env.plugin_id.clone();
                    let user_id = env.user_id;

                    env.on_server(async move {
                        let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                        members::ban(
                            &ctx,
                            proto::BanMember {
                                user_id: target_user_id,
                                reason: reason_val,
                            },
                        )
                        .await
                    })
                    .await?
                    .map_err(PluginEnv::api_error)?;

                    Ok(())
                }
            },
        )?,
    )?;

    // members.unban(user_id)
    let env_unban = env.clone();
    table.raw_set(
        "unban",
        async_function(lua, move |_lua, uid_val: Value| {
            let env = env_unban.clone();
            async move {
                env.require(PluginPermission::BanMembers)?;
                let target_user_id = parse_id(uid_val, "user_id")?;
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
                    Value::Table(t) => (
                        t.get::<u32>("max_uses").ok(),
                        t.get::<u32>("expires_in_s").ok(),
                    ),
                    _ => (None, None),
                };

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
                let state = env.state()?;
                let guild = state.guild();

                let res = lua.create_table()?;
                res.raw_set("id", hex::encode(guild.meta.server_id))?;
                res.raw_set("name", guild.meta.name.clone())?;
                res.raw_set("description", guild.meta.description.clone())?;
                res.raw_set("owner_id", guild.meta.owner_id.unwrap_or(0).to_string())?;
                Ok(Value::Table(res))
            }
        })?,
    )?;

    lua.globals().raw_set("server", table)?;
    Ok(())
}
