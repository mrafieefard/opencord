//! Just enough of a server's state to resolve the user's own permissions
//! with the shared resolver as events arrive.

use std::collections::{BTreeMap, HashMap};

use opencord_common::permissions::{
    MemberContext, Overwrite, OverwriteTarget, Permissions, RoleRef,
};
use opencord_proto::v1 as proto;
use proto::event::Kind;

/// The user's permissions: server-wide, and per visible channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionSnapshot {
    pub server: u64,
    pub channels: BTreeMap<i64, u64>,
}

#[derive(Debug, Clone)]
pub struct GuildMirror {
    self_id: i64,
    owner_id: Option<i64>,
    everyone_role_id: i64,
    roles: HashMap<i64, RoleRef>,
    channels: HashMap<i64, Vec<Overwrite>>,
    own_role_ids: Vec<i64>,
}

impl GuildMirror {
    pub fn from_ready(ready: &proto::Ready) -> Self {
        let self_id = ready.self_user.as_ref().map_or(0, |user| user.id);
        let server = ready.server.clone().unwrap_or_default();
        Self {
            self_id,
            owner_id: server.owner_id,
            everyone_role_id: server.everyone_role_id,
            roles: ready
                .roles
                .iter()
                .map(|role| (role.id, role_ref(role)))
                .collect(),
            channels: ready
                .channels
                .iter()
                .map(|channel| (channel.id, overwrites(channel)))
                .collect(),
            own_role_ids: ready
                .members
                .iter()
                .find(|member| member.user.as_ref().is_some_and(|user| user.id == self_id))
                .map(|member| member.role_ids.clone())
                .unwrap_or_default(),
        }
    }

    /// Applies an event. Returns whether it can change the user's
    /// permissions.
    pub fn apply(&mut self, event: &Kind) -> bool {
        match event {
            Kind::RoleCreate(proto::RoleCreate { role: Some(role) })
            | Kind::RoleUpdate(proto::RoleUpdate { role: Some(role) }) => {
                self.roles.insert(role.id, role_ref(role));
                true
            }
            Kind::RoleDelete(delete) => {
                self.roles.remove(&delete.role_id);
                self.own_role_ids.retain(|id| *id != delete.role_id);
                true
            }
            Kind::ChannelCreate(proto::ChannelCreate {
                channel: Some(channel),
            })
            | Kind::ChannelUpdate(proto::ChannelUpdate {
                channel: Some(channel),
            }) => {
                self.channels.insert(channel.id, overwrites(channel));
                true
            }
            Kind::ChannelDelete(delete) => {
                self.channels.remove(&delete.channel_id);
                true
            }
            Kind::MemberUpdate(proto::MemberUpdate {
                member: Some(member),
            }) if member
                .user
                .as_ref()
                .is_some_and(|user| user.id == self.self_id) =>
            {
                self.own_role_ids.clone_from(&member.role_ids);
                true
            }
            Kind::ServerUpdate(proto::ServerUpdate {
                server: Some(server),
            }) => {
                let changed = server.owner_id != self.owner_id;
                self.owner_id = server.owner_id;
                changed
            }
            _ => false,
        }
    }

    pub fn permissions(&self) -> PermissionSnapshot {
        let roles: Vec<RoleRef> = self
            .own_role_ids
            .iter()
            .filter_map(|role_id| self.roles.get(role_id))
            .copied()
            .collect();
        let context = MemberContext {
            user_id: self.self_id,
            is_owner: self.owner_id == Some(self.self_id),
            everyone_role_id: self.everyone_role_id,
            everyone: self
                .roles
                .get(&self.everyone_role_id)
                .map_or(Permissions::empty(), |role| role.permissions),
            roles: &roles,
        };
        PermissionSnapshot {
            server: context.base_permissions().bits(),
            channels: self
                .channels
                .iter()
                .map(|(channel_id, overwrites)| {
                    (*channel_id, context.channel_permissions(overwrites).bits())
                })
                .collect(),
        }
    }
}

fn role_ref(role: &proto::Role) -> RoleRef {
    RoleRef {
        id: role.id,
        position: role.position,
        permissions: Permissions::from_bits_truncate(role.permissions),
    }
}

fn overwrites(channel: &proto::Channel) -> Vec<Overwrite> {
    channel
        .overwrites
        .iter()
        .filter_map(|overwrite| {
            let target = match proto::OverwriteTarget::try_from(overwrite.target_kind) {
                Ok(proto::OverwriteTarget::Role) => OverwriteTarget::Role(overwrite.target_id),
                Ok(proto::OverwriteTarget::Member) => OverwriteTarget::Member(overwrite.target_id),
                _ => return None,
            };
            Some(Overwrite {
                target,
                allow: Permissions::from_bits_truncate(overwrite.allow),
                deny: Permissions::from_bits_truncate(overwrite.deny),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: i64 = 10;
    const OTHER: i64 = 11;
    const EVERYONE: i64 = 1;
    const MOD: i64 = 2;
    const GENERAL: i64 = 100;

    fn role(id: i64, position: i32, permissions: Permissions) -> proto::Role {
        proto::Role {
            id,
            position,
            permissions: permissions.bits(),
            ..Default::default()
        }
    }

    fn ready(owner: Option<i64>) -> proto::Ready {
        proto::Ready {
            self_user: Some(proto::User {
                id: ME,
                ..Default::default()
            }),
            server: Some(proto::ServerInfo {
                owner_id: owner,
                everyone_role_id: EVERYONE,
                ..Default::default()
            }),
            channels: vec![proto::Channel {
                id: GENERAL,
                ..Default::default()
            }],
            roles: vec![
                role(EVERYONE, 0, Permissions::DEFAULT_EVERYONE),
                role(MOD, 1, Permissions::MANAGE_MESSAGES),
            ],
            members: vec![proto::Member {
                user: Some(proto::User {
                    id: ME,
                    ..Default::default()
                }),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn me_with_roles(role_ids: Vec<i64>) -> Kind {
        Kind::MemberUpdate(proto::MemberUpdate {
            member: Some(proto::Member {
                user: Some(proto::User {
                    id: ME,
                    ..Default::default()
                }),
                role_ids,
                ..Default::default()
            }),
        })
    }

    fn general(perms: &PermissionSnapshot) -> Permissions {
        Permissions::from_bits_truncate(perms.channels[&GENERAL])
    }

    #[test]
    fn starts_from_ready() {
        let mirror = GuildMirror::from_ready(&ready(None));

        let perms = mirror.permissions();

        assert_eq!(perms.server, Permissions::DEFAULT_EVERYONE.bits());
        assert_eq!(general(&perms), Permissions::DEFAULT_EVERYONE);
    }

    #[test]
    fn owners_have_everything() {
        let mirror = GuildMirror::from_ready(&ready(Some(ME)));

        assert_eq!(mirror.permissions().server, Permissions::all().bits());
    }

    #[test]
    fn gaining_a_role_adds_its_permissions() {
        let mut mirror = GuildMirror::from_ready(&ready(None));

        let relevant = mirror.apply(&me_with_roles(vec![MOD]));

        assert!(relevant);
        assert!(general(&mirror.permissions()).contains(Permissions::MANAGE_MESSAGES));
    }

    #[test]
    fn other_members_changing_is_irrelevant() {
        let mut mirror = GuildMirror::from_ready(&ready(None));
        let other = Kind::MemberUpdate(proto::MemberUpdate {
            member: Some(proto::Member {
                user: Some(proto::User {
                    id: OTHER,
                    ..Default::default()
                }),
                role_ids: vec![MOD],
                ..Default::default()
            }),
        });

        assert!(!mirror.apply(&other));
        assert!(!mirror.apply(&Kind::TypingStart(proto::TypingStart::default())));
    }

    #[test]
    fn role_updates_and_deletes_apply() {
        let mut mirror = GuildMirror::from_ready(&ready(None));
        mirror.apply(&me_with_roles(vec![MOD]));

        mirror.apply(&Kind::RoleUpdate(proto::RoleUpdate {
            role: Some(role(MOD, 1, Permissions::KICK_MEMBERS)),
        }));
        let updated = mirror.permissions();
        mirror.apply(&Kind::RoleDelete(proto::RoleDelete { role_id: MOD }));
        let deleted = mirror.permissions();

        assert!(
            Permissions::from_bits_truncate(updated.server).contains(Permissions::KICK_MEMBERS)
        );
        assert!(
            !Permissions::from_bits_truncate(deleted.server).contains(Permissions::KICK_MEMBERS)
        );
    }

    #[test]
    fn overwrites_and_visibility_follow_channel_events() {
        let mut mirror = GuildMirror::from_ready(&ready(None));
        let muted = proto::Channel {
            id: GENERAL,
            overwrites: vec![proto::PermissionOverwrite {
                target_kind: proto::OverwriteTarget::Role as i32,
                target_id: EVERYONE,
                allow: 0,
                deny: Permissions::SEND_MESSAGES.bits(),
            }],
            ..Default::default()
        };

        mirror.apply(&Kind::ChannelUpdate(proto::ChannelUpdate {
            channel: Some(muted),
        }));
        let muted_perms = mirror.permissions();
        mirror.apply(&Kind::ChannelDelete(proto::ChannelDelete {
            channel_id: GENERAL,
        }));

        assert!(!general(&muted_perms).contains(Permissions::SEND_MESSAGES));
        assert!(mirror.permissions().channels.is_empty());
    }

    #[test]
    fn ownership_changes_apply() {
        let mut mirror = GuildMirror::from_ready(&ready(None));

        mirror.apply(&Kind::ServerUpdate(proto::ServerUpdate {
            server: Some(proto::ServerInfo {
                owner_id: Some(ME),
                everyone_role_id: EVERYONE,
                ..Default::default()
            }),
        }));

        assert_eq!(mirror.permissions().server, Permissions::all().bits());
    }
}
