//! Conversions between protobuf messages and the Dart-facing types.

use opencord_proto::v1 as proto;

use crate::api::types::{
    Attachment, AttachmentKind, Ban, Channel, ChannelKind, ChannelPermissions, ConnectAddress,
    CoreError, CoreEventPayload, EphemeralMessage, ErrorCode, Invite, Member, Message,
    OverwriteTargetKind, PermissionOverwrite, Presence, PresenceStatus, ReadySnapshot, Role,
    S3Config, ScreenShareResolution, ScreenStream, ServerAddresses, ServerInfo, SoundEffect,
    SoundboardSound, StorageBackend, StorageInfo, StorageKind, StorageMove, StorageTestResult,
    StorageTestStep, StreamSourceKind, UploadSettings, User, VoiceSettings, VoiceSettingsChanges,
    VoiceState,
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
        avatar_hash: (!user.avatar_hash.is_empty()).then_some(user.avatar_hash),
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
        max_upload_bytes: role.max_upload_bytes,
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

pub fn stream(stream: proto::Stream) -> ScreenStream {
    ScreenStream {
        stream_key: stream.stream_key,
        channel_id: stream.channel_id,
        user_id: stream.user_id,
        source_kind: match proto::StreamSourceKind::try_from(stream.source_kind) {
            Ok(proto::StreamSourceKind::Window) => StreamSourceKind::Window,
            _ => StreamSourceKind::Screen,
        },
        resolution: screen_share_resolution(stream.resolution),
        fps: stream.fps,
        has_audio: stream.has_audio,
        viewer_count: stream.viewer_count,
    }
}

pub fn stream_source_kind_to_proto(kind: StreamSourceKind) -> proto::StreamSourceKind {
    match kind {
        StreamSourceKind::Screen => proto::StreamSourceKind::Screen,
        StreamSourceKind::Window => proto::StreamSourceKind::Window,
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
        command: message.interaction.map(|interaction| interaction.command),
        attachments: message.attachments.into_iter().map(attachment).collect(),
    }
}

pub fn attachment(attachment: proto::Attachment) -> Attachment {
    let kind = match proto::AttachmentKind::try_from(attachment.kind) {
        Ok(proto::AttachmentKind::Image) => AttachmentKind::Image,
        Ok(proto::AttachmentKind::Video) => AttachmentKind::Video,
        Ok(proto::AttachmentKind::Audio) => AttachmentKind::Audio,
        Ok(proto::AttachmentKind::VoiceMessage) => AttachmentKind::VoiceMessage,
        _ => AttachmentKind::File,
    };
    Attachment {
        id: attachment.id,
        filename: attachment.filename,
        content_type: attachment.content_type,
        size_bytes: attachment.size_bytes,
        kind,
        width: attachment.width,
        height: attachment.height,
        duration_ms: attachment.duration_ms,
        waveform: attachment.waveform,
        has_thumbnail: attachment.has_thumbnail,
        thumbnail_width: attachment.thumbnail_width,
        thumbnail_height: attachment.thumbnail_height,
        animated: attachment.animated,
    }
}

pub fn upload_settings(settings: proto::UploadSettings) -> UploadSettings {
    UploadSettings {
        blocked_extensions: settings.blocked_extensions,
        voice_message_max_seconds: settings.voice_message_max_seconds,
        max_file_bytes: settings.max_file_bytes,
        max_attachments: settings.max_attachments,
    }
}

pub fn storage(storage: proto::Storage) -> StorageInfo {
    StorageInfo {
        backends: storage.backends.into_iter().map(storage_backend).collect(),
        move_status: storage.r#move.map(|status| StorageMove {
            to_storage_id: status.to_storage_id,
            running: status.running,
            files_done: status.files_done,
            files_total: status.files_total,
            bytes_done: status.bytes_done,
            bytes_total: status.bytes_total,
            failed: status.failed,
        }),
        temp_free_bytes: storage.temp_free_bytes,
    }
}

fn storage_backend(backend: proto::StorageBackend) -> StorageBackend {
    let kind = match proto::StorageKind::try_from(backend.kind) {
        Ok(proto::StorageKind::S3) => StorageKind::S3,
        _ => StorageKind::Local,
    };
    StorageBackend {
        id: backend.id,
        kind,
        name: backend.name,
        active: backend.active,
        s3: backend.s3.map(s3_config),
        has_secret: backend.has_secret,
        file_count: backend.file_count,
        bytes: backend.bytes,
        last_error: backend.last_error,
    }
}

fn s3_config(config: proto::S3Config) -> S3Config {
    S3Config {
        endpoint: config.endpoint,
        region: config.region,
        bucket: config.bucket,
        key_prefix: config.key_prefix,
        path_style: config.path_style,
        allow_http: config.allow_http,
        access_key_id: config.access_key_id,
        secret_access_key: config.secret_access_key,
    }
}

pub fn s3_config_to_proto(config: S3Config) -> proto::S3Config {
    proto::S3Config {
        endpoint: config.endpoint,
        region: config.region,
        bucket: config.bucket,
        key_prefix: config.key_prefix,
        path_style: config.path_style,
        allow_http: config.allow_http,
        access_key_id: config.access_key_id,
        secret_access_key: config.secret_access_key,
    }
}

pub fn storage_test(test: proto::StorageTest) -> StorageTestResult {
    StorageTestResult {
        steps: test
            .steps
            .into_iter()
            .map(|step| StorageTestStep {
                name: step.name,
                ok: step.ok,
                detail: step.detail,
            })
            .collect(),
        ok: test.ok,
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
        icon_hash: (!info.icon_hash.is_empty()).then_some(info.icon_hash),
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

pub fn invite(invite: proto::Invite, link: String, web_link: Option<String>) -> Invite {
    Invite {
        domain_id: invite
            .address
            .as_ref()
            .map(|address| address.domain_id)
            .filter(|id| *id != 0),
        code: invite.code,
        link,
        web_link,
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
        streams: ready.streams.into_iter().map(stream).collect(),
        voice_settings: voice_settings(ready.voice_settings.unwrap_or_default()),
        soundboard_sounds: ready
            .soundboard_sounds
            .into_iter()
            .map(soundboard_sound)
            .collect(),
        addresses: server_addresses(ready.server_addresses.unwrap_or_default()),
        commands: crate::commands::list_from(ready.commands.unwrap_or_default()),
        upload_settings: upload_settings(ready.upload_settings.unwrap_or_default()),
        private_messages: ready.private_messages,
    }
}

pub fn server_addresses(addresses: proto::ServerAddresses) -> ServerAddresses {
    ServerAddresses {
        primary: addresses.primary.map(connect_address),
        domains: addresses.domains.into_iter().map(connect_address).collect(),
        ip: addresses.ip.map(connect_address),
    }
}

pub fn connect_address(address: proto::ConnectAddress) -> ConnectAddress {
    ConnectAddress {
        host: address.host,
        port: address.port,
        trusted_certificate: address.trusted_certificate,
        domain_id: address.domain_id,
    }
}

pub fn soundboard_sound(sound: proto::SoundboardSound) -> SoundboardSound {
    SoundboardSound {
        id: sound.id,
        name: sound.name,
        emoji: sound.emoji,
        volume: sound.volume,
        sha256: hex::encode(sound.sha256),
        size_bytes: sound.size_bytes,
        duration_ms: sound.duration_ms,
        uploader_id: sound.uploader_id,
        created_at_ms: sound.created_at_ms,
    }
}

pub fn sound_effect(effect: proto::VoiceChannelEffect) -> SoundEffect {
    SoundEffect {
        channel_id: effect.channel_id,
        user_id: effect.user_id,
        sound_ref: effect.sound_ref,
        name: effect.name,
        emoji: effect.emoji,
        volume: effect.volume,
        started_at_ms: effect.started_at_ms,
        duration_ms: effect.duration_ms,
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
        Kind::StreamCreate(event) => CoreEventPayload::StreamCreate(stream(event.stream?)),
        Kind::StreamUpdate(event) => CoreEventPayload::StreamUpdate(stream(event.stream?)),
        Kind::StreamDelete(event) => CoreEventPayload::StreamDelete {
            stream_key: event.stream_key,
            channel_id: event.channel_id,
        },
        Kind::StreamViewersUpdate(event) => CoreEventPayload::StreamViewersUpdate {
            stream_key: event.stream_key,
            viewer_ids: event.viewer_ids,
        },
        // Soundboard events arrive with their milestone.
        Kind::VoiceChannelEffect(effect) => {
            CoreEventPayload::VoiceChannelEffect(sound_effect(effect))
        }
        Kind::SoundboardSoundCreate(event) => {
            CoreEventPayload::SoundboardSoundCreate(soundboard_sound(event.sound?))
        }
        Kind::SoundboardSoundUpdate(event) => {
            CoreEventPayload::SoundboardSoundUpdate(soundboard_sound(event.sound?))
        }
        Kind::SoundboardSoundDelete(event) => CoreEventPayload::SoundboardSoundDelete {
            sound_id: event.sound_id,
        },
        Kind::NetworkStatus(event) => {
            CoreEventPayload::NetworkStatus(crate::network::info_from(event.info?))
        }
        Kind::ServerNotice(notice) => {
            CoreEventPayload::ServerNotice(crate::network::notice(notice))
        }
        Kind::ServerAddressesUpdate(update) => {
            CoreEventPayload::ServerAddressesUpdate(server_addresses(update.addresses?))
        }
        Kind::LavalinkNodeStatus(status) => {
            let node = status.node?;
            if status.removed {
                CoreEventPayload::LavalinkNodeRemoved {
                    node_id: node.id,
                    channels_playing: status.channels_playing,
                }
            } else {
                CoreEventPayload::LavalinkNodeStatus {
                    node: crate::lavalink::node_from(node),
                    channels_playing: status.channels_playing,
                }
            }
        }
        Kind::LavalinkNodeEvent(event) => {
            CoreEventPayload::LavalinkNodeEvent(crate::lavalink::event_from(event))
        }
        Kind::LavalinkTestProgress(progress) => {
            CoreEventPayload::LavalinkTestProgress(crate::lavalink::test_progress_from(progress))
        }
        Kind::MusicSettingsUpdate(update) => {
            CoreEventPayload::MusicSettingsUpdate(crate::music::settings_from(update.settings?))
        }
        Kind::CommandsUpdate(update) => CoreEventPayload::CommandsUpdate(
            crate::commands::list_from(update.commands.unwrap_or_default()),
        ),
        Kind::EphemeralMessage(message) => CoreEventPayload::EphemeralMessage(EphemeralMessage {
            channel_id: message.channel_id,
            nonce: message.nonce,
            content: message.content,
        }),
        Kind::UploadSettingsUpdate(update) => {
            CoreEventPayload::UploadSettingsUpdate(upload_settings(update.settings?))
        }
        Kind::StorageUpdate(update) => CoreEventPayload::StorageUpdate(storage(update.storage?)),
        // With the server's clock: `music::event`.
        Kind::MusicSnapshot(_)
        | Kind::MusicUpdate(_)
        | Kind::MusicNotice(_)
        | Kind::MusicNowPlaying(_)
        // For the send waiting on them: `connection.rs` hands them to
        // `Uploads`.
        | Kind::UploadReady(_)
        | Kind::UploadFailed(_)
        | Kind::PendingEvent(_)
        | Kind::PrivateTypingStart(_) => return None,
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
        Ok(proto::ErrorCode::DomainInvalid) => ErrorCode::DomainInvalid,
        Ok(proto::ErrorCode::DomainExists) => ErrorCode::DomainExists,
        Ok(proto::ErrorCode::DomainLimit) => ErrorCode::DomainLimit,
        Ok(proto::ErrorCode::DomainNotVerified) => ErrorCode::DomainNotVerified,
        Ok(proto::ErrorCode::DnsMismatch) => ErrorCode::DnsMismatch,
        Ok(proto::ErrorCode::AcmeFailed) => ErrorCode::AcmeFailed,
        Ok(proto::ErrorCode::AcmeRateLimited) => ErrorCode::AcmeRateLimited,
        Ok(proto::ErrorCode::PortUnreachable) => ErrorCode::PortUnreachable,
        Ok(proto::ErrorCode::MusicNotConfigured) => ErrorCode::MusicNotConfigured,
        Ok(proto::ErrorCode::MusicNodeUnavailable) => ErrorCode::MusicNodeUnavailable,
        Ok(proto::ErrorCode::MusicNotInChannel) => ErrorCode::MusicNotInChannel,
        Ok(proto::ErrorCode::MusicBlocked) => ErrorCode::MusicBlocked,
        Ok(proto::ErrorCode::MusicQueueFull) => ErrorCode::MusicQueueFull,
        Ok(proto::ErrorCode::MusicMemberLimit) => ErrorCode::MusicMemberLimit,
        Ok(proto::ErrorCode::MusicTrackTooLong) => ErrorCode::MusicTrackTooLong,
        Ok(proto::ErrorCode::MusicStreamsNotAllowed) => ErrorCode::MusicStreamsNotAllowed,
        Ok(proto::ErrorCode::MusicSourceNotAllowed) => ErrorCode::MusicSourceNotAllowed,
        Ok(proto::ErrorCode::MusicLinkNotAllowed) => ErrorCode::MusicLinkNotAllowed,
        Ok(proto::ErrorCode::MusicStale) => ErrorCode::MusicStale,
        Ok(proto::ErrorCode::CommandUnknown) => ErrorCode::CommandUnknown,
        Ok(proto::ErrorCode::CommandDisabled) => ErrorCode::CommandDisabled,
        Ok(proto::ErrorCode::CommandNotAllowedHere) => ErrorCode::CommandNotAllowedHere,
        Ok(proto::ErrorCode::CommandInvalidOptions) => ErrorCode::CommandInvalidOptions,
        Ok(proto::ErrorCode::FileTooLarge) => ErrorCode::FileTooLarge,
        Ok(proto::ErrorCode::FileTypeBlocked) => ErrorCode::FileTypeBlocked,
        Ok(proto::ErrorCode::UploadOffsetMismatch) => ErrorCode::UploadOffsetMismatch,
        Ok(proto::ErrorCode::StorageUnavailable) => ErrorCode::StorageUnavailable,
        Ok(proto::ErrorCode::StorageFull) => ErrorCode::StorageFull,
        Ok(proto::ErrorCode::VoiceMessageInvalid) => ErrorCode::VoiceMessageInvalid,
        Ok(proto::ErrorCode::VoiceMessageTooLong) => ErrorCode::VoiceMessageTooLong,
        Ok(proto::ErrorCode::FriendRequestsClosed) => ErrorCode::FriendRequestsClosed,
        Ok(proto::ErrorCode::MessageRequestsClosed) => ErrorCode::MessageRequestsClosed,
        Ok(proto::ErrorCode::Undeliverable) => ErrorCode::Undeliverable,
        Ok(proto::ErrorCode::DeviceListChanged) => ErrorCode::DeviceListChanged,
        Ok(proto::ErrorCode::PrivateMessagesUnavailable) => ErrorCode::PrivateMessagesUnavailable,

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
    fn streams_reach_the_app_from_ready_and_events() {
        let live = proto::Stream {
            stream_key: "stream:5:8".to_owned(),
            channel_id: 5,
            user_id: 8,
            source_kind: proto::StreamSourceKind::Window as i32,
            resolution: proto::ScreenShareResolution::ScreenShareResolution1080p as i32,
            fps: 60,
            has_audio: true,
            viewer_count: 2,
        };
        let expected = ScreenStream {
            stream_key: "stream:5:8".to_owned(),
            channel_id: 5,
            user_id: 8,
            source_kind: StreamSourceKind::Window,
            resolution: ScreenShareResolution::P1080,
            fps: 60,
            has_audio: true,
            viewer_count: 2,
        };

        let snapshot = super::ready(proto::Ready {
            streams: vec![live.clone()],
            ..Default::default()
        });
        let created = super::event(
            proto::event::Kind::StreamCreate(proto::StreamCreate { stream: Some(live) }),
            "mine",
        );
        let viewers = super::event(
            proto::event::Kind::StreamViewersUpdate(proto::StreamViewersUpdate {
                stream_key: "stream:5:8".to_owned(),
                viewer_ids: vec![3, 4],
            }),
            "mine",
        );

        assert_eq!(snapshot.streams, std::slice::from_ref(&expected));
        assert_eq!(created, Some(CoreEventPayload::StreamCreate(expected)));
        assert_eq!(
            viewers,
            Some(CoreEventPayload::StreamViewersUpdate {
                stream_key: "stream:5:8".to_owned(),
                viewer_ids: vec![3, 4],
            })
        );
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
