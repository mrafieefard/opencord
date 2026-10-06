//! Permission bits, the resolution algorithm and role hierarchy rules.

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct Permissions: u64 {
        const VIEW_CHANNEL = 1 << 0;
        const SEND_MESSAGES = 1 << 1;
        const READ_HISTORY = 1 << 2;
        const MANAGE_MESSAGES = 1 << 3;
        const MANAGE_CHANNELS = 1 << 4;
        const MANAGE_ROLES = 1 << 5;
        const KICK_MEMBERS = 1 << 6;
        const BAN_MEMBERS = 1 << 7;
        const CREATE_INVITE = 1 << 8;
        const MANAGE_SERVER = 1 << 9;
        /// Reserved for attachments.
        const ATTACH_FILES = 1 << 10;
        const MENTION_EVERYONE = 1 << 11;
        const CHANGE_NICKNAME = 1 << 12;
        const MANAGE_NICKNAMES = 1 << 13;
        /// Reserved for voice.
        const CONNECT = 1 << 16;
        /// Reserved for voice.
        const SPEAK = 1 << 17;
        /// Reserved for voice.
        const VIDEO = 1 << 18;
        /// Reserved for voice.
        const SCREENSHARE = 1 << 19;
        /// Reserved for voice.
        const MUTE_MEMBERS = 1 << 20;
        /// Reserved for voice.
        const DEAFEN_MEMBERS = 1 << 21;
        /// Reserved for voice.
        const MOVE_MEMBERS = 1 << 22;
        /// Reserved for voice.
        const PRIORITY_SPEAKER = 1 << 23;
        const ADMINISTRATOR = 1 << 63;
    }
}

impl Permissions {
    /// Permissions of @everyone on a freshly created server.
    pub const DEFAULT_EVERYONE: Self = Self::VIEW_CHANNEL
        .union(Self::SEND_MESSAGES)
        .union(Self::READ_HISTORY)
        .union(Self::CREATE_INVITE)
        .union(Self::CHANGE_NICKNAME)
        .union(Self::CONNECT)
        .union(Self::SPEAK)
        .union(Self::VIDEO)
        .union(Self::SCREENSHARE);
}

/// A role as far as permission checks are concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoleRef {
    pub id: i64,
    pub position: i32,
    pub permissions: Permissions,
}

/// Everything about a member needed to resolve their permissions.
#[derive(Debug, Clone, Copy)]
pub struct MemberContext<'a> {
    pub user_id: i64,
    pub is_owner: bool,
    pub everyone_role_id: i64,
    /// Permissions of the @everyone role.
    pub everyone: Permissions,
    /// The member's roles, without @everyone.
    pub roles: &'a [RoleRef],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OverwriteTarget {
    Role(i64),
    Member(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Overwrite {
    pub target: OverwriteTarget,
    pub allow: Permissions,
    pub deny: Permissions,
}

impl MemberContext<'_> {
    /// Permissions before channel overwrites.
    pub fn base_permissions(&self) -> Permissions {
        if self.is_owner {
            return Permissions::all();
        }
        let base = self
            .roles
            .iter()
            .fold(self.everyone, |acc, role| acc | role.permissions);
        if base.contains(Permissions::ADMINISTRATOR) {
            Permissions::all()
        } else {
            base
        }
    }

    /// Permissions in a channel with the given overwrites.
    pub fn channel_permissions(&self, overwrites: &[Overwrite]) -> Permissions {
        let base = self.base_permissions();
        if base.contains(Permissions::ADMINISTRATOR) {
            return Permissions::all();
        }

        let everyone_target = OverwriteTarget::Role(self.everyone_role_id);
        let after_everyone = overwrites
            .iter()
            .find(|overwrite| overwrite.target == everyone_target)
            .map_or(base, |overwrite| {
                apply(base, overwrite.allow, overwrite.deny)
            });

        let (role_allow, role_deny) = overwrites
            .iter()
            .filter(|overwrite| match overwrite.target {
                OverwriteTarget::Role(id) => id != self.everyone_role_id && self.has_role(id),
                OverwriteTarget::Member(_) => false,
            })
            .fold(
                (Permissions::empty(), Permissions::empty()),
                |(allow, deny), overwrite| (allow | overwrite.allow, deny | overwrite.deny),
            );
        let after_roles = apply(after_everyone, role_allow, role_deny);

        let member_target = OverwriteTarget::Member(self.user_id);
        let resolved = overwrites
            .iter()
            .find(|overwrite| overwrite.target == member_target)
            .map_or(after_roles, |overwrite| {
                apply(after_roles, overwrite.allow, overwrite.deny)
            });

        if resolved.contains(Permissions::VIEW_CHANNEL) {
            resolved
        } else {
            Permissions::empty()
        }
    }

    /// Position of the member's highest role; 0 (@everyone) without roles.
    pub fn highest_position(&self) -> i32 {
        self.roles
            .iter()
            .map(|role| role.position)
            .max()
            .unwrap_or(0)
    }

    /// Whether this member may kick, ban or edit a member whose highest
    /// role is at `target_highest`.
    pub fn outranks_member(&self, target_is_owner: bool, target_highest: i32) -> bool {
        if target_is_owner {
            return false;
        }
        self.is_owner || self.highest_position() > target_highest
    }

    /// Whether this member may edit, assign or remove a role at `position`.
    pub fn outranks_role(&self, position: i32) -> bool {
        self.is_owner || position < self.highest_position()
    }

    fn has_role(&self, role_id: i64) -> bool {
        self.roles.iter().any(|role| role.id == role_id)
    }
}

/// Whether someone holding `held` may grant (or deny) `requested` to others.
pub fn can_grant(held: Permissions, requested: Permissions) -> bool {
    held.contains(Permissions::ADMINISTRATOR) || held.contains(requested)
}

fn apply(permissions: Permissions, allow: Permissions, deny: Permissions) -> Permissions {
    permissions.difference(deny).union(allow)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVERYONE_ID: i64 = 1;
    const USER_ID: i64 = 100;
    const MOD_ROLE: i64 = 10;
    const MEMBER_ROLE: i64 = 11;

    fn role(id: i64, position: i32, permissions: Permissions) -> RoleRef {
        RoleRef {
            id,
            position,
            permissions,
        }
    }

    fn member<'a>(everyone: Permissions, roles: &'a [RoleRef]) -> MemberContext<'a> {
        MemberContext {
            user_id: USER_ID,
            is_owner: false,
            everyone_role_id: EVERYONE_ID,
            everyone,
            roles,
        }
    }

    fn overwrite(target: OverwriteTarget, allow: Permissions, deny: Permissions) -> Overwrite {
        Overwrite {
            target,
            allow,
            deny,
        }
    }

    #[test]
    fn bits_match_the_protocol() {
        assert_eq!(Permissions::VIEW_CHANNEL.bits(), 1);
        assert_eq!(Permissions::MANAGE_NICKNAMES.bits(), 1 << 13);
        assert_eq!(Permissions::CONNECT.bits(), 1 << 16);
        assert_eq!(Permissions::PRIORITY_SPEAKER.bits(), 1 << 23);
        assert_eq!(Permissions::ADMINISTRATOR.bits(), 1 << 63);
    }

    #[test]
    fn owner_has_everything_despite_overwrites() {
        let ctx = MemberContext {
            is_owner: true,
            ..member(Permissions::empty(), &[])
        };
        let deny_all = [overwrite(
            OverwriteTarget::Member(USER_ID),
            Permissions::empty(),
            Permissions::all(),
        )];

        assert_eq!(ctx.base_permissions(), Permissions::all());
        assert_eq!(ctx.channel_permissions(&deny_all), Permissions::all());
    }

    #[test]
    fn base_combines_everyone_and_member_roles() {
        let roles = [
            role(MOD_ROLE, 2, Permissions::KICK_MEMBERS),
            role(MEMBER_ROLE, 1, Permissions::CREATE_INVITE),
        ];
        let ctx = member(Permissions::VIEW_CHANNEL, &roles);

        assert_eq!(
            ctx.base_permissions(),
            Permissions::VIEW_CHANNEL | Permissions::KICK_MEMBERS | Permissions::CREATE_INVITE
        );
    }

    #[test]
    fn administrator_has_everything_despite_overwrites() {
        let roles = [role(MOD_ROLE, 1, Permissions::ADMINISTRATOR)];
        let ctx = member(Permissions::empty(), &roles);
        let deny_everyone = [overwrite(
            OverwriteTarget::Role(EVERYONE_ID),
            Permissions::empty(),
            Permissions::all(),
        )];

        assert_eq!(ctx.base_permissions(), Permissions::all());
        assert_eq!(ctx.channel_permissions(&deny_everyone), Permissions::all());
    }

    #[test]
    fn everyone_overwrite_applies_to_all_members() {
        let ctx = member(Permissions::DEFAULT_EVERYONE, &[]);
        let overwrites = [overwrite(
            OverwriteTarget::Role(EVERYONE_ID),
            Permissions::MANAGE_MESSAGES,
            Permissions::SEND_MESSAGES,
        )];

        let resolved = ctx.channel_permissions(&overwrites);

        assert!(!resolved.contains(Permissions::SEND_MESSAGES));
        assert!(resolved.contains(Permissions::MANAGE_MESSAGES));
        assert!(resolved.contains(Permissions::VIEW_CHANNEL));
    }

    #[test]
    fn role_overwrites_are_aggregated_and_allow_wins_over_deny() {
        let roles = [
            role(MOD_ROLE, 2, Permissions::empty()),
            role(MEMBER_ROLE, 1, Permissions::empty()),
        ];
        let ctx = member(Permissions::DEFAULT_EVERYONE, &roles);
        let overwrites = [
            overwrite(
                OverwriteTarget::Role(MEMBER_ROLE),
                Permissions::empty(),
                Permissions::SEND_MESSAGES | Permissions::READ_HISTORY,
            ),
            overwrite(
                OverwriteTarget::Role(MOD_ROLE),
                Permissions::SEND_MESSAGES,
                Permissions::empty(),
            ),
        ];

        let resolved = ctx.channel_permissions(&overwrites);

        assert!(resolved.contains(Permissions::SEND_MESSAGES));
        assert!(!resolved.contains(Permissions::READ_HISTORY));
    }

    #[test]
    fn role_overwrites_beat_the_everyone_overwrite() {
        let roles = [role(MEMBER_ROLE, 1, Permissions::empty())];
        let ctx = member(Permissions::DEFAULT_EVERYONE, &roles);
        let overwrites = [
            overwrite(
                OverwriteTarget::Role(EVERYONE_ID),
                Permissions::empty(),
                Permissions::VIEW_CHANNEL,
            ),
            overwrite(
                OverwriteTarget::Role(MEMBER_ROLE),
                Permissions::VIEW_CHANNEL,
                Permissions::empty(),
            ),
        ];

        assert!(
            ctx.channel_permissions(&overwrites)
                .contains(Permissions::VIEW_CHANNEL)
        );
    }

    #[test]
    fn overwrites_for_roles_the_member_lacks_are_ignored() {
        let ctx = member(Permissions::DEFAULT_EVERYONE, &[]);
        let overwrites = [overwrite(
            OverwriteTarget::Role(MOD_ROLE),
            Permissions::empty(),
            Permissions::SEND_MESSAGES,
        )];

        assert!(
            ctx.channel_permissions(&overwrites)
                .contains(Permissions::SEND_MESSAGES)
        );
    }

    #[test]
    fn member_overwrite_beats_role_overwrites() {
        let roles = [role(MEMBER_ROLE, 1, Permissions::empty())];
        let ctx = member(Permissions::DEFAULT_EVERYONE, &roles);
        let overwrites = [
            overwrite(
                OverwriteTarget::Role(MEMBER_ROLE),
                Permissions::MANAGE_MESSAGES,
                Permissions::empty(),
            ),
            overwrite(
                OverwriteTarget::Member(USER_ID),
                Permissions::empty(),
                Permissions::MANAGE_MESSAGES | Permissions::SEND_MESSAGES,
            ),
            overwrite(
                OverwriteTarget::Member(USER_ID + 1),
                Permissions::empty(),
                Permissions::VIEW_CHANNEL,
            ),
        ];

        let resolved = ctx.channel_permissions(&overwrites);

        assert!(!resolved.contains(Permissions::MANAGE_MESSAGES));
        assert!(!resolved.contains(Permissions::SEND_MESSAGES));
        assert!(resolved.contains(Permissions::VIEW_CHANNEL));
    }

    #[test]
    fn missing_view_channel_removes_every_permission() {
        let roles = [role(MOD_ROLE, 1, Permissions::MANAGE_MESSAGES)];
        let ctx = member(Permissions::DEFAULT_EVERYONE, &roles);
        let overwrites = [overwrite(
            OverwriteTarget::Member(USER_ID),
            Permissions::empty(),
            Permissions::VIEW_CHANNEL,
        )];

        assert_eq!(ctx.channel_permissions(&overwrites), Permissions::empty());
    }

    #[test]
    fn highest_position_is_zero_without_roles() {
        assert_eq!(member(Permissions::empty(), &[]).highest_position(), 0);

        let roles = [
            role(MEMBER_ROLE, 3, Permissions::empty()),
            role(MOD_ROLE, 7, Permissions::empty()),
        ];
        assert_eq!(member(Permissions::empty(), &roles).highest_position(), 7);
    }

    #[test]
    fn members_only_outrank_strictly_lower_members() {
        let roles = [role(MOD_ROLE, 5, Permissions::empty())];
        let ctx = member(Permissions::empty(), &roles);

        assert!(ctx.outranks_member(false, 4));
        assert!(!ctx.outranks_member(false, 5));
        assert!(!ctx.outranks_member(false, 6));
        assert!(!ctx.outranks_member(true, 0));
    }

    #[test]
    fn owner_outranks_everyone_but_another_owner() {
        let ctx = MemberContext {
            is_owner: true,
            ..member(Permissions::empty(), &[])
        };

        assert!(ctx.outranks_member(false, 200));
        assert!(!ctx.outranks_member(true, 0));
        assert!(ctx.outranks_role(200));
    }

    #[test]
    fn members_only_manage_roles_below_their_highest() {
        let roles = [role(MOD_ROLE, 5, Permissions::empty())];
        let ctx = member(Permissions::empty(), &roles);

        assert!(ctx.outranks_role(4));
        assert!(!ctx.outranks_role(5));
        assert!(!ctx.outranks_role(9));
    }

    #[test]
    fn can_only_grant_held_permissions() {
        let held = Permissions::SEND_MESSAGES | Permissions::KICK_MEMBERS;

        assert!(can_grant(held, Permissions::SEND_MESSAGES));
        assert!(can_grant(held, Permissions::empty()));
        assert!(!can_grant(
            held,
            Permissions::SEND_MESSAGES | Permissions::BAN_MEMBERS
        ));
        assert!(can_grant(Permissions::ADMINISTRATOR, Permissions::all()));
    }
}
