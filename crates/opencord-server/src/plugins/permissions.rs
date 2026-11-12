//! What a plugin may do (design 2026-11-07 §8): its permissions, the member
//! permissions its managed role carries for them, and who may grant them.

use std::collections::HashSet;

use opencord_common::permissions::{Permissions, can_grant};

use crate::guild::Guild;

/// One of `opencord_plugins::permissions::PERMISSIONS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluginPermission {
    ReadMessages,
    SendMessages,
    ManageMessages,
    AttachFiles,
    ReadChannels,
    ManageChannels,
    CreateChannelTypes,
    RegisterActivities,
    RegisterCommands,
    ReadMembers,
    ReadRoles,
    ManageRoles,
    ManageNicknames,
    KickMembers,
    BanMembers,
    ReadInvites,
    CreateInvites,
    ReadServer,
    ReadVoice,
    MoveMembers,
    MuteMembers,
    DeafenMembers,
    PlaySoundboard,
    ReadMusic,
    ControlMusic,
    Internet,
    ReceiveWebRequests,
    AcceptUploads,
    TalkToPlugins,
}

impl PluginPermission {
    /// Every plugin permission, in the shared list's order.
    pub const ALL: [Self; 29] = [
        Self::ReadMessages,
        Self::SendMessages,
        Self::ManageMessages,
        Self::AttachFiles,
        Self::ReadChannels,
        Self::ManageChannels,
        Self::CreateChannelTypes,
        Self::RegisterActivities,
        Self::RegisterCommands,
        Self::ReadMembers,
        Self::ReadRoles,
        Self::ManageRoles,
        Self::ManageNicknames,
        Self::KickMembers,
        Self::BanMembers,
        Self::ReadInvites,
        Self::CreateInvites,
        Self::ReadServer,
        Self::ReadVoice,
        Self::MoveMembers,
        Self::MuteMembers,
        Self::DeafenMembers,
        Self::PlaySoundboard,
        Self::ReadMusic,
        Self::ControlMusic,
        Self::Internet,
        Self::ReceiveWebRequests,
        Self::AcceptUploads,
        Self::TalkToPlugins,
    ];

    /// The name manifests and the protocol use: `"send_messages"`.
    pub fn name(self) -> &'static str {
        match self {
            Self::ReadMessages => "read_messages",
            Self::SendMessages => "send_messages",
            Self::ManageMessages => "manage_messages",
            Self::AttachFiles => "attach_files",
            Self::ReadChannels => "read_channels",
            Self::ManageChannels => "manage_channels",
            Self::CreateChannelTypes => "create_channel_types",
            Self::RegisterActivities => "register_activities",
            Self::RegisterCommands => "register_commands",
            Self::ReadMembers => "read_members",
            Self::ReadRoles => "read_roles",
            Self::ManageRoles => "manage_roles",
            Self::ManageNicknames => "manage_nicknames",
            Self::KickMembers => "kick_members",
            Self::BanMembers => "ban_members",
            Self::ReadInvites => "read_invites",
            Self::CreateInvites => "create_invites",
            Self::ReadServer => "read_server",
            Self::ReadVoice => "read_voice",
            Self::MoveMembers => "move_members",
            Self::MuteMembers => "mute_members",
            Self::DeafenMembers => "deafen_members",
            Self::PlaySoundboard => "play_soundboard",
            Self::ReadMusic => "read_music",
            Self::ControlMusic => "control_music",
            Self::Internet => "internet",
            Self::ReceiveWebRequests => "receive_web_requests",
            Self::AcceptUploads => "accept_uploads",
            Self::TalkToPlugins => "talk_to_plugins",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|permission| permission.name() == name)
    }

    /// What the admin granting it must hold (§8.1's last column).
    pub fn admin_needs(self) -> Permissions {
        match self {
            Self::ReadMessages => Permissions::READ_HISTORY,
            Self::SendMessages => Permissions::SEND_MESSAGES,
            Self::ManageMessages => Permissions::MANAGE_MESSAGES,
            Self::AttachFiles | Self::AcceptUploads => Permissions::ATTACH_FILES,
            Self::ManageChannels | Self::CreateChannelTypes => Permissions::MANAGE_CHANNELS,
            Self::RegisterActivities
            | Self::RegisterCommands
            | Self::ReadInvites
            | Self::Internet
            | Self::ReceiveWebRequests
            | Self::TalkToPlugins => Permissions::MANAGE_SERVER,
            Self::ManageRoles => Permissions::MANAGE_ROLES,
            Self::ManageNicknames => Permissions::MANAGE_NICKNAMES,
            Self::KickMembers => Permissions::KICK_MEMBERS,
            Self::BanMembers => Permissions::BAN_MEMBERS,
            Self::CreateInvites => Permissions::CREATE_INVITE,
            Self::MoveMembers => Permissions::MOVE_MEMBERS,
            Self::MuteMembers => Permissions::MUTE_MEMBERS,
            Self::DeafenMembers => Permissions::DEAFEN_MEMBERS,
            Self::PlaySoundboard => Permissions::USE_SOUNDBOARD,
            Self::ControlMusic => Permissions::MUSIC_MANAGE,
            Self::ReadChannels
            | Self::ReadMembers
            | Self::ReadRoles
            | Self::ReadServer
            | Self::ReadVoice
            | Self::ReadMusic => Permissions::empty(),
        }
    }

    /// The member permissions its managed role carries for it.
    pub fn role_bits(self) -> Permissions {
        match self {
            Self::ReadMessages => Permissions::READ_HISTORY,
            Self::SendMessages => Permissions::SEND_MESSAGES,
            Self::ManageMessages => Permissions::MANAGE_MESSAGES,
            Self::AttachFiles => Permissions::ATTACH_FILES,
            Self::ManageChannels => Permissions::MANAGE_CHANNELS,
            Self::ManageRoles => Permissions::MANAGE_ROLES,
            Self::ManageNicknames => Permissions::MANAGE_NICKNAMES,
            Self::KickMembers => Permissions::KICK_MEMBERS,
            Self::BanMembers => Permissions::BAN_MEMBERS,
            Self::ReadInvites => Permissions::MANAGE_SERVER,
            Self::CreateInvites => Permissions::CREATE_INVITE,
            Self::MoveMembers => Permissions::MOVE_MEMBERS,
            Self::MuteMembers => Permissions::MUTE_MEMBERS,
            Self::DeafenMembers => Permissions::DEAFEN_MEMBERS,
            Self::PlaySoundboard => Permissions::USE_SOUNDBOARD,
            Self::ControlMusic => {
                Permissions::MUSIC_ADD
                    | Permissions::MUSIC_CONTROL
                    | Permissions::MUSIC_MANAGE_QUEUE
                    | Permissions::MUSIC_MANAGE
            }
            Self::ReadChannels
            | Self::CreateChannelTypes
            | Self::RegisterActivities
            | Self::RegisterCommands
            | Self::ReadMembers
            | Self::ReadRoles
            | Self::ReadServer
            | Self::ReadVoice
            | Self::ReadMusic
            | Self::Internet
            | Self::ReceiveWebRequests
            | Self::AcceptUploads
            | Self::TalkToPlugins => Permissions::empty(),
        }
    }
}

/// The managed role's permissions for these granted plugin permissions:
/// always `VIEW_CHANNEL`, so a plugin sees the channels @everyone may not.
pub fn role_permissions(granted: &HashSet<PluginPermission>) -> Permissions {
    granted
        .iter()
        .fold(Permissions::VIEW_CHANNEL, |bits, permission| {
            bits | permission.role_bits()
        })
}

/// Whether `admin_id` may switch `permission` on for a plugin: the owner
/// and `ADMINISTRATOR` may grant anything, other members only what they
/// hold `admin_needs` for (§8.3).
pub fn may_grant(guild: &Guild, admin_id: i64, permission: PluginPermission) -> bool {
    guild.members.contains_key(&admin_id)
        && can_grant(guild.base_permissions(admin_id), permission.admin_needs())
}

#[cfg(test)]
mod tests {
    use opencord_plugins::permissions::PERMISSIONS;

    use super::PluginPermission as P;
    use super::*;
    use crate::guild::Role;
    use crate::guild::tests::{ALICE, MOD_ROLE, OUTSIDER, OWNER, sample_guild};

    #[test]
    fn every_name_maps_both_ways() {
        // Catches: a permission added to the shared list that the server
        // can't name (its manifests would be refused), or two variants
        // answering to one name.
        assert_eq!(P::ALL.len(), PERMISSIONS.len());
        for (name, _) in PERMISSIONS {
            let permission = P::from_name(name).unwrap_or_else(|| panic!("{name} isn't known"));
            assert_eq!(permission.name(), *name);
        }
        for permission in P::ALL {
            assert_eq!(P::from_name(permission.name()), Some(permission));
        }
        for unknown in ["", "READ_MESSAGES", "read_message", "administrator"] {
            assert_eq!(P::from_name(unknown), None, "{unknown}");
        }
    }

    #[test]
    fn admin_needs_follow_the_spec() {
        // §8.1's last column, row by row. Catches: a permission an admin
        // could switch on without holding what it lets the plugin do.
        let table = [
            (P::ReadMessages, Permissions::READ_HISTORY),
            (P::SendMessages, Permissions::SEND_MESSAGES),
            (P::ManageMessages, Permissions::MANAGE_MESSAGES),
            (P::AttachFiles, Permissions::ATTACH_FILES),
            (P::ReadChannels, Permissions::empty()),
            (P::ManageChannels, Permissions::MANAGE_CHANNELS),
            (P::CreateChannelTypes, Permissions::MANAGE_CHANNELS),
            (P::RegisterActivities, Permissions::MANAGE_SERVER),
            (P::RegisterCommands, Permissions::MANAGE_SERVER),
            (P::ReadMembers, Permissions::empty()),
            (P::ReadRoles, Permissions::empty()),
            (P::ManageRoles, Permissions::MANAGE_ROLES),
            (P::ManageNicknames, Permissions::MANAGE_NICKNAMES),
            (P::KickMembers, Permissions::KICK_MEMBERS),
            (P::BanMembers, Permissions::BAN_MEMBERS),
            (P::ReadInvites, Permissions::MANAGE_SERVER),
            (P::CreateInvites, Permissions::CREATE_INVITE),
            (P::ReadServer, Permissions::empty()),
            (P::ReadVoice, Permissions::empty()),
            (P::MoveMembers, Permissions::MOVE_MEMBERS),
            (P::MuteMembers, Permissions::MUTE_MEMBERS),
            (P::DeafenMembers, Permissions::DEAFEN_MEMBERS),
            (P::PlaySoundboard, Permissions::USE_SOUNDBOARD),
            (P::ReadMusic, Permissions::empty()),
            (P::ControlMusic, Permissions::MUSIC_MANAGE),
            (P::Internet, Permissions::MANAGE_SERVER),
            (P::ReceiveWebRequests, Permissions::MANAGE_SERVER),
            (P::AcceptUploads, Permissions::ATTACH_FILES),
            (P::TalkToPlugins, Permissions::MANAGE_SERVER),
        ];
        assert_eq!(table.len(), P::ALL.len());
        for (permission, needs) in table {
            assert_eq!(permission.admin_needs(), needs, "{}", permission.name());
        }
    }

    #[test]
    fn role_bits_follow_the_plan() {
        // Catches: a managed role missing the bit a handler checks (the
        // plugin's call fails) or carrying one it wasn't granted.
        let music = Permissions::MUSIC_ADD
            | Permissions::MUSIC_CONTROL
            | Permissions::MUSIC_MANAGE_QUEUE
            | Permissions::MUSIC_MANAGE;
        let table = [
            (P::ReadMessages, Permissions::READ_HISTORY),
            (P::SendMessages, Permissions::SEND_MESSAGES),
            (P::ManageMessages, Permissions::MANAGE_MESSAGES),
            (P::AttachFiles, Permissions::ATTACH_FILES),
            (P::ManageChannels, Permissions::MANAGE_CHANNELS),
            (P::ManageRoles, Permissions::MANAGE_ROLES),
            (P::ManageNicknames, Permissions::MANAGE_NICKNAMES),
            (P::KickMembers, Permissions::KICK_MEMBERS),
            (P::BanMembers, Permissions::BAN_MEMBERS),
            (P::ReadInvites, Permissions::MANAGE_SERVER),
            (P::CreateInvites, Permissions::CREATE_INVITE),
            (P::MoveMembers, Permissions::MOVE_MEMBERS),
            (P::MuteMembers, Permissions::MUTE_MEMBERS),
            (P::DeafenMembers, Permissions::DEAFEN_MEMBERS),
            (P::PlaySoundboard, Permissions::USE_SOUNDBOARD),
            (P::ControlMusic, music),
        ];
        for permission in P::ALL {
            let expected = table
                .iter()
                .find(|(listed, _)| *listed == permission)
                .map_or(Permissions::empty(), |(_, bits)| *bits);
            assert_eq!(permission.role_bits(), expected, "{}", permission.name());
        }
    }

    #[test]
    fn the_role_always_views_channels() {
        // Catches: a plugin granted nothing, or only read_* permissions,
        // whose role sees no channel and so never gets a channel's events.
        assert_eq!(role_permissions(&HashSet::new()), Permissions::VIEW_CHANNEL);
        assert_eq!(
            role_permissions(&[P::ReadChannels, P::ReadMembers, P::Internet].into()),
            Permissions::VIEW_CHANNEL
        );
        assert_eq!(
            role_permissions(&[P::SendMessages, P::KickMembers, P::ControlMusic].into()),
            Permissions::VIEW_CHANNEL
                | Permissions::SEND_MESSAGES
                | Permissions::KICK_MEMBERS
                | Permissions::MUSIC_ADD
                | Permissions::MUSIC_CONTROL
                | Permissions::MUSIC_MANAGE_QUEUE
                | Permissions::MUSIC_MANAGE
        );
    }

    #[tokio::test]
    async fn may_grant_needs_what_the_permission_needs() {
        // Catches: anyone with MANAGE_PLUGINS granting kick_members, ban
        // or internet without holding the matching member permission.
        const ADMIN: i64 = 5;
        const ADMIN_ROLE: i64 = 51;
        let mut guild = sample_guild().await;
        guild.members.get_mut(&ALICE).unwrap().role_ids = vec![MOD_ROLE];
        guild.roles.insert(
            ADMIN_ROLE,
            Role {
                id: ADMIN_ROLE,
                name: "admin".to_owned(),
                color: 0,
                position: 2,
                permissions: Permissions::ADMINISTRATOR,
                hoist: false,
                mentionable: false,
                max_upload_bytes: None,
                plugin_id: None,
            },
        );
        let mut admin = guild.members[&ALICE].clone();
        admin.user.id = ADMIN;
        admin.role_ids = vec![ADMIN_ROLE];
        guild.members.insert(ADMIN, admin);

        for permission in P::ALL {
            assert!(
                may_grant(&guild, OWNER, permission),
                "{}",
                permission.name()
            );
            assert!(
                may_grant(&guild, ADMIN, permission),
                "{}",
                permission.name()
            );
        }
        // The moderator holds KICK_MEMBERS and @everyone's defaults.
        assert!(may_grant(&guild, ALICE, P::KickMembers));
        assert!(may_grant(&guild, ALICE, P::SendMessages));
        assert!(may_grant(&guild, ALICE, P::ReadChannels));
        assert!(!may_grant(&guild, ALICE, P::BanMembers));
        assert!(!may_grant(&guild, ALICE, P::Internet));
        assert!(!may_grant(&guild, ALICE, P::ControlMusic));
        assert!(!may_grant(&guild, ALICE, P::ManageMessages));
        // Someone who isn't a member grants nothing, not even what needs
        // nothing.
        assert!(!may_grant(&guild, OUTSIDER, P::ReadChannels));
    }
}
