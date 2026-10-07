//! Conversions between protobuf messages and the Dart-facing types.

use opencord_proto::v1 as proto;

use crate::api::types::{
    Ban, Channel, ChannelKind, ChannelPermissions, CoreError, CoreEventPayload, ErrorCode, Invite,
    Member, Message, OverwriteTargetKind, PermissionOverwrite, Presence, PresenceStatus,
    ReadySnapshot, Role, ScreenShareResolution, ServerInfo, User, VoiceSettings,
    VoiceSettingsChanges, VoiceState,
};
use crate::identity::public_key_fingerprint;

/// The protocol's `u64` permission bits as the `i64` Dart sees.
pub fn bits_to_api(bits: u64) -> i64 {
    i64::from_ne_bytes(bits.to_ne_bytes())
}

pub fn bits_from_api(bits: i64) -> u64 {
    u64::from_ne_bytes(bits.to_ne_bytes())
}

pub fn user(user: proto::User) -> User {
    User {
        id: user.id,
        public_key_hex: hex::encode(&user.public_key),
        fingerprint: public_key_fingerprint(&user.public_key),
        display_name: user.display_name,
    }
}

pub fn member(member: proto::Member) -> Member {
    Member {
        user: user(member.user.unwrap_or_default()),
        nickname: member.nickname,
        role_ids: member.role_ids,
        joined_at_ms: member.joined_at_ms,
    }
}

pub fn role(role: proto::Role) -> Role {
    Role {
        id: role.id,
        name: role.name,
        color: role.color,
        position: role.position,
        permissions: bits_to_api(role.permissions),
        hoist: role.hoist,
        mentionable: role.mentionable,
    }
}

pub fn channel_kind(kind: i32) -> ChannelKind {
    match proto::ChannelKind::try_from(kind) {
        Ok(proto::ChannelKind::Voice) => ChannelKind::Voice,
        Ok(proto::ChannelKind::Category) => ChannelKind::Category,
        _ => ChannelKind::Text,
    }
}

pub fn channel_kind_to_proto(kind: ChannelKind) -> proto::ChannelKind {
    match kind {
        ChannelKind::Text => proto::ChannelKind::Text,
        ChannelKind::Voice => proto::ChannelKind::Voice,
        ChannelKind::Category => proto::ChannelKind::Category,
    }
}

pub fn target_kind_to_proto(kind: OverwriteTargetKind) -> proto::OverwriteTarget {
    match kind {
        OverwriteTargetKind::Role => proto::OverwriteTarget::Role,
        OverwriteTargetKind::Member => proto::OverwriteTarget::Member,
    }
}

pub fn overwrite(overwrite: proto::PermissionOverwrite) -> PermissionOverwrite {
    PermissionOverwrite {
        target_kind: match proto::OverwriteTarget::try_from(overwrite.target_kind) {
            Ok(proto::OverwriteTarget::Member) => OverwriteTargetKind::Member,
            _ => OverwriteTargetKind::Role,
        },
        target_id: overwrite.target_id,
        allow: bits_to_api(overwrite.allow),
        deny: bits_to_api(overwrite.deny),
    }
}

pub fn channel(channel: proto::Channel) -> Channel {
    Channel {
        id: channel.id,
        kind: channel_kind(channel.kind),
        name: channel.name,
        topic: channel.topic,
        parent_id: channel.parent_id,
        position: channel.position,
        overwrites: channel.overwrites.into_iter().map(overwrite).collect(),
        bitrate: channel.bitrate,
        user_limit: channel.user_limit,
        text_in_voice: channel.text_in_voice,
    }
}

/// `session_id` is this device's session on that server.
pub fn voice_state(state: proto::VoiceState, session_id: &str) -> VoiceState {
    VoiceState {
        user_id: state.user_id,
        channel_id: state.channel_id,
        this_device: !session_id.is_empty() && state.session_id == session_id,
        self_mute: state.self_mute,
        self_deaf: state.self_deaf,
        server_mute: state.server_mute,
        server_deaf: state.server_deaf,
        suppress: state.suppress,
        self_video: state.self_video,
        self_stream: state.self_stream,
    }
}

pub fn screen_share_resolution(value: i32) -> ScreenShareResolution {
    match proto::ScreenShareResolution::try_from(value) {
        Ok(proto::ScreenShareResolution::ScreenShareResolution480p) => ScreenShareResolution::P480,
        Ok(proto::ScreenShareResolution::ScreenShareResolution1080p) => {
            ScreenShareResolution::P1080
        }
        Ok(proto::ScreenShareResolution::ScreenShareResolution1440p) => {
            ScreenShareResolution::P1440
        }
        Ok(proto::ScreenShareResolution::Source) => ScreenShareResolution::Source,
        _ => ScreenShareResolution::P720,
    }
}

pub fn screen_share_resolution_to_proto(
    resolution: ScreenShareResolution,
) -> proto::ScreenShareResolution {
    match resolution {
        ScreenShareResolution::P480 => proto::ScreenShareResolution::ScreenShareResolution480p,
        ScreenShareResolution::P720 => proto::ScreenShareResolution::ScreenShareResolution720p,
        ScreenShareResolution::P1080 => proto::ScreenShareResolution::ScreenShareResolution1080p,
        ScreenShareResolution::P1440 => proto::ScreenShareResolution::ScreenShareResolution1440p,
        ScreenShareResolution::Source => proto::ScreenShareResolution::Source,
    }
}

pub fn voice_settings(settings: proto::VoiceSettings) -> VoiceSettings {
    VoiceSettings {
        screen_share_max_resolution: screen_share_resolution(settings.screen_share_max_resolution),
        screen_share_max_fps: settings.screen_share_max_fps,
        max_stream_viewers: settings.max_stream_viewers,
        camera_allowed: settings.camera_allowed,
        max_camera_participants: settings.max_camera_participants,
        max_voice_bitrate: settings.max_voice_bitrate,
        afk_channel_id: settings.afk_channel_id,
        afk_timeout_s: settings.afk_timeout_s,
        soundboard_enabled: settings.soundboard_enabled,
        allow_default_sounds: settings.allow_default_sounds,
        allow_external_sounds: settings.allow_external_sounds,
        sound_cooldown_s: settings.sound_cooldown_s,
        max_sounds: settings.max_sounds,
    }
}

pub fn voice_settings_changes(changes: VoiceSettingsChanges) -> proto::UpdateVoiceSettings {
    proto::UpdateVoiceSettings {
        screen_share_max_resolution: changes
            .screen_share_max_resolution
            .map(|resolution| screen_share_resolution_to_proto(resolution) as i32),
        screen_share_max_fps: changes.screen_share_max_fps,
        max_stream_viewers: changes.max_stream_viewers,
        camera_allowed: changes.camera_allowed,
        max_camera_participants: changes.max_camera_participants,
        max_voice_bitrate: changes.max_voice_bitrate,
        afk_channel_id: changes.afk_channel_id,
        afk_timeout_s: changes.afk_timeout_s,
        soundboard_enabled: changes.soundboard_enabled,
        allow_default_sounds: changes.allow_default_sounds,
        allow_external_sounds: changes.allow_external_sounds,
        sound_cooldown_s: changes.sound_cooldown_s,
        max_sounds: changes.max_sounds,
    }
}

pub fn message(message: proto::Message) -> Message {
    Message {
        id: message.id,
        channel_id: message.channel_id,
        author_id: message.author_id,
        content: message.content,
        created_at_ms: message.created_at_ms,
        edited_at_ms: message.edited_at_ms,
        nonce: message.nonce,
    }
}

pub fn server_info(info: proto::ServerInfo) -> ServerInfo {
    ServerInfo {
        server_id_hex: hex::encode(&info.server_id),
        name: info.name,
        description: info.description,
        owner_id: info.owner_id,
        open_join: info.open_join,
        everyone_role_id: info.everyone_role_id,
    }
}

pub fn presence_status(status: i32) -> PresenceStatus {
    match proto::PresenceStatus::try_from(status) {
        Ok(proto::PresenceStatus::Online) => PresenceStatus::Online,
        Ok(proto::PresenceStatus::Idle) => PresenceStatus::Idle,
        Ok(proto::PresenceStatus::Dnd) => PresenceStatus::Dnd,
        _ => PresenceStatus::Offline,
    }
}

pub fn presence_status_to_proto(status: PresenceStatus) -> proto::PresenceStatus {
    match status {
        PresenceStatus::Online => proto::PresenceStatus::Online,
        PresenceStatus::Idle => proto::PresenceStatus::Idle,
        PresenceStatus::Dnd => proto::PresenceStatus::Dnd,
        PresenceStatus::Offline => proto::PresenceStatus::Offline,
    }
}

pub fn presence(presence: proto::Presence) -> Presence {
    Presence {
        user_id: presence.user_id,
        status: presence_status(presence.status),
    }
}

pub fn ban(ban: proto::Ban) -> Ban {
    Ban {
        user: user(ban.user.unwrap_or_default()),
        reason: ban.reason,
        banned_by: ban.banned_by,
        created_at_ms: ban.created_at_ms,
    }
}

pub fn invite(invite: proto::Invite, link: String) -> Invite {
    Invite {
        code: invite.code,
        link,
        created_by: invite.created_by,
        created_at_ms: invite.created_at_ms,
        max_uses: invite.max_uses,
        uses: invite.uses,
        expires_at_ms: invite.expires_at_ms,
    }
}

pub fn channel_permissions(
    permissions: impl IntoIterator<Item = (i64, u64)>,
) -> Vec<ChannelPermissions> {
    let mut list: Vec<ChannelPermissions> = permissions
        .into_iter()
        .map(|(channel_id, permissions)| ChannelPermissions {
            channel_id,
            permissions: bits_to_api(permissions),
        })
        .collect();
    list.sort_by_key(|entry| entry.channel_id);
    list
}

pub fn ready(ready: proto::Ready) -> ReadySnapshot {
    let session_id = ready.session_id;
    ReadySnapshot {
        self_user: user(ready.self_user.unwrap_or_default()),
        server: server_info(ready.server.unwrap_or_default()),
        channels: ready.channels.into_iter().map(channel).collect(),
        roles: ready.roles.into_iter().map(role).collect(),
        members: ready.members.into_iter().map(member).collect(),
        presences: ready.presences.into_iter().map(presence).collect(),
        server_permissions: bits_to_api(ready.server_permissions),
        channel_permissions: channel_permissions(ready.channel_permissions),
        voice_enabled: ready.voice_enabled,
        voice_states: ready
            .voice_states
            .into_iter()
            .map(|state| voice_state(state, &session_id))
            .collect(),
        voice_settings: voice_settings(ready.voice_settings.unwrap_or_default()),
    }
}

/// `None` for events with a missing body, and for events the core keeps to
/// itself. `session_id` is this device's session on that server.
pub fn event(kind: proto::event::Kind, session_id: &str) -> Option<CoreEventPayload> {
    use proto::event::Kind;
    Some(match kind {
        Kind::MessageCreate(event) => CoreEventPayload::MessageCreate(message(event.message?)),
        Kind::MessageUpdate(event) => CoreEventPayload::MessageUpdate(message(event.message?)),
        Kind::MessageDelete(event) => CoreEventPayload::MessageDelete {
            channel_id: event.channel_id,
            message_id: event.message_id,
        },
        Kind::ChannelCreate(event) => CoreEventPayload::ChannelCreate(channel(event.channel?)),
        Kind::ChannelUpdate(event) => CoreEventPayload::ChannelUpdate(channel(event.channel?)),
        Kind::ChannelDelete(event) => CoreEventPayload::ChannelDelete {
            channel_id: event.channel_id,
        },
        Kind::RoleCreate(event) => CoreEventPayload::RoleCreate(role(event.role?)),
        Kind::RoleUpdate(event) => CoreEventPayload::RoleUpdate(role(event.role?)),
        Kind::RoleDelete(event) => CoreEventPayload::RoleDelete {
            role_id: event.role_id,
        },
        Kind::MemberJoin(event) => CoreEventPayload::MemberJoin(member(event.member?)),
        Kind::MemberLeave(event) => CoreEventPayload::MemberLeave {
            user_id: event.user_id,
        },
        Kind::MemberUpdate(event) => CoreEventPayload::MemberUpdate(member(event.member?)),
        Kind::PresenceUpdate(event) => CoreEventPayload::PresenceUpdate(presence(event.presence?)),
        Kind::TypingStart(event) => CoreEventPayload::TypingStart {
            channel_id: event.channel_id,
            user_id: event.user_id,
        },
        Kind::ServerUpdate(event) => CoreEventPayload::ServerUpdate(server_info(event.server?)),
        Kind::VoiceStateUpdate(event) => {
            CoreEventPayload::VoiceStateUpdate(voice_state(event.voice_state?, session_id))
        }
        Kind::VoiceSettingsUpdate(event) => {
            CoreEventPayload::VoiceSettingsUpdate(voice_settings(event.settings?))
        }
        // The media engine's business (V1 onward), not the app's.
        Kind::VoiceServerUpdate(_) => return None,
        // Screen share and soundboard events arrive with their milestones.
        Kind::StreamCreate(_)
        | Kind::StreamUpdate(_)
        | Kind::StreamDelete(_)
        | Kind::StreamViewersUpdate(_)
        | Kind::VoiceChannelEffect(_)
        | Kind::SoundboardSoundCreate(_)
        | Kind::SoundboardSoundUpdate(_)
        | Kind::SoundboardSoundDelete(_) => return None,
    })
}

pub fn error_code(code: i32) -> ErrorCode {
    match proto::ErrorCode::try_from(code) {
        Ok(proto::ErrorCode::Unauthorized) => ErrorCode::Unauthorized,
        Ok(proto::ErrorCode::Forbidden) => ErrorCode::Forbidden,
        Ok(proto::ErrorCode::NotFound) => ErrorCode::NotFound,
        Ok(proto::ErrorCode::InvalidArgument) => ErrorCode::InvalidArgument,
        Ok(proto::ErrorCode::RateLimited) => ErrorCode::RateLimited,
        Ok(proto::ErrorCode::InvalidSession) => ErrorCode::InvalidSession,
        Ok(proto::ErrorCode::Conflict) => ErrorCode::Conflict,
        Ok(proto::ErrorCode::Internal) => ErrorCode::Internal,
        Ok(proto::ErrorCode::VoiceChannelFull) => ErrorCode::VoiceChannelFull,
        Ok(proto::ErrorCode::VoiceNotConnected) => ErrorCode::VoiceNotConnected,
        Ok(proto::ErrorCode::QualityLimit) => ErrorCode::QualityLimit,
        Ok(proto::ErrorCode::CameraLimit) => ErrorCode::CameraLimit,
        Ok(proto::ErrorCode::StreamViewerLimit) => ErrorCode::StreamViewerLimit,
        Ok(proto::ErrorCode::SoundCooldown) => ErrorCode::SoundCooldown,
        Ok(proto::ErrorCode::SoundTooLong) => ErrorCode::SoundTooLong,
        Ok(proto::ErrorCode::SoundInvalid) => ErrorCode::SoundInvalid,
        Ok(proto::ErrorCode::SoundboardFull) => ErrorCode::SoundboardFull,
        _ => ErrorCode::Unknown,
    }
}

pub fn error(error: proto::Error) -> CoreError {
    CoreError::Server {
        code: error_code(error.code),
        message: error.message,
        retry_after_ms: error
            .retry_after_ms
            .map(|ms| u32::try_from(ms).unwrap_or(u32::MAX)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::{ScreenShareResolution, VoiceState};

    const ME: i64 = 7;

    fn voice(user_id: i64, session_id: &str, channel_id: Option<i64>) -> proto::VoiceState {
        proto::VoiceState {
            user_id,
            channel_id,
            session_id: session_id.to_owned(),
            self_mute: true,
            ..Default::default()
        }
    }

    #[test]
    fn voice_states_say_whether_this_session_holds_them() {
        let here = voice_state(voice(ME, "mine", Some(5)), "mine");
        let elsewhere = voice_state(voice(ME, "other", Some(5)), "mine");

        assert_eq!(
            here,
            VoiceState {
                user_id: ME,
                channel_id: Some(5),
                this_device: true,
                self_mute: true,
                self_deaf: false,
                server_mute: false,
                server_deaf: false,
                suppress: false,
                self_video: false,
                self_stream: false,
            }
        );
        assert!(!elsewhere.this_device);
    }

    #[test]
    fn ready_marks_voice_states_against_its_own_session() {
        let ready = proto::Ready {
            session_id: "mine".to_owned(),
            voice_enabled: true,
            voice_states: vec![voice(ME, "mine", Some(5)), voice(8, "theirs", Some(5))],
            voice_settings: Some(proto::VoiceSettings {
                screen_share_max_resolution:
                    proto::ScreenShareResolution::ScreenShareResolution1080p as i32,
                max_voice_bitrate: 96_000,
                ..Default::default()
            }),
            ..Default::default()
        };

        let snapshot = super::ready(ready);

        assert!(snapshot.voice_enabled);
        let held: Vec<bool> = snapshot
            .voice_states
            .iter()
            .map(|s| s.this_device)
            .collect();
        assert_eq!(held, [true, false]);
        assert_eq!(
            snapshot.voice_settings.screen_share_max_resolution,
            ScreenShareResolution::P1080
        );
        assert_eq!(snapshot.voice_settings.max_voice_bitrate, 96_000);
    }

    #[test]
    fn voice_events_convert_and_server_updates_stay_in_the_core() {
        let update = event(
            proto::event::Kind::VoiceStateUpdate(proto::VoiceStateUpdate {
                voice_state: Some(voice(ME, "mine", None)),
            }),
            "mine",
        );
        let server = event(
            proto::event::Kind::VoiceServerUpdate(proto::VoiceServerUpdate::default()),
            "mine",
        );

        assert!(matches!(
            update,
            Some(CoreEventPayload::VoiceStateUpdate(VoiceState {
                channel_id: None,
                this_device: true,
                ..
            }))
        ));
        assert_eq!(server, None);
    }

    #[test]
    fn voice_error_codes_have_names() {
        assert_eq!(
            error_code(proto::ErrorCode::VoiceChannelFull as i32),
            ErrorCode::VoiceChannelFull
        );
        assert_eq!(
            error_code(proto::ErrorCode::SoundboardFull as i32),
            ErrorCode::SoundboardFull
        );
    }

    #[test]
    fn resolutions_round_trip() {
        for resolution in [
            ScreenShareResolution::P480,
            ScreenShareResolution::P720,
            ScreenShareResolution::P1080,
            ScreenShareResolution::P1440,
            ScreenShareResolution::Source,
        ] {
            assert_eq!(
                screen_share_resolution(screen_share_resolution_to_proto(resolution) as i32),
                resolution
            );
        }
    }
}
