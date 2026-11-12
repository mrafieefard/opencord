//! Event routing's tests (plan Task 9).

use std::collections::BTreeSet;

use opencord_common::snowflake::SnowflakeGenerator;
use opencord_proto::v1 as proto;
use proto::event::Kind;

use super::{Route, enum_name, payload, route, stream_channel};
use crate::bootstrap;
use crate::db::test_pool;
use crate::guild::{Guild, Member, PluginUser, User};
use crate::plugins::permissions::PluginPermission::{self, *};

/// How many kinds `proto::event::Kind` has.
const KINDS: usize = 60;

/// A distinct number per kind. There's no catch-all arm, so a new kind
/// stops this building until it's given a number and a row in `table`.
fn number(kind: &Kind) -> usize {
    match kind {
        Kind::MessageCreate(_) => 0,
        Kind::MessageUpdate(_) => 1,
        Kind::MessageDelete(_) => 2,
        Kind::ChannelCreate(_) => 3,
        Kind::ChannelUpdate(_) => 4,
        Kind::ChannelDelete(_) => 5,
        Kind::RoleCreate(_) => 6,
        Kind::RoleUpdate(_) => 7,
        Kind::RoleDelete(_) => 8,
        Kind::MemberJoin(_) => 9,
        Kind::MemberLeave(_) => 10,
        Kind::MemberUpdate(_) => 11,
        Kind::PresenceUpdate(_) => 12,
        Kind::TypingStart(_) => 13,
        Kind::ServerUpdate(_) => 14,
        Kind::VoiceSettingsUpdate(_) => 15,
        Kind::VoiceStateUpdate(_) => 16,
        Kind::VoiceServerUpdate(_) => 17,
        Kind::StreamCreate(_) => 18,
        Kind::StreamUpdate(_) => 19,
        Kind::StreamDelete(_) => 20,
        Kind::StreamViewersUpdate(_) => 21,
        Kind::VoiceChannelEffect(_) => 22,
        Kind::SoundboardSoundCreate(_) => 23,
        Kind::SoundboardSoundUpdate(_) => 24,
        Kind::SoundboardSoundDelete(_) => 25,
        Kind::NetworkStatus(_) => 26,
        Kind::ServerNotice(_) => 27,
        Kind::ServerAddressesUpdate(_) => 28,
        Kind::LavalinkNodeStatus(_) => 29,
        Kind::LavalinkNodeEvent(_) => 30,
        Kind::LavalinkTestProgress(_) => 31,
        Kind::MusicSnapshot(_) => 32,
        Kind::MusicUpdate(_) => 33,
        Kind::MusicNotice(_) => 34,
        Kind::MusicNowPlaying(_) => 35,
        Kind::MusicSettingsUpdate(_) => 36,
        Kind::CommandsUpdate(_) => 37,
        Kind::EphemeralMessage(_) => 38,
        Kind::UploadSettingsUpdate(_) => 39,
        Kind::UploadReady(_) => 40,
        Kind::UploadFailed(_) => 41,
        Kind::StorageUpdate(_) => 42,
        Kind::PendingEvent(_) => 43,
        Kind::PrivateTypingStart(_) => 44,
        Kind::PrivateSettingsUpdate(_) => 45,
        Kind::PluginUpdate(_) => 46,
        Kind::PluginRemove(_) => 47,
        Kind::PluginReviewUpdate(_) => 48,
        Kind::PluginReviewRemove(_) => 49,
        Kind::PluginLogAppend(_) => 50,
        Kind::PluginUsersUpdate(_) => 51,
        Kind::ViewMessage(_) => 52,
        Kind::ViewClosed(_) => 53,
        Kind::ViewScriptsUpdate(_) => 54,
        Kind::PluginChannelTypesUpdate(_) => 55,
        Kind::PluginActivitiesUpdate(_) => 56,
        Kind::ActivitySessionCreate(_) => 57,
        Kind::ActivitySessionUpdate(_) => 58,
        Kind::ActivitySessionDelete(_) => 59,
    }
}

type Expected = Option<(&'static str, PluginPermission)>;

/// Every kind, and where it must go: §10.4's event and permission, or
/// nowhere (§8.4).
fn table() -> Vec<(Kind, Expected)> {
    let stream_key = "stream:5:6".to_owned();
    vec![
        (
            Kind::MessageCreate(Default::default()),
            Some(("message_create", ReadMessages)),
        ),
        (
            Kind::MessageUpdate(Default::default()),
            Some(("message_update", ReadMessages)),
        ),
        (
            Kind::MessageDelete(Default::default()),
            Some(("message_delete", ReadMessages)),
        ),
        (
            Kind::TypingStart(Default::default()),
            Some(("typing_start", ReadMessages)),
        ),
        (
            Kind::ChannelCreate(Default::default()),
            Some(("channel_create", ReadChannels)),
        ),
        (
            Kind::ChannelUpdate(Default::default()),
            Some(("channel_update", ReadChannels)),
        ),
        (
            Kind::ChannelDelete(Default::default()),
            Some(("channel_delete", ReadChannels)),
        ),
        (
            Kind::RoleCreate(Default::default()),
            Some(("role_create", ReadRoles)),
        ),
        (
            Kind::RoleUpdate(Default::default()),
            Some(("role_update", ReadRoles)),
        ),
        (
            Kind::RoleDelete(Default::default()),
            Some(("role_delete", ReadRoles)),
        ),
        (Kind::MemberJoin(Default::default()), None),
        (Kind::MemberLeave(Default::default()), None),
        (
            Kind::MemberUpdate(Default::default()),
            Some(("member_update", ReadMembers)),
        ),
        (
            Kind::PresenceUpdate(Default::default()),
            Some(("presence_update", ReadMembers)),
        ),
        (
            Kind::ServerUpdate(Default::default()),
            Some(("server_update", ReadServer)),
        ),
        (
            Kind::VoiceSettingsUpdate(Default::default()),
            Some(("voice_settings_update", ReadServer)),
        ),
        (
            Kind::UploadSettingsUpdate(Default::default()),
            Some(("upload_settings_update", ReadServer)),
        ),
        (
            Kind::VoiceStateUpdate(Default::default()),
            Some(("voice_state_update", ReadVoice)),
        ),
        (
            Kind::StreamCreate(Default::default()),
            Some(("stream_create", ReadVoice)),
        ),
        (
            Kind::StreamUpdate(Default::default()),
            Some(("stream_update", ReadVoice)),
        ),
        (
            Kind::StreamDelete(Default::default()),
            Some(("stream_delete", ReadVoice)),
        ),
        (
            Kind::StreamViewersUpdate(proto::StreamViewersUpdate {
                stream_key,
                viewer_ids: Vec::new(),
            }),
            Some(("stream_viewers_update", ReadVoice)),
        ),
        (
            Kind::VoiceChannelEffect(Default::default()),
            Some(("soundboard_play", ReadVoice)),
        ),
        (
            Kind::SoundboardSoundCreate(Default::default()),
            Some(("soundboard_sound_create", ReadVoice)),
        ),
        (
            Kind::SoundboardSoundUpdate(Default::default()),
            Some(("soundboard_sound_update", ReadVoice)),
        ),
        (
            Kind::SoundboardSoundDelete(Default::default()),
            Some(("soundboard_sound_delete", ReadVoice)),
        ),
        (
            Kind::MusicNowPlaying(Default::default()),
            Some(("music_now_playing", ReadMusic)),
        ),
        (
            Kind::MusicUpdate(Default::default()),
            Some(("music_queue_update", ReadMusic)),
        ),
        (
            Kind::MusicNotice(Default::default()),
            Some(("music_notice", ReadMusic)),
        ),
        // Off-limits, whatever is granted.
        (Kind::VoiceServerUpdate(Default::default()), None),
        (Kind::NetworkStatus(Default::default()), None),
        (Kind::ServerNotice(Default::default()), None),
        (Kind::ServerAddressesUpdate(Default::default()), None),
        (Kind::LavalinkNodeStatus(Default::default()), None),
        (Kind::LavalinkNodeEvent(Default::default()), None),
        (Kind::LavalinkTestProgress(Default::default()), None),
        (Kind::MusicSnapshot(Default::default()), None),
        (Kind::MusicSettingsUpdate(Default::default()), None),
        (Kind::CommandsUpdate(Default::default()), None),
        (Kind::EphemeralMessage(Default::default()), None),
        (Kind::UploadReady(Default::default()), None),
        (Kind::UploadFailed(Default::default()), None),
        (Kind::StorageUpdate(Default::default()), None),
        (Kind::PendingEvent(Default::default()), None),
        (Kind::PrivateTypingStart(Default::default()), None),
        (Kind::PrivateSettingsUpdate(Default::default()), None),
        (Kind::PluginUpdate(Default::default()), None),
        (Kind::PluginRemove(Default::default()), None),
        (Kind::PluginReviewUpdate(Default::default()), None),
        (Kind::PluginReviewRemove(Default::default()), None),
        (Kind::PluginLogAppend(Default::default()), None),
        (Kind::PluginUsersUpdate(Default::default()), None),
        (Kind::ViewMessage(Default::default()), None),
        (Kind::ViewClosed(Default::default()), None),
        (Kind::ViewScriptsUpdate(Default::default()), None),
        (Kind::PluginChannelTypesUpdate(Default::default()), None),
        (Kind::PluginActivitiesUpdate(Default::default()), None),
        (Kind::ActivitySessionCreate(Default::default()), None),
        (Kind::ActivitySessionUpdate(Default::default()), None),
        (Kind::ActivitySessionDelete(Default::default()), None),
    ]
}

#[test]
fn route_covers_every_event_kind() {
    // Catches: a kind left out of the decision, routed under the wrong
    // permission, or routed though it's off-limits.
    let table = table();
    let numbers: BTreeSet<usize> = table.iter().map(|(kind, _)| number(kind)).collect();
    assert_eq!(numbers.len(), table.len(), "a kind is listed twice");
    assert_eq!(numbers, (0..KINDS).collect(), "a kind is missing");
    for (kind, expected) in &table {
        let routed = route(kind).map(|route| (route.name, route.needs));
        assert_eq!(routed, *expected, "{kind:?}");
    }
}

#[test]
fn routes_name_the_channel_to_check() {
    // Catches: a channel event routed without its channel, which would
    // reach plugins whose role can't view it.
    let channel = |kind: Kind| route(&kind).and_then(|route| route.channel);
    let message = proto::Message {
        channel_id: 5,
        ..Default::default()
    };
    assert_eq!(
        channel(Kind::MessageCreate(proto::MessageCreate {
            message: Some(message.clone()),
        })),
        Some(5)
    );
    assert_eq!(
        channel(Kind::MessageUpdate(proto::MessageUpdate {
            message: Some(message),
        })),
        Some(5)
    );
    assert_eq!(
        channel(Kind::TypingStart(proto::TypingStart {
            channel_id: 5,
            user_id: 6,
        })),
        Some(5)
    );
    assert_eq!(
        channel(Kind::VoiceChannelEffect(proto::VoiceChannelEffect {
            channel_id: 5,
            ..Default::default()
        })),
        Some(5)
    );
    assert_eq!(
        channel(Kind::StreamViewersUpdate(proto::StreamViewersUpdate {
            stream_key: "stream:5:6".to_owned(),
            viewer_ids: Vec::new(),
        })),
        Some(5)
    );
    assert_eq!(
        channel(Kind::MusicNotice(proto::MusicNotice {
            channel_id: 5,
            ..Default::default()
        })),
        Some(5)
    );
}

#[test]
fn a_stream_key_without_a_channel_goes_nowhere() {
    for key in [
        "",
        "stream:",
        "stream:x:6",
        "stream:5",
        "stream:5:6:7",
        "other:5:6",
    ] {
        assert_eq!(stream_channel(key), None, "{key}");
        let kind = Kind::StreamViewersUpdate(proto::StreamViewersUpdate {
            stream_key: key.to_owned(),
            viewer_ids: Vec::new(),
        });
        assert_eq!(route(&kind), None::<Route>, "{key}");
    }
}

/// A guild with one member (id 2, "Alice", nicknamed "Al") and one plugin
/// user (id 3, plugin "bot", named "Bot").
async fn guild() -> Guild {
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
            role_ids: Vec::new(),
            joined_at: 0,
        },
    );
    guild.plugin_users.insert(
        3,
        PluginUser {
            plugin_id: "bot".to_owned(),
            role_id: 9,
            name: "Bot".to_owned(),
            avatar_hash: None,
            installed: true,
        },
    );
    guild
}

#[tokio::test]
async fn a_message_reaches_lua_with_string_ids_and_its_author() {
    let kind = Kind::MessageCreate(proto::MessageCreate {
        message: Some(proto::Message {
            id: 9_007_199_254_740_993,
            channel_id: 5,
            author_id: 3,
            content: "hi".to_owned(),
            created_at_ms: 1_000,
            attachments: vec![proto::Attachment {
                id: 7,
                filename: "a.png".to_owned(),
                size_bytes: 10,
                kind: proto::AttachmentKind::Image as i32,
                ..Default::default()
            }],
            ..Default::default()
        }),
    });

    assert_eq!(
        payload(&kind, &guild().await),
        serde_json::json!({
            "message": {
                "id": "9007199254740993",
                "channel_id": "5",
                "author": { "id": "3", "name": "Bot", "plugin": "bot" },
                "content": "hi",
                "created_at_ms": 1_000,
                "attachments": [{ "id": "7", "filename": "a.png", "kind": "image", "size": 10 }],
            }
        })
    );
}

#[tokio::test]
async fn a_voice_state_names_its_user_and_leaves_the_session_out() {
    let kind = Kind::VoiceStateUpdate(proto::VoiceStateUpdate {
        voice_state: Some(proto::VoiceState {
            user_id: 2,
            channel_id: Some(5),
            session_id: "secret-session".to_owned(),
            self_mute: true,
            ..Default::default()
        }),
    });

    assert_eq!(
        payload(&kind, &guild().await),
        serde_json::json!({
            "voice_state": {
                "user": { "id": "2", "name": "Al" },
                "channel_id": "5",
                "self_mute": true,
                "self_deaf": false,
                "server_mute": false,
                "server_deaf": false,
                "suppress": false,
                "self_video": false,
                "self_stream": false,
                "video": false,
                "streaming": false,
            }
        })
    );
}

#[tokio::test]
async fn a_member_joining_is_a_member_table() {
    let kind = Kind::MemberJoin(proto::MemberJoin {
        member: Some(proto::Member {
            user: Some(proto::User {
                id: 4,
                display_name: "Bea".to_owned(),
                ..Default::default()
            }),
            ..Default::default()
        }),
    });

    assert_eq!(
        payload(&kind, &guild().await),
        serde_json::json!({
            "member": {
                "id": "4",
                "name": "Bea",
                "role_ids": [],
                "roles": [],
                "joined_at_ms": 0,
            }
        })
    );
}

#[test]
fn enum_names_lose_their_prefix() {
    assert_eq!(
        enum_name("PRESENCE_STATUS_ONLINE", "PRESENCE_STATUS_"),
        serde_json::json!("online")
    );
}

#[test]
fn start_activities_has_its_name() {
    // Fails if the bit loses its row in the names table, or the row's name
    // differs from the definitions file's.
    use opencord_common::permissions::Permissions;
    assert_eq!(
        super::permissions_to_names(Permissions::START_ACTIVITIES),
        vec!["start_activities".to_owned()]
    );
    assert_eq!(
        super::member_permission_from_name("start_activities"),
        Some(Permissions::START_ACTIVITIES)
    );
}
