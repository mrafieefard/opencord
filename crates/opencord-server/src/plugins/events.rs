//! Server events reaching plugins (design 2026-11-07 §10.4).
//!
//! Each gateway event either maps onto one plugin event, with the plugin
//! permission it needs and the channel the plugin's role must be able to
//! view, or never reaches a plugin (§8.4). Payloads are JSON with every id
//! a decimal string, so they cross to the VM's thread and fit in Lua.

use std::cell::OnceCell;

use opencord_proto::v1 as proto;
use proto::event::Kind;
use serde_json::{Map, Value, json};

use super::permissions::PluginPermission;
use super::{PluginEvent, PluginHandle, Plugins};
use crate::guild::Guild;
use crate::state::{AppState, Audience};

/// Where a gateway event goes for plugins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Route {
    /// The plugin event's name, as `opencord.on` takes it.
    pub name: &'static str,
    pub needs: PluginPermission,
    /// The channel the plugin's role must be able to view, if any.
    pub channel: Option<i64>,
}

/// Who may hear an event, on top of the permission its route needs.
#[derive(Debug, Clone, Copy)]
pub enum Scope<'a> {
    /// Sent to members with this audience. A plugin hears it where its role
    /// can view the route's channel and the audience's; it is never in a
    /// call, so `Voice` counts as the channel. `User` adds nothing: the one
    /// such event routed, `StreamViewersUpdate`, is checked by its stream's
    /// channel.
    Audience(Audience),
    /// A voice state, to those who can view any of these channels (the one
    /// left and the one joined).
    AnyOf(&'a [i64]),
    /// Worked out for this plugin user alone: channels that appeared or
    /// disappeared for its role.
    User(i64),
}

/// §10.4's table. The match has no catch-all arm: a new kind doesn't build
/// until someone decides whether plugins may see it.
pub fn route(kind: &Kind) -> Option<Route> {
    use PluginPermission::{ReadChannels, ReadMembers, ReadMessages, ReadMusic, ReadRoles};
    use PluginPermission::{ReadServer, ReadVoice};
    let to = |name, needs, channel| {
        Some(Route {
            name,
            needs,
            channel,
        })
    };
    match kind {
        Kind::MessageCreate(event) => to(
            "message_create",
            ReadMessages,
            event.message.as_ref().map(|message| message.channel_id),
        ),
        Kind::MessageUpdate(event) => to(
            "message_update",
            ReadMessages,
            event.message.as_ref().map(|message| message.channel_id),
        ),
        Kind::MessageDelete(event) => to("message_delete", ReadMessages, Some(event.channel_id)),
        Kind::TypingStart(event) => to("typing_start", ReadMessages, Some(event.channel_id)),

        Kind::ChannelCreate(event) => to(
            "channel_create",
            ReadChannels,
            event.channel.as_ref().map(|channel| channel.id),
        ),
        Kind::ChannelUpdate(event) => to(
            "channel_update",
            ReadChannels,
            event.channel.as_ref().map(|channel| channel.id),
        ),
        // The channel is gone, or hidden from the role: visibility works
        // out who hears this (`Scope::User`).
        Kind::ChannelDelete(_) => to("channel_delete", ReadChannels, None),

        Kind::RoleCreate(_) => to("role_create", ReadRoles, None),
        Kind::RoleUpdate(_) => to("role_update", ReadRoles, None),
        Kind::RoleDelete(_) => to("role_delete", ReadRoles, None),

        // Task 17: member_join (with invite_code) and member_leave (with reason
        // and by) are emitted directly at their call sites after the database write.
        Kind::MemberJoin(_) => None,
        Kind::MemberLeave(_) => None,
        Kind::MemberUpdate(_) => to("member_update", ReadMembers, None),
        Kind::PresenceUpdate(_) => to("presence_update", ReadMembers, None),

        Kind::ServerUpdate(_) => to("server_update", ReadServer, None),
        Kind::VoiceSettingsUpdate(_) => to("voice_settings_update", ReadServer, None),
        Kind::UploadSettingsUpdate(_) => to("upload_settings_update", ReadServer, None),

        Kind::VoiceStateUpdate(event) => to(
            "voice_state_update",
            ReadVoice,
            event.voice_state.as_ref().and_then(|state| state.channel_id),
        ),
        Kind::StreamCreate(event) => to(
            "stream_create",
            ReadVoice,
            event.stream.as_ref().map(|stream| stream.channel_id),
        ),
        Kind::StreamUpdate(event) => to(
            "stream_update",
            ReadVoice,
            event.stream.as_ref().map(|stream| stream.channel_id),
        ),
        Kind::StreamDelete(event) => to("stream_delete", ReadVoice, Some(event.channel_id)),
        // Without a channel to check, it goes nowhere.
        Kind::StreamViewersUpdate(event) => {
            stream_channel(&event.stream_key).and_then(|channel| {
                to("stream_viewers_update", ReadVoice, Some(channel))
            })
        }
        Kind::VoiceChannelEffect(event) => to("soundboard_play", ReadVoice, Some(event.channel_id)),
        Kind::SoundboardSoundCreate(_) => to("soundboard_sound_create", ReadVoice, None),
        Kind::SoundboardSoundUpdate(_) => to("soundboard_sound_update", ReadVoice, None),
        Kind::SoundboardSoundDelete(_) => to("soundboard_sound_delete", ReadVoice, None),

        Kind::MusicNowPlaying(event) => to("music_now_playing", ReadMusic, Some(event.channel_id)),
        Kind::MusicUpdate(event) => to("music_queue_update", ReadMusic, Some(event.channel_id)),
        Kind::MusicNotice(event) => to("music_notice", ReadMusic, Some(event.channel_id)),

        // Off-limits whatever is granted (§8.4): private messages, network,
        // storage, uploads in progress, owner-only notices, one session's
        // own events, and plugins' own management.
        Kind::PendingEvent(_)
        | Kind::PrivateTypingStart(_)
        | Kind::PrivateSettingsUpdate(_)
        | Kind::NetworkStatus(_)
        | Kind::ServerNotice(_)
        | Kind::ServerAddressesUpdate(_)
        | Kind::LavalinkNodeStatus(_)
        | Kind::LavalinkNodeEvent(_)
        | Kind::LavalinkTestProgress(_)
        | Kind::StorageUpdate(_)
        | Kind::UploadReady(_)
        | Kind::UploadFailed(_)
        | Kind::VoiceServerUpdate(_)
        | Kind::EphemeralMessage(_)
        | Kind::CommandsUpdate(_)
        | Kind::PluginUpdate(_)
        | Kind::PluginRemove(_)
        | Kind::PluginReviewUpdate(_)
        | Kind::PluginReviewRemove(_)
        | Kind::PluginLogAppend(_)
        | Kind::PluginUsersUpdate(_)
        | Kind::ViewMessage(_)
        | Kind::ViewClosed(_)
        | Kind::ViewScriptsUpdate(_)
        | Kind::PluginChannelTypesUpdate(_)
        | Kind::PluginActivitiesUpdate(_)
        | Kind::ActivitySessionCreate(_)
        | Kind::ActivitySessionUpdate(_)
        | Kind::ActivitySessionDelete(_)
        // §10.4 names no event for these: a session's first music state
        // (plugins read it with `music.state`) and the music settings.
        | Kind::MusicSnapshot(_)
        | Kind::MusicSettingsUpdate(_) => None,
    }
}

/// The channel in a stream key, `stream:{channel_id}:{user_id}`
/// (`voice::streams::key`).
fn stream_channel(key: &str) -> Option<i64> {
    let mut parts = key.strip_prefix("stream:")?.split(':');
    let channel = parts.next()?.parse().ok()?;
    parts.next()?.parse::<i64>().ok()?;
    parts.next().is_none().then_some(channel)
}

/// The routed event's payload: the event's fields named as in the proto,
/// every id (`id`, `*_id`) a decimal string, enums lowercase without their
/// prefix (`"online"`), nested messages as tables, and a user or member as
/// `{ id, name, plugin }` (see [`user_table`]).
pub fn payload(kind: &Kind, guild: &Guild) -> Value {
    match kind {
        Kind::MessageCreate(event) => json!({
            "message": event.message.as_ref().map(|message| message_table(message, guild)),
        }),
        Kind::MemberJoin(event) => json!({
            "member": event.member.as_ref().map(member_table),
        }),
        Kind::VoiceStateUpdate(event) => json!({
            "voice_state": event
                .voice_state
                .as_ref()
                .map(|state| voice_state_table(state, guild)),
        }),
        _ => super::event_tables::payload(kind, guild),
    }
}

/// An id as plugins get it.
pub fn id(value: i64) -> Value {
    Value::String(value.to_string())
}

/// A proto enum's name in lowercase without its prefix:
/// `enum_name("ATTACHMENT_KIND_IMAGE", "ATTACHMENT_KIND_")` is `"image"`.
pub fn enum_name(name: &str, prefix: &str) -> Value {
    Value::String(
        name.strip_prefix(prefix)
            .unwrap_or(name)
            .to_ascii_lowercase(),
    )
}

/// A user as plugins see one: `{ id, name, plugin }`, with `name` the
/// member's nickname or display name, or the plugin's name, and `plugin`
/// the plugin's id for plugin users only. Someone the server no longer
/// knows is `{ id }`.
pub fn user_table(guild: &Guild, user_id: i64) -> Value {
    let mut table = Map::new();
    table.insert("id".to_owned(), id(user_id));
    if let Some(member) = guild.members.get(&user_id) {
        let name = member
            .nickname
            .clone()
            .unwrap_or_else(|| member.user.display_name.clone());
        table.insert("name".to_owned(), Value::String(name));
    } else if let Some(plugin) = guild.plugin_users.get(&user_id) {
        table.insert("name".to_owned(), Value::String(plugin.name.clone()));
        table.insert("plugin".to_owned(), Value::String(plugin.plugin_id.clone()));
    }
    Value::Object(table)
}

/// A member as plugins see one, from the member as sent: `{ id, name }`.
pub fn member_table(member: &proto::Member) -> Value {
    let user = member.user.as_ref();
    let name = user.map(|u| u.display_name.clone()).unwrap_or_default();
    let role_ids: Vec<Value> = member.role_ids.iter().map(|&r| id(r)).collect();
    let mut table = Map::new();
    table.insert("id".to_owned(), id(user.map_or(0, |user| user.id)));
    table.insert("name".to_owned(), Value::String(name));
    if let Some(nick) = &member.nickname {
        table.insert("nickname".to_owned(), Value::String(nick.clone()));
    }
    table.insert("roles".to_owned(), Value::Array(role_ids.clone()));
    table.insert("role_ids".to_owned(), Value::Array(role_ids));
    if let Some(user) = user
        && !user.avatar_hash.is_empty()
    {
        table.insert(
            "avatar_hash".to_owned(),
            Value::String(user.avatar_hash.clone()),
        );
    }
    table.insert("joined_at_ms".to_owned(), json!(member.joined_at_ms));
    Value::Object(table)
}

pub const MEMBER_PERMISSIONS: &[(opencord_common::permissions::Permissions, &str)] = &[
    (
        opencord_common::permissions::Permissions::VIEW_CHANNEL,
        "view_channel",
    ),
    (
        opencord_common::permissions::Permissions::SEND_MESSAGES,
        "send_messages",
    ),
    (
        opencord_common::permissions::Permissions::READ_HISTORY,
        "read_history",
    ),
    (
        opencord_common::permissions::Permissions::MANAGE_MESSAGES,
        "manage_messages",
    ),
    (
        opencord_common::permissions::Permissions::MANAGE_CHANNELS,
        "manage_channels",
    ),
    (
        opencord_common::permissions::Permissions::MANAGE_ROLES,
        "manage_roles",
    ),
    (
        opencord_common::permissions::Permissions::KICK_MEMBERS,
        "kick_members",
    ),
    (
        opencord_common::permissions::Permissions::BAN_MEMBERS,
        "ban_members",
    ),
    (
        opencord_common::permissions::Permissions::CREATE_INVITE,
        "create_invite",
    ),
    (
        opencord_common::permissions::Permissions::MANAGE_SERVER,
        "manage_server",
    ),
    (
        opencord_common::permissions::Permissions::ATTACH_FILES,
        "attach_files",
    ),
    (
        opencord_common::permissions::Permissions::MENTION_EVERYONE,
        "mention_everyone",
    ),
    (
        opencord_common::permissions::Permissions::CHANGE_NICKNAME,
        "change_nickname",
    ),
    (
        opencord_common::permissions::Permissions::MANAGE_NICKNAMES,
        "manage_nicknames",
    ),
    (
        opencord_common::permissions::Permissions::SEND_VOICE_MESSAGES,
        "send_voice_messages",
    ),
    (
        opencord_common::permissions::Permissions::MANAGE_PLUGINS,
        "manage_plugins",
    ),
    (
        opencord_common::permissions::Permissions::CONNECT,
        "connect",
    ),
    (opencord_common::permissions::Permissions::SPEAK, "speak"),
    (opencord_common::permissions::Permissions::VIDEO, "video"),
    (
        opencord_common::permissions::Permissions::SCREENSHARE,
        "screenshare",
    ),
    (
        opencord_common::permissions::Permissions::MUTE_MEMBERS,
        "mute_members",
    ),
    (
        opencord_common::permissions::Permissions::DEAFEN_MEMBERS,
        "deafen_members",
    ),
    (
        opencord_common::permissions::Permissions::MOVE_MEMBERS,
        "move_members",
    ),
    (
        opencord_common::permissions::Permissions::PRIORITY_SPEAKER,
        "priority_speaker",
    ),
    (
        opencord_common::permissions::Permissions::USE_VOICE_ACTIVITY,
        "use_voice_activity",
    ),
    (
        opencord_common::permissions::Permissions::USE_SOUNDBOARD,
        "use_soundboard",
    ),
    (
        opencord_common::permissions::Permissions::USE_EXTERNAL_SOUNDS,
        "use_external_sounds",
    ),
    (
        opencord_common::permissions::Permissions::MANAGE_SOUNDBOARD,
        "manage_soundboard",
    ),
    (
        opencord_common::permissions::Permissions::MUSIC_ADD,
        "music_add",
    ),
    (
        opencord_common::permissions::Permissions::MUSIC_CONTROL,
        "music_control",
    ),
    (
        opencord_common::permissions::Permissions::MUSIC_MANAGE_QUEUE,
        "music_manage_queue",
    ),
    (
        opencord_common::permissions::Permissions::MUSIC_MANAGE,
        "music_manage",
    ),
    (
        opencord_common::permissions::Permissions::MUSIC_ALL_LOCAL_SONGS,
        "music_all_local_songs",
    ),
    (
        opencord_common::permissions::Permissions::ADMINISTRATOR,
        "administrator",
    ),
];

/// Converts a permission bitset into a list of snake_case permission names.
pub fn permissions_to_names(perms: opencord_common::permissions::Permissions) -> Vec<String> {
    MEMBER_PERMISSIONS
        .iter()
        .filter(|(bit, _)| perms.contains(*bit))
        .map(|(_, name)| (*name).to_owned())
        .collect()
}

pub fn member_permission_from_name(
    name: &str,
) -> Option<opencord_common::permissions::Permissions> {
    MEMBER_PERMISSIONS
        .iter()
        .find(|(_, n)| *n == name)
        .map(|(bit, _)| *bit)
}

/// A message as plugins see one: `{ id, channel_id, author, content,
/// created_at_ms, edited_at_ms?, interaction?, attachments }`.
pub fn message_table(message: &proto::Message, guild: &Guild) -> Value {
    let mut table = Map::new();
    table.insert("id".to_owned(), id(message.id));
    table.insert("channel_id".to_owned(), id(message.channel_id));
    table.insert("author".to_owned(), user_table(guild, message.author_id));
    table.insert("content".to_owned(), Value::String(message.content.clone()));
    table.insert("created_at_ms".to_owned(), json!(message.created_at_ms));
    if let Some(edited) = message.edited_at_ms {
        table.insert("edited_at_ms".to_owned(), json!(edited));
    }
    if let Some(interaction) = &message.interaction {
        table.insert(
            "interaction".to_owned(),
            json!({
                "user": user_table(guild, interaction.user_id),
                "command": interaction.command,
            }),
        );
    }
    let attachments: Vec<Value> = message
        .attachments
        .iter()
        .map(|attachment| {
            json!({
                "id": id(attachment.id),
                "filename": attachment.filename,
                "kind": enum_name(attachment.kind().as_str_name(), "ATTACHMENT_KIND_"),
                "size": attachment.size_bytes,
            })
        })
        .collect();
    table.insert("attachments".to_owned(), Value::Array(attachments));
    Value::Object(table)
}

/// A voice state as plugins see one. The session holding it stays out
/// (§8.4: member sessions are off-limits).
pub fn voice_state_table(state: &proto::VoiceState, guild: &Guild) -> Value {
    let mut table = Map::new();
    table.insert("user".to_owned(), user_table(guild, state.user_id));
    if let Some(channel_id) = state.channel_id {
        table.insert("channel_id".to_owned(), id(channel_id));
    }
    for (name, value) in [
        ("self_mute", state.self_mute),
        ("self_deaf", state.self_deaf),
        ("server_mute", state.server_mute),
        ("server_deaf", state.server_deaf),
        ("suppress", state.suppress),
        ("self_video", state.self_video),
        ("self_stream", state.self_stream),
        ("video", state.self_video),
        ("streaming", state.self_stream),
    ] {
        table.insert(name.to_owned(), Value::Bool(value));
    }
    Value::Object(table)
}

impl Plugins {
    /// Offers a gateway event sent to `audience` to every plugin that may
    /// see it. Called by `AppState::broadcast` with the guild read: it only
    /// builds the payload (once) and pushes into queues, never waits.
    pub fn observe(&self, guild: &Guild, kind: &Kind, audience: Audience) {
        self.observe_in(guild, kind, Scope::Audience(audience));
    }

    /// [`Self::observe`] for events fanned out outside `broadcast`.
    pub fn observe_in(&self, guild: &Guild, kind: &Kind, scope: Scope<'_>) {
        let Some(route) = route(kind) else { return };
        let built = OnceCell::new();
        self.deliver_all(|handle| {
            hears(guild, handle, &route, scope).then(|| PluginEvent {
                name: route.name.to_owned(),
                payload: built.get_or_init(|| payload(kind, guild)).clone(),
                reply: None,
                command: None,
            })
        });
    }
}

fn hears(guild: &Guild, handle: &PluginHandle, route: &Route, scope: Scope<'_>) -> bool {
    if !handle.holds(route.needs) {
        return false;
    }
    let user_id = handle.user_id;
    match scope {
        Scope::User(target) => target == user_id,
        Scope::AnyOf(channels) => channels
            .iter()
            .any(|channel_id| guild.can_view(user_id, *channel_id)),
        Scope::Audience(audience) => {
            route
                .channel
                .is_none_or(|channel_id| guild.can_view(user_id, channel_id))
                && match audience {
                    Audience::Everyone | Audience::User(_) => true,
                    Audience::Channel(channel_id) | Audience::Voice(channel_id) => {
                        guild.can_view(user_id, channel_id)
                    }
                    Audience::Holding(needed) => guild.base_permissions(user_id).contains(needed),
                }
        }
    }
}

/// Sends a plugin-only event, one no gateway event carries, to every
/// plugin holding `needs` whose role can view `channel` (when given).
pub fn emit(
    state: &AppState,
    name: &str,
    needs: PluginPermission,
    channel: Option<i64>,
    payload: Value,
) {
    let guild = state.guild();
    state.plugins.deliver_all(|handle| {
        (handle.holds(needs)
            && channel.is_none_or(|channel_id| guild.can_view(handle.user_id, channel_id)))
        .then(|| PluginEvent {
            name: name.to_owned(),
            payload: payload.clone(),
            reply: None,
            command: None,
        })
    });
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
