// SPDX-License-Identifier: MPL-2.0

//! Lua API for voice, soundboard and music (§10.3, §10.9).

use std::sync::Arc;

use mlua::{Lua, Table, Value};
use opencord_proto::v1 as proto;
use proto::response::Result as Response;

use super::{async_function, raise};
use crate::error::ApiError;
use crate::guild::Guild;
use crate::handlers::Ctx;
use crate::handlers::{music, soundboard, voice};
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

fn parse_id_opt(val: Value, name: &'static str) -> mlua::Result<Option<i64>> {
    match val {
        Value::Nil => Ok(None),
        Value::String(s) => {
            let str_val = s.to_str()?;
            if str_val.is_empty() {
                Ok(None)
            } else {
                str_val
                    .parse::<i64>()
                    .map(Some)
                    .map_err(|_| raise(format!("INVALID_ARGUMENT: {name} must be an id string")))
            }
        }
        _ => Err(raise(format!(
            "INVALID_ARGUMENT: {name} must be an id string"
        ))),
    }
}

fn user_table(lua: &Lua, guild: &Guild, user_id: i64) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.raw_set("id", user_id.to_string())?;
    if let Some(plugin_user) = guild.plugin_users.get(&user_id) {
        t.raw_set("name", plugin_user.name.clone())?;
        t.raw_set("plugin", plugin_user.plugin_id.clone())?;
    } else if let Some(member) = guild.members.get(&user_id) {
        let name = member
            .nickname
            .clone()
            .unwrap_or_else(|| member.user.display_name.clone());
        t.raw_set("name", name)?;
    } else {
        t.raw_set("name", "")?;
    }
    Ok(t)
}

fn track_table(lua: &Lua, track: &proto::MusicTrack) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.raw_set("title", track.title.clone())?;
    t.raw_set("artist", track.artist.clone())?;
    t.raw_set("duration_ms", track.duration_ms)?;
    t.raw_set("is_stream", track.is_stream)?;
    t.raw_set("is_seekable", track.is_seekable)?;
    t.raw_set("source", track.source.clone())?;
    t.raw_set("source_display_name", track.source_display_name.clone())?;
    t.raw_set("uri", track.uri.clone())?;
    t.raw_set("artwork_id", track.artwork_id.clone())?;
    t.raw_set("local_song_id", track.local_song_id.to_string())?;
    t.raw_set("removable", track.removable)?;
    Ok(t)
}

fn item_table(lua: &Lua, item: &proto::MusicQueueItem) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.raw_set("item_id", item.item_id.clone())?;
    t.raw_set("requester_id", item.requester_id.to_string())?;
    t.raw_set("added_at_ms", item.added_at_ms)?;
    if let Some(ref track) = item.track {
        t.raw_set("track", track_table(lua, track)?)?;
    }
    Ok(t)
}

fn session_table(lua: &Lua, s: &proto::MusicSession) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.raw_set("channel_id", s.channel_id.to_string())?;
    t.raw_set("version", s.version)?;
    let play_state = match proto::MusicPlayState::try_from(s.state).unwrap_or_default() {
        proto::MusicPlayState::Idle => "idle",
        proto::MusicPlayState::Loading => "loading",
        proto::MusicPlayState::Playing => "playing",
        proto::MusicPlayState::Paused => "paused",
        proto::MusicPlayState::Unspecified => "unspecified",
    };
    t.raw_set("state", play_state)?;
    if let Some(ref np) = s.now_playing {
        t.raw_set("now_playing", item_table(lua, np)?)?;
    }
    let queue_table = lua.create_table()?;
    for (idx, item) in s.queue.iter().enumerate() {
        queue_table.raw_set(idx + 1, item_table(lua, item)?)?;
    }
    t.raw_set("queue", queue_table)?;

    let history_table = lua.create_table()?;
    for (idx, item) in s.history.iter().enumerate() {
        history_table.raw_set(idx + 1, item_table(lua, item)?)?;
    }
    t.raw_set("history", history_table)?;

    t.raw_set("position_ms", s.position_ms)?;
    t.raw_set("server_time_ms", s.server_time_ms)?;
    let repeat = match proto::MusicRepeat::try_from(s.repeat).unwrap_or_default() {
        proto::MusicRepeat::Off => "off",
        proto::MusicRepeat::Queue => "queue",
        proto::MusicRepeat::Track => "track",
        proto::MusicRepeat::Unspecified => "unspecified",
    };
    t.raw_set("repeat", repeat)?;
    t.raw_set("shuffle", s.shuffle)?;
    t.raw_set("waiting_for_node", s.waiting_for_node)?;
    t.raw_set("paused_empty", s.paused_empty)?;
    Ok(t)
}

pub fn install(lua: &Lua, env: &Arc<PluginEnv>) -> mlua::Result<()> {
    install_voice(lua, env)?;
    install_soundboard(lua, env)?;
    install_music(lua, env)?;
    Ok(())
}

fn install_voice(lua: &Lua, env: &Arc<PluginEnv>) -> mlua::Result<()> {
    let table = lua.create_table()?;

    // voice.states(channel_id?)
    let env_states = env.clone();
    table.raw_set(
        "states",
        async_function(lua, move |lua, ch_val: Value| {
            let env = env_states.clone();
            async move {
                env.require(PluginPermission::ReadVoice)?;
                let ch_opt = parse_id_opt(ch_val, "channel_id")?;
                let state = env.state()?;
                let guild = state.guild();
                let voice_states = state.voice();

                let raw_states = voice_states.visible_to(&guild, env.user_id);
                let res_table = lua.create_table()?;
                let mut idx = 1;
                for st in raw_states {
                    if let Some(target_ch) = ch_opt
                        && st.channel_id != Some(target_ch)
                    {
                        continue;
                    }
                    let item = lua.create_table()?;
                    item.raw_set("user", user_table(&lua, &guild, st.user_id)?)?;
                    item.raw_set("channel_id", st.channel_id.unwrap_or(0).to_string())?;
                    item.raw_set("self_mute", st.self_mute)?;
                    item.raw_set("self_deaf", st.self_deaf)?;
                    item.raw_set("server_mute", st.server_mute)?;
                    item.raw_set("server_deaf", st.server_deaf)?;
                    item.raw_set("video", st.self_video)?;
                    item.raw_set("streaming", st.self_stream)?;
                    res_table.raw_set(idx, item)?;
                    idx += 1;
                }
                Ok(Value::Table(res_table))
            }
        })?,
    )?;

    // voice.move(user_id, channel_id)
    let env_move = env.clone();
    table.raw_set(
        "move",
        async_function(lua, move |_lua, (uid_val, ch_val): (Value, Value)| {
            let env = env_move.clone();
            async move {
                env.require(PluginPermission::MoveMembers)?;
                let target_user_id = parse_id(uid_val, "user_id")?;
                let destination_channel_id = parse_id(ch_val, "channel_id")?;

                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                env.on_server(async move {
                    let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                    voice::move_member(
                        &ctx,
                        proto::MoveMember {
                            user_id: target_user_id,
                            channel_id: destination_channel_id,
                        },
                    )
                    .await
                })
                .await?
                .map_err(PluginEnv::api_error)?;

                env.log.push(
                    proto::PluginLogLevel::Info,
                    &format!("moved member {target_user_id} to channel {destination_channel_id}"),
                );

                Ok(())
            }
        })?,
    )?;

    // voice.disconnect(user_id)
    let env_disconnect = env.clone();
    table.raw_set(
        "disconnect",
        async_function(lua, move |_lua, uid_val: Value| {
            let env = env_disconnect.clone();
            async move {
                env.require(PluginPermission::MoveMembers)?;
                let target_user_id = parse_id(uid_val, "user_id")?;

                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                env.on_server(async move {
                    let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                    voice::disconnect_member(
                        &ctx,
                        proto::DisconnectMember {
                            user_id: target_user_id,
                        },
                    )
                    .await
                })
                .await?
                .map_err(PluginEnv::api_error)?;

                env.log.push(
                    proto::PluginLogLevel::Info,
                    &format!("disconnected member {target_user_id} from voice"),
                );

                Ok(())
            }
        })?,
    )?;

    // voice.mute(user_id, bool)
    let env_mute = env.clone();
    table.raw_set(
        "mute",
        async_function(lua, move |_lua, (uid_val, val): (Value, bool)| {
            let env = env_mute.clone();
            async move {
                env.require(PluginPermission::MuteMembers)?;
                let target_user_id = parse_id(uid_val, "user_id")?;

                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                env.on_server(async move {
                    let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                    voice::server_mute(
                        &ctx,
                        proto::ServerMuteMember {
                            user_id: target_user_id,
                            value: val,
                        },
                    )
                    .await
                })
                .await?
                .map_err(PluginEnv::api_error)?;

                env.log.push(
                    proto::PluginLogLevel::Info,
                    &format!("server mute member {target_user_id}: {val}"),
                );

                Ok(())
            }
        })?,
    )?;

    // voice.deafen(user_id, bool)
    let env_deafen = env.clone();
    table.raw_set(
        "deafen",
        async_function(lua, move |_lua, (uid_val, val): (Value, bool)| {
            let env = env_deafen.clone();
            async move {
                env.require(PluginPermission::DeafenMembers)?;
                let target_user_id = parse_id(uid_val, "user_id")?;

                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                env.on_server(async move {
                    let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                    voice::server_deafen(
                        &ctx,
                        proto::ServerDeafenMember {
                            user_id: target_user_id,
                            value: val,
                        },
                    )
                    .await
                })
                .await?
                .map_err(PluginEnv::api_error)?;

                env.log.push(
                    proto::PluginLogLevel::Info,
                    &format!("server deafen member {target_user_id}: {val}"),
                );

                Ok(())
            }
        })?,
    )?;

    lua.globals().raw_set("voice", table)?;
    Ok(())
}

fn install_soundboard(lua: &Lua, env: &Arc<PluginEnv>) -> mlua::Result<()> {
    let table = lua.create_table()?;

    // soundboard.play(channel_id, sound_id)
    let env_play = env.clone();
    table.raw_set(
        "play",
        async_function(lua, move |_lua, (ch_val, sid_val): (Value, Value)| {
            let env = env_play.clone();
            async move {
                env.require(PluginPermission::PlaySoundboard)?;
                let channel_id = parse_id(ch_val, "channel_id")?;
                let sound_id_str = match sid_val {
                    Value::String(s) => s.to_str()?.to_owned(),
                    _ => {
                        return Err(raise(
                            "INVALID_ARGUMENT: sound_id must be a sound id or server ref",
                        ));
                    }
                };

                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                let sound_ref = if let Ok(id) = sound_id_str.parse::<i64>() {
                    let guild = state.guild();
                    let s = guild
                        .sounds
                        .get(&id)
                        .ok_or_else(|| raise("NOT_FOUND: sound"))?;
                    format!("server:{}", hex::encode(s.sha256))
                } else if sound_id_str.starts_with("server:") {
                    sound_id_str
                } else {
                    return Err(raise(
                        "INVALID_ARGUMENT: sound_id must be a sound id or server ref",
                    ));
                };

                let sound_ref_clone = sound_ref.clone();
                env.on_server(async move {
                    let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                    soundboard::play(
                        &ctx,
                        proto::PlaySoundboardSound {
                            channel_id,
                            sound_ref: sound_ref_clone,
                            ..Default::default()
                        },
                    )
                    .await
                })
                .await?
                .map_err(PluginEnv::api_error)?;

                env.log.push(
                    proto::PluginLogLevel::Info,
                    &format!("played sound {sound_ref} in channel {channel_id}"),
                );

                Ok(())
            }
        })?,
    )?;

    lua.globals().raw_set("soundboard", table)?;
    Ok(())
}

fn install_music(lua: &Lua, env: &Arc<PluginEnv>) -> mlua::Result<()> {
    let table = lua.create_table()?;

    // music.state(channel_id)
    let env_state = env.clone();
    table.raw_set(
        "state",
        async_function(lua, move |lua, ch_val: Value| {
            let env = env_state.clone();
            async move {
                env.require(PluginPermission::ReadMusic)?;
                let channel_id = parse_id(ch_val, "channel_id")?;

                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                let resp = env
                    .on_server(async move {
                        let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                        music::fetch_session(&ctx, &proto::FetchMusicSession { channel_id }).await
                    })
                    .await?
                    .map_err(PluginEnv::api_error)?;

                match resp {
                    Response::MusicSession(s) => Ok(Value::Table(session_table(&lua, &s)?)),
                    _ => Err(raise("INTERNAL: unexpected response")),
                }
            }
        })?,
    )?;

    // music.add(channel_id, query)
    let env_add = env.clone();
    table.raw_set(
        "add",
        async_function(lua, move |_lua, (ch_val, query_val): (Value, Value)| {
            let env = env_add.clone();
            async move {
                env.require(PluginPermission::ControlMusic)?;
                let channel_id = parse_id(ch_val, "channel_id")?;
                let query = match query_val {
                    Value::String(s) => s.to_str()?.to_owned(),
                    _ => return Err(raise("INVALID_ARGUMENT: query must be a string")),
                };

                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                let query_clone = query.clone();
                env.on_server(async move {
                    let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                    let search_res = music::search(
                        &ctx,
                        proto::MusicSearch {
                            channel_id,
                            query: query_clone,
                            source: String::new(),
                        },
                    )
                    .await?;

                    let results = match search_res {
                        Response::MusicSearchResults(r) => r,
                        _ => return Err(ApiError::internal()),
                    };

                    if results.results.is_empty() {
                        return Err(ApiError::not_found("track"));
                    }

                    let result_ids = if results.playlist.is_some() {
                        results.results.into_iter().map(|r| r.result_id).collect()
                    } else {
                        vec![results.results[0].result_id.clone()]
                    };

                    music::enqueue(
                        &ctx,
                        proto::MusicEnqueue {
                            channel_id,
                            result_ids,
                            placement: proto::MusicPlacement::End as i32,
                            before_item_id: None,
                            history_item_ids: Vec::new(),
                        },
                    )
                    .await
                })
                .await?
                .map_err(PluginEnv::api_error)?;

                env.log.push(
                    proto::PluginLogLevel::Info,
                    &format!("added music '{query}' to channel {channel_id}"),
                );

                Ok(())
            }
        })?,
    )?;

    // music.skip(channel_id)
    let env_skip = env.clone();
    table.raw_set(
        "skip",
        async_function(lua, move |_lua, ch_val: Value| {
            let env = env_skip.clone();
            async move {
                env.require(PluginPermission::ControlMusic)?;
                let channel_id = parse_id(ch_val, "channel_id")?;

                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                env.on_server(async move {
                    let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                    music::playback(
                        &ctx,
                        proto::MusicPlayback {
                            channel_id,
                            action: proto::MusicAction::Next as i32,
                            position_ms: None,
                            expected_item_id: None,
                        },
                    )
                    .await
                })
                .await?
                .map_err(PluginEnv::api_error)?;

                env.log.push(
                    proto::PluginLogLevel::Info,
                    &format!("skipped music in channel {channel_id}"),
                );

                Ok(())
            }
        })?,
    )?;

    // music.pause(channel_id)
    let env_pause = env.clone();
    table.raw_set(
        "pause",
        async_function(lua, move |_lua, ch_val: Value| {
            let env = env_pause.clone();
            async move {
                env.require(PluginPermission::ControlMusic)?;
                let channel_id = parse_id(ch_val, "channel_id")?;

                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                env.on_server(async move {
                    let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                    music::playback(
                        &ctx,
                        proto::MusicPlayback {
                            channel_id,
                            action: proto::MusicAction::Pause as i32,
                            position_ms: None,
                            expected_item_id: None,
                        },
                    )
                    .await
                })
                .await?
                .map_err(PluginEnv::api_error)?;

                env.log.push(
                    proto::PluginLogLevel::Info,
                    &format!("paused music in channel {channel_id}"),
                );

                Ok(())
            }
        })?,
    )?;

    // music.resume(channel_id)
    let env_resume = env.clone();
    table.raw_set(
        "resume",
        async_function(lua, move |_lua, ch_val: Value| {
            let env = env_resume.clone();
            async move {
                env.require(PluginPermission::ControlMusic)?;
                let channel_id = parse_id(ch_val, "channel_id")?;

                let state = env.state()?;
                let plugin_id = env.plugin_id.clone();
                let user_id = env.user_id;

                env.on_server(async move {
                    let ctx = Ctx::for_plugin(&state, user_id, &plugin_id);
                    music::playback(
                        &ctx,
                        proto::MusicPlayback {
                            channel_id,
                            action: proto::MusicAction::Resume as i32,
                            position_ms: None,
                            expected_item_id: None,
                        },
                    )
                    .await
                })
                .await?
                .map_err(PluginEnv::api_error)?;

                env.log.push(
                    proto::PluginLogLevel::Info,
                    &format!("resumed music in channel {channel_id}"),
                );

                Ok(())
            }
        })?,
    )?;

    lua.globals().raw_set("music", table)?;
    Ok(())
}
