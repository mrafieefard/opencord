//! Conversions between protobuf messages and the Dart-facing types.

use opencord_proto::v1 as proto;

use crate::api::types::{
    Ban, Channel, ChannelKind, ChannelPermissions, CoreError, CoreEventPayload, ErrorCode, Invite,
    Member, Message, OverwriteTargetKind, PermissionOverwrite, Presence, PresenceStatus,
    ReadySnapshot, Role, ServerInfo, User,
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
    ReadySnapshot {
        self_user: user(ready.self_user.unwrap_or_default()),
        server: server_info(ready.server.unwrap_or_default()),
        channels: ready.channels.into_iter().map(channel).collect(),
        roles: ready.roles.into_iter().map(role).collect(),
        members: ready.members.into_iter().map(member).collect(),
        presences: ready.presences.into_iter().map(presence).collect(),
        server_permissions: bits_to_api(ready.server_permissions),
        channel_permissions: channel_permissions(ready.channel_permissions),
    }
}

/// `None` for events with a missing body.
pub fn event(kind: proto::event::Kind) -> Option<CoreEventPayload> {
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
