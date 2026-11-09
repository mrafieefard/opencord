// SPDX-License-Identifier: MPL-2.0

//! Payload tables for routed gateway events reaching plugins (design 2026-11-07 §10.4).

use opencord_proto::v1 as proto;
use proto::event::Kind;
use serde_json::{Map, Value, json};

use super::events;
use crate::guild::Guild;

/// Builds the JSON payload for a routed event. All IDs are strings, enums are
/// lowercase without prefixes, nested messages are tables.
pub fn payload(kind: &Kind, guild: &Guild) -> Value {
    match kind {
        Kind::MessageCreate(event) => json!({
            "message": event.message.as_ref().map(|message| events::message_table(message, guild)),
        }),
        Kind::MessageUpdate(event) => json!({
            "message": event.message.as_ref().map(|message| events::message_table(message, guild)),
        }),
        Kind::MessageDelete(event) => json!({
            "channel_id": events::id(event.channel_id),
            "message_id": events::id(event.message_id),
        }),
        Kind::TypingStart(event) => json!({
            "channel_id": events::id(event.channel_id),
            "user_id": events::id(event.user_id),
            "user": events::user_table(guild, event.user_id),
        }),

        Kind::ChannelCreate(event) => json!({
            "channel": event.channel.as_ref().map(channel_table),
        }),
        Kind::ChannelUpdate(event) => json!({
            "channel": event.channel.as_ref().map(channel_table),
        }),
        Kind::ChannelDelete(event) => json!({
            "channel_id": events::id(event.channel_id),
        }),

        Kind::RoleCreate(event) => json!({
            "role": event.role.as_ref().map(role_table),
        }),
        Kind::RoleUpdate(event) => json!({
            "role": event.role.as_ref().map(role_table),
        }),
        Kind::RoleDelete(event) => json!({
            "role_id": events::id(event.role_id),
        }),

        Kind::MemberJoin(event) => json!({
            "member": event.member.as_ref().map(events::member_table),
        }),
        Kind::MemberUpdate(event) => json!({
            "member": event.member.as_ref().map(member_update_table),
        }),
        Kind::PresenceUpdate(event) => json!({
            "presence": event.presence.as_ref().map(|p| {
                json!({
                    "user_id": events::id(p.user_id),
                    "user": events::user_table(guild, p.user_id),
                    "status": events::enum_name(p.status().as_str_name(), "PRESENCE_STATUS_"),
                })
            }),
        }),

        Kind::ServerUpdate(event) => json!({
            "server": event.server.as_ref().map(server_info_table),
        }),
        Kind::VoiceSettingsUpdate(event) => json!({
            "settings": event.settings.as_ref().map(voice_settings_table),
        }),
        Kind::UploadSettingsUpdate(event) => json!({
            "settings": event.settings.as_ref().map(upload_settings_table),
        }),

        Kind::VoiceStateUpdate(event) => json!({
            "voice_state": event
                .voice_state
                .as_ref()
                .map(|state| events::voice_state_table(state, guild)),
        }),
        Kind::StreamCreate(event) => json!({
            "stream": event.stream.as_ref().map(|s| stream_table(s, guild)),
        }),
        Kind::StreamUpdate(event) => json!({
            "stream": event.stream.as_ref().map(|s| stream_table(s, guild)),
        }),
        Kind::StreamDelete(event) => json!({
            "stream_key": event.stream_key,
            "channel_id": events::id(event.channel_id),
        }),
        Kind::StreamViewersUpdate(event) => json!({
            "stream_key": event.stream_key,
            "viewer_ids": event.viewer_ids.iter().map(|&v| events::id(v)).collect::<Vec<_>>(),
        }),

        Kind::VoiceChannelEffect(event) => {
            let mut table = Map::new();
            table.insert("channel_id".to_owned(), events::id(event.channel_id));
            table.insert("user_id".to_owned(), events::id(event.user_id));
            table.insert("user".to_owned(), events::user_table(guild, event.user_id));
            table.insert(
                "sound_ref".to_owned(),
                Value::String(event.sound_ref.clone()),
            );
            table.insert("name".to_owned(), Value::String(event.name.clone()));
            if let Some(emoji) = &event.emoji {
                table.insert("emoji".to_owned(), Value::String(emoji.clone()));
            }
            table.insert("volume".to_owned(), json!(event.volume));
            table.insert("started_at_ms".to_owned(), json!(event.started_at_ms));
            table.insert("duration_ms".to_owned(), json!(event.duration_ms));
            Value::Object(table)
        }
        Kind::SoundboardSoundCreate(event) => json!({
            "sound": event.sound.as_ref().map(soundboard_sound_table),
        }),
        Kind::SoundboardSoundUpdate(event) => json!({
            "sound": event.sound.as_ref().map(soundboard_sound_table),
        }),
        Kind::SoundboardSoundDelete(event) => json!({
            "sound_id": events::id(event.sound_id),
        }),

        Kind::MusicNowPlaying(event) => json!({
            "channel_id": events::id(event.channel_id),
            "title": event.title,
            "artist": event.artist,
            "artwork_id": event.artwork_id,
            "playing": event.playing,
            "ended": event.ended,
            "source_display_name": event.source_display_name,
        }),
        Kind::MusicUpdate(event) => music_update_table(event, guild),
        Kind::MusicNotice(event) => json!({
            "channel_id": events::id(event.channel_id),
            "text": event.text,
            "at_ms": event.at_ms,
        }),

        _ => Value::Null,
    }
}

pub fn channel_table(c: &proto::Channel) -> Value {
    let mut table = Map::new();
    table.insert("id".to_owned(), events::id(c.id));
    table.insert(
        "kind".to_owned(),
        events::enum_name(c.kind().as_str_name(), "CHANNEL_KIND_"),
    );
    table.insert("name".to_owned(), Value::String(c.name.clone()));
    if let Some(topic) = &c.topic {
        table.insert("topic".to_owned(), Value::String(topic.clone()));
    }
    if let Some(parent_id) = c.parent_id {
        table.insert("parent_id".to_owned(), events::id(parent_id));
    }
    table.insert("position".to_owned(), json!(c.position));
    let overwrites: Vec<Value> = c
        .overwrites
        .iter()
        .map(|o| {
            let allow_perms =
                opencord_common::permissions::Permissions::from_bits_truncate(o.allow);
            let deny_perms =
                opencord_common::permissions::Permissions::from_bits_truncate(o.deny);
            json!({
                "target_kind": events::enum_name(o.target_kind().as_str_name(), "OVERWRITE_TARGET_"),
                "target_id": events::id(o.target_id),
                "allow": events::permissions_to_names(allow_perms),
                "deny": events::permissions_to_names(deny_perms),
            })
        })
        .collect();
    table.insert("overwrites".to_owned(), Value::Array(overwrites));
    table.insert("bitrate".to_owned(), json!(c.bitrate));
    table.insert("user_limit".to_owned(), json!(c.user_limit));
    table.insert("text_in_voice".to_owned(), Value::Bool(c.text_in_voice));
    Value::Object(table)
}

pub fn role_table(r: &proto::Role) -> Value {
    let mut table = Map::new();
    table.insert("id".to_owned(), events::id(r.id));
    table.insert("name".to_owned(), Value::String(r.name.clone()));
    table.insert("color".to_owned(), json!(r.color));
    table.insert("position".to_owned(), json!(r.position));
    let perms = opencord_common::permissions::Permissions::from_bits_truncate(r.permissions);
    let perm_names = events::permissions_to_names(perms);
    table.insert("permissions".to_owned(), json!(perm_names));
    table.insert("hoist".to_owned(), Value::Bool(r.hoist));
    table.insert("mentionable".to_owned(), Value::Bool(r.mentionable));
    if let Some(max_upload) = r.max_upload_bytes {
        table.insert("max_upload_bytes".to_owned(), json!(max_upload));
    }
    if let Some(plugin_id) = &r.plugin_id {
        table.insert("plugin_id".to_owned(), Value::String(plugin_id.clone()));
    }
    Value::Object(table)
}

pub fn member_update_table(m: &proto::Member) -> Value {
    let user = m.user.as_ref();
    let name = m
        .nickname
        .clone()
        .or_else(|| user.map(|u| u.display_name.clone()))
        .unwrap_or_default();
    let user_id = user.map_or(0, |u| u.id);
    let role_ids: Vec<Value> = m.role_ids.iter().map(|&r| events::id(r)).collect();
    let mut table = Map::new();
    table.insert("id".to_owned(), events::id(user_id));
    table.insert("name".to_owned(), Value::String(name));
    if let Some(u) = user {
        table.insert(
            "user".to_owned(),
            json!({
                "id": events::id(u.id),
                "display_name": u.display_name,
                "avatar_hash": u.avatar_hash,
            }),
        );
    }
    if let Some(nick) = &m.nickname {
        table.insert("nickname".to_owned(), Value::String(nick.clone()));
    }
    table.insert("roles".to_owned(), Value::Array(role_ids.clone()));
    table.insert("role_ids".to_owned(), Value::Array(role_ids));
    table.insert("joined_at_ms".to_owned(), json!(m.joined_at_ms));
    Value::Object(table)
}

pub fn server_info_table(s: &proto::ServerInfo) -> Value {
    let mut table = Map::new();
    let id_str = hex::encode(&s.server_id);
    table.insert("id".to_owned(), Value::String(id_str.clone()));
    table.insert("server_id".to_owned(), Value::String(id_str));
    table.insert("name".to_owned(), Value::String(s.name.clone()));
    table.insert(
        "description".to_owned(),
        Value::String(s.description.clone()),
    );
    if let Some(owner_id) = s.owner_id {
        table.insert("owner_id".to_owned(), events::id(owner_id));
    }
    table.insert("open_join".to_owned(), Value::Bool(s.open_join));
    table.insert(
        "everyone_role_id".to_owned(),
        events::id(s.everyone_role_id),
    );
    table.insert("icon_hash".to_owned(), Value::String(s.icon_hash.clone()));
    Value::Object(table)
}

pub fn voice_settings_table(s: &proto::VoiceSettings) -> Value {
    let mut table = Map::new();
    table.insert(
        "screen_share_max_resolution".to_owned(),
        events::enum_name(
            s.screen_share_max_resolution().as_str_name(),
            "SCREEN_SHARE_RESOLUTION_",
        ),
    );
    table.insert(
        "screen_share_max_fps".to_owned(),
        json!(s.screen_share_max_fps),
    );
    table.insert("max_stream_viewers".to_owned(), json!(s.max_stream_viewers));
    table.insert("camera_allowed".to_owned(), Value::Bool(s.camera_allowed));
    table.insert(
        "max_camera_participants".to_owned(),
        json!(s.max_camera_participants),
    );
    table.insert("max_voice_bitrate".to_owned(), json!(s.max_voice_bitrate));
    if let Some(afk_id) = s.afk_channel_id {
        table.insert("afk_channel_id".to_owned(), events::id(afk_id));
    }
    table.insert("afk_timeout_s".to_owned(), json!(s.afk_timeout_s));
    table.insert(
        "soundboard_enabled".to_owned(),
        Value::Bool(s.soundboard_enabled),
    );
    table.insert(
        "allow_default_sounds".to_owned(),
        Value::Bool(s.allow_default_sounds),
    );
    table.insert(
        "allow_external_sounds".to_owned(),
        Value::Bool(s.allow_external_sounds),
    );
    table.insert("sound_cooldown_s".to_owned(), json!(s.sound_cooldown_s));
    table.insert("max_sounds".to_owned(), json!(s.max_sounds));
    Value::Object(table)
}

pub fn upload_settings_table(s: &proto::UploadSettings) -> Value {
    json!({
        "blocked_extensions": s.blocked_extensions,
        "voice_message_max_seconds": s.voice_message_max_seconds,
        "max_file_bytes": s.max_file_bytes,
        "max_attachments": s.max_attachments,
        "piece_bytes": s.piece_bytes,
    })
}

pub fn stream_table(s: &proto::Stream, guild: &Guild) -> Value {
    json!({
        "stream_key": s.stream_key,
        "channel_id": events::id(s.channel_id),
        "user_id": events::id(s.user_id),
        "user": events::user_table(guild, s.user_id),
        "source_kind": events::enum_name(s.source_kind().as_str_name(), "STREAM_SOURCE_KIND_"),
        "resolution": events::enum_name(s.resolution().as_str_name(), "SCREEN_SHARE_RESOLUTION_"),
        "fps": s.fps,
        "has_audio": s.has_audio,
        "viewer_count": s.viewer_count,
    })
}

pub fn soundboard_sound_table(s: &proto::SoundboardSound) -> Value {
    let mut table = Map::new();
    table.insert("id".to_owned(), events::id(s.id));
    table.insert("name".to_owned(), Value::String(s.name.clone()));
    if let Some(emoji) = &s.emoji {
        table.insert("emoji".to_owned(), Value::String(emoji.clone()));
    }
    table.insert("volume".to_owned(), json!(s.volume));
    table.insert("size_bytes".to_owned(), json!(s.size_bytes));
    table.insert("duration_ms".to_owned(), json!(s.duration_ms));
    if let Some(uploader_id) = s.uploader_id {
        table.insert("uploader_id".to_owned(), events::id(uploader_id));
    }
    table.insert("created_at_ms".to_owned(), json!(s.created_at_ms));
    Value::Object(table)
}

pub fn music_update_table(event: &proto::MusicUpdate, guild: &Guild) -> Value {
    let mut table = Map::new();
    table.insert("channel_id".to_owned(), events::id(event.channel_id));
    table.insert("base_version".to_owned(), json!(event.base_version));
    table.insert("version".to_owned(), json!(event.version));
    table.insert(
        "state".to_owned(),
        events::enum_name(event.state().as_str_name(), "MUSIC_PLAY_STATE_"),
    );
    table.insert("position_ms".to_owned(), json!(event.position_ms));
    table.insert("server_time_ms".to_owned(), json!(event.server_time_ms));
    table.insert(
        "repeat".to_owned(),
        events::enum_name(event.repeat().as_str_name(), "MUSIC_REPEAT_"),
    );
    table.insert("shuffle".to_owned(), Value::Bool(event.shuffle));
    table.insert(
        "waiting_for_node".to_owned(),
        Value::Bool(event.waiting_for_node),
    );
    table.insert("paused_empty".to_owned(), Value::Bool(event.paused_empty));
    let skip_votes: Vec<Value> = event.skip_votes.iter().map(|&v| events::id(v)).collect();
    table.insert("skip_votes".to_owned(), Value::Array(skip_votes));
    table.insert(
        "skip_votes_needed".to_owned(),
        json!(event.skip_votes_needed),
    );
    if let Some(np) = &event.now_playing {
        table.insert(
            "now_playing".to_owned(),
            json!({
                "item": np.item.as_ref().map(|item| music_queue_item_table(item, guild)),
            }),
        );
    }
    if let Some(queue) = &event.queue {
        table.insert("queue".to_owned(), music_list_change_table(queue, guild));
    }
    if let Some(history) = &event.history {
        table.insert(
            "history".to_owned(),
            music_list_change_table(history, guild),
        );
    }
    Value::Object(table)
}

fn music_queue_item_table(item: &proto::MusicQueueItem, guild: &Guild) -> Value {
    json!({
        "item_id": item.item_id,
        "track": item.track.as_ref().map(music_track_table),
        "requester_id": events::id(item.requester_id),
        "requester": events::user_table(guild, item.requester_id),
        "added_at_ms": item.added_at_ms,
    })
}

fn music_track_table(t: &proto::MusicTrack) -> Value {
    json!({
        "title": t.title,
        "artist": t.artist,
        "duration_ms": t.duration_ms,
        "is_stream": t.is_stream,
        "is_seekable": t.is_seekable,
        "source": t.source,
        "source_display_name": t.source_display_name,
        "uri": t.uri,
        "artwork_id": t.artwork_id,
        "local_song_id": events::id(t.local_song_id),
    })
}

fn music_list_change_table(c: &proto::MusicListChange, guild: &Guild) -> Value {
    let placed: Vec<Value> = c
        .placed
        .iter()
        .map(|p| {
            let mut obj = Map::new();
            obj.insert("item_id".to_owned(), Value::String(p.item_id.clone()));
            if let Some(item) = &p.item {
                obj.insert("item".to_owned(), music_queue_item_table(item, guild));
            }
            if let Some(before) = &p.before_item_id {
                obj.insert("before_item_id".to_owned(), Value::String(before.clone()));
            }
            Value::Object(obj)
        })
        .collect();
    json!({
        "removed": c.removed,
        "placed": placed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap;
    use crate::db::test_pool;
    use crate::guild::{Guild, Member, User};
    use opencord_common::snowflake::SnowflakeGenerator;

    async fn test_guild() -> Guild {
        let (pool, _dir) = test_pool().await;
        let ids = SnowflakeGenerator::new(0).unwrap();
        let bootstrap = bootstrap::initialize(&pool, "Test", &ids, 0).await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let mut guild = Guild::load(&mut conn, bootstrap.meta).await.unwrap();
        guild.members.insert(
            2,
            Member {
                user: User {
                    id: 2,
                    public_key: vec![2; 32],
                    display_name: "Alice".to_owned(),
                    avatar_hash: None,
                },
                nickname: Some("Al".to_owned()),
                role_ids: vec![10],
                joined_at: 100,
            },
        );
        guild
    }

    #[tokio::test]
    async fn channel_payloads() {
        let guild = test_guild().await;
        let create = Kind::ChannelCreate(proto::ChannelCreate {
            channel: Some(proto::Channel {
                id: 123456,
                kind: proto::ChannelKind::Text as i32,
                name: "chat".to_owned(),
                topic: Some("talk here".to_owned()),
                parent_id: Some(999),
                position: 1,
                overwrites: vec![proto::PermissionOverwrite {
                    target_kind: proto::OverwriteTarget::Role as i32,
                    target_id: 10,
                    allow: 1024,
                    deny: 2048,
                }],
                bitrate: 0,
                user_limit: 0,
                text_in_voice: false,
            }),
        });
        let res = payload(&create, &guild);
        assert_eq!(res["channel"]["id"], "123456");
        assert_eq!(res["channel"]["kind"], "text");
        assert_eq!(res["channel"]["parent_id"], "999");
        assert_eq!(res["channel"]["overwrites"][0]["target_kind"], "role");
        assert_eq!(res["channel"]["overwrites"][0]["target_id"], "10");

        let delete = Kind::ChannelDelete(proto::ChannelDelete { channel_id: 123456 });
        assert_eq!(payload(&delete, &guild)["channel_id"], "123456");
    }

    #[tokio::test]
    async fn role_payloads() {
        let guild = test_guild().await;
        let create = Kind::RoleCreate(proto::RoleCreate {
            role: Some(proto::Role {
                id: 789,
                name: "Admin".to_owned(),
                color: 0xff0000,
                position: 2,
                permissions: 8,
                hoist: true,
                mentionable: true,
                max_upload_bytes: Some(10_000_000),
                plugin_id: Some("my-plugin".to_owned()),
            }),
        });
        let res = payload(&create, &guild);
        assert_eq!(res["role"]["id"], "789");
        assert_eq!(res["role"]["name"], "Admin");
        assert_eq!(res["role"]["plugin_id"], "my-plugin");

        let delete = Kind::RoleDelete(proto::RoleDelete { role_id: 789 });
        assert_eq!(payload(&delete, &guild)["role_id"], "789");
    }

    #[tokio::test]
    async fn member_and_presence_payloads() {
        let guild = test_guild().await;
        let update = Kind::MemberUpdate(proto::MemberUpdate {
            member: Some(proto::Member {
                user: Some(proto::User {
                    id: 2,
                    display_name: "Alice".to_owned(),
                    avatar_hash: "hash123".to_owned(),
                    public_key: vec![],
                }),
                nickname: Some("Al".to_owned()),
                role_ids: vec![10, 20],
                joined_at_ms: 500,
            }),
        });
        let res = payload(&update, &guild);
        assert_eq!(res["member"]["id"], "2");
        assert_eq!(res["member"]["name"], "Al");
        assert_eq!(res["member"]["role_ids"], serde_json::json!(["10", "20"]));

        let presence = Kind::PresenceUpdate(proto::PresenceUpdate {
            presence: Some(proto::Presence {
                user_id: 2,
                status: proto::PresenceStatus::Dnd as i32,
            }),
        });
        let pres_res = payload(&presence, &guild);
        assert_eq!(pres_res["presence"]["user_id"], "2");
        assert_eq!(pres_res["presence"]["user"]["name"], "Al");
        assert_eq!(pres_res["presence"]["status"], "dnd");
    }

    #[tokio::test]
    async fn server_and_settings_payloads() {
        let guild = test_guild().await;
        let s_update = Kind::ServerUpdate(proto::ServerUpdate {
            server: Some(proto::ServerInfo {
                server_id: vec![1; 16],
                name: "My Server".to_owned(),
                description: "Cool".to_owned(),
                owner_id: Some(2),
                open_join: true,
                everyone_role_id: 0,
                icon_hash: "icon".to_owned(),
            }),
        });
        let res = payload(&s_update, &guild);
        assert_eq!(res["server"]["owner_id"], "2");
        assert_eq!(res["server"]["everyone_role_id"], "0");
        assert_eq!(res["server"]["name"], "My Server");

        let v_update = Kind::VoiceSettingsUpdate(proto::VoiceSettingsUpdate {
            settings: Some(proto::VoiceSettings {
                screen_share_max_resolution:
                    proto::ScreenShareResolution::ScreenShareResolution1080p as i32,
                afk_channel_id: Some(555),
                sound_cooldown_s: 3,
                ..Default::default()
            }),
        });
        let v_res = payload(&v_update, &guild);
        assert_eq!(v_res["settings"]["afk_channel_id"], "555");
        assert_eq!(v_res["settings"]["screen_share_max_resolution"], "1080p");
        assert_eq!(v_res["settings"]["sound_cooldown_s"], 3);

        let u_update = Kind::UploadSettingsUpdate(proto::UploadSettingsUpdate {
            settings: Some(proto::UploadSettings {
                blocked_extensions: vec!["bat".to_owned()],
                max_attachments: 10,
                ..Default::default()
            }),
        });
        let u_res = payload(&u_update, &guild);
        assert_eq!(
            u_res["settings"]["blocked_extensions"],
            serde_json::json!(["bat"])
        );
    }

    #[tokio::test]
    async fn soundboard_and_streams_and_music_payloads() {
        let guild = test_guild().await;
        let effect = Kind::VoiceChannelEffect(proto::VoiceChannelEffect {
            channel_id: 111,
            user_id: 2,
            sound_ref: "horn".to_owned(),
            name: "Airhorn".to_owned(),
            emoji: Some("📯".to_owned()),
            volume: 80,
            started_at_ms: 12345,
            duration_ms: 1500,
        });
        let eff_res = payload(&effect, &guild);
        assert_eq!(eff_res["channel_id"], "111");
        assert_eq!(eff_res["user_id"], "2");
        assert_eq!(eff_res["user"]["name"], "Al");
        assert_eq!(eff_res["sound_ref"], "horn");

        let s_create = Kind::SoundboardSoundCreate(proto::SoundboardSoundCreate {
            sound: Some(proto::SoundboardSound {
                id: 999,
                name: "Quack".to_owned(),
                emoji: None,
                volume: 50,
                size_bytes: 4096,
                duration_ms: 1000,
                uploader_id: Some(2),
                created_at_ms: 10000,
                sha256: vec![],
            }),
        });
        let sc_res = payload(&s_create, &guild);
        assert_eq!(sc_res["sound"]["id"], "999");
        assert_eq!(sc_res["sound"]["uploader_id"], "2");

        let stream_del = Kind::StreamDelete(proto::StreamDelete {
            stream_key: "stream:111:2".to_owned(),
            channel_id: 111,
        });
        let sd_res = payload(&stream_del, &guild);
        assert_eq!(sd_res["channel_id"], "111");
        assert_eq!(sd_res["stream_key"], "stream:111:2");

        let viewers = Kind::StreamViewersUpdate(proto::StreamViewersUpdate {
            stream_key: "stream:111:2".to_owned(),
            viewer_ids: vec![2, 3],
        });
        let vw_res = payload(&viewers, &guild);
        assert_eq!(vw_res["viewer_ids"], serde_json::json!(["2", "3"]));

        let np = Kind::MusicNowPlaying(proto::MusicNowPlaying {
            channel_id: 111,
            title: "Song".to_owned(),
            artist: "Band".to_owned(),
            artwork_id: "art".to_owned(),
            playing: true,
            ended: false,
            source_display_name: "YouTube".to_owned(),
        });
        let np_res = payload(&np, &guild);
        assert_eq!(np_res["channel_id"], "111");
        assert_eq!(np_res["title"], "Song");

        let notice = Kind::MusicNotice(proto::MusicNotice {
            channel_id: 111,
            text: "Skipped".to_owned(),
            at_ms: 9999,
        });
        let not_res = payload(&notice, &guild);
        assert_eq!(not_res["channel_id"], "111");
        assert_eq!(not_res["text"], "Skipped");

        let m_update = Kind::MusicUpdate(Box::new(proto::MusicUpdate {
            channel_id: 111,
            base_version: 1,
            version: 2,
            state: proto::MusicPlayState::Playing as i32,
            position_ms: 5000,
            server_time_ms: 123456,
            repeat: proto::MusicRepeat::Queue as i32,
            shuffle: false,
            waiting_for_node: false,
            paused_empty: false,
            skip_votes: vec![2],
            skip_votes_needed: 2,
            now_playing: None,
            queue: None,
            history: None,
        }));
        let mu_res = payload(&m_update, &guild);
        assert_eq!(mu_res["channel_id"], "111");
        assert_eq!(mu_res["state"], "playing");
        assert_eq!(mu_res["repeat"], "queue");
        assert_eq!(mu_res["skip_votes"], serde_json::json!(["2"]));
    }
}
