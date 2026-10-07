//! In-memory copy of the server's roles, channels and members, used for
//! permission checks and event fan-out without touching the database.
//! Every change is written to the database first, then applied here.

use std::collections::{HashMap, HashSet};

use opencord_common::channel::ChannelKind;
use opencord_common::permissions::{
    MemberContext, Overwrite, OverwriteTarget, Permissions, RoleRef,
};
use opencord_proto::v1 as proto;
use sqlx::SqliteConnection;

use crate::db::channels::{ChannelRow, OverwriteRow};
use crate::db::meta::ServerMeta;
use crate::db::roles::RoleRow;
use crate::db::{channels, members, permissions_from_db, permissions_to_db, roles};
use crate::voice::settings::VoiceSettings;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: i64,
    pub public_key: Vec<u8>,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub user: User,
    pub nickname: Option<String>,
    /// Without @everyone.
    pub role_ids: Vec<i64>,
    pub joined_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Role {
    pub id: i64,
    pub name: String,
    pub color: u32,
    pub position: i32,
    pub permissions: Permissions,
    pub hoist: bool,
    pub mentionable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    pub id: i64,
    pub kind: ChannelKind,
    pub name: String,
    pub topic: Option<String>,
    pub parent_id: Option<i64>,
    pub position: i32,
    pub overwrites: Vec<Overwrite>,
    /// Voice channels only: bits per second, before the server's cap.
    pub bitrate: u32,
    /// Voice channels only; 0 means no limit.
    pub user_limit: u32,
    /// Voice channels only.
    pub text_in_voice: bool,
}

#[derive(Debug, Clone)]
pub struct Guild {
    pub meta: ServerMeta,
    pub voice_settings: VoiceSettings,
    pub roles: HashMap<i64, Role>,
    pub channels: HashMap<i64, Channel>,
    pub members: HashMap<i64, Member>,
}

#[derive(Debug, thiserror::Error)]
pub enum GuildError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("stored {what} {id} is invalid")]
    Corrupt { what: &'static str, id: i64 },
    #[error(transparent)]
    Meta(#[from] crate::db::meta::MetaError),
}

impl Guild {
    pub async fn load(conn: &mut SqliteConnection, meta: ServerMeta) -> Result<Self, GuildError> {
        let roles = roles::list(conn)
            .await?
            .into_iter()
            .map(|row| {
                let role = Role {
                    id: row.id,
                    name: row.name,
                    color: u32::try_from(row.color).map_err(|_| corrupt("role", row.id))?,
                    position: i32::try_from(row.position).map_err(|_| corrupt("role", row.id))?,
                    permissions: permissions_from_db(row.permissions),
                    hoist: row.hoist,
                    mentionable: row.mentionable,
                };
                Ok((role.id, role))
            })
            .collect::<Result<HashMap<_, _>, GuildError>>()?;

        let mut overwrites: HashMap<i64, Vec<Overwrite>> = HashMap::new();
        for row in channels::list_overwrites(conn).await? {
            let target = match row.target_kind.as_str() {
                "role" => OverwriteTarget::Role(row.target_id),
                "member" => OverwriteTarget::Member(row.target_id),
                _ => return Err(corrupt("overwrite on channel", row.channel_id)),
            };
            overwrites
                .entry(row.channel_id)
                .or_default()
                .push(Overwrite {
                    target,
                    allow: permissions_from_db(row.allow),
                    deny: permissions_from_db(row.deny),
                });
        }
        let channels = channels::list(conn)
            .await?
            .into_iter()
            .map(|row| {
                let channel = Channel {
                    id: row.id,
                    kind: ChannelKind::parse(&row.kind)
                        .ok_or_else(|| corrupt("channel", row.id))?,
                    name: row.name,
                    topic: row.topic,
                    parent_id: row.parent_id,
                    position: i32::try_from(row.position)
                        .map_err(|_| corrupt("channel", row.id))?,
                    overwrites: overwrites.remove(&row.id).unwrap_or_default(),
                    bitrate: u32::try_from(row.bitrate).map_err(|_| corrupt("channel", row.id))?,
                    user_limit: u32::try_from(row.user_limit)
                        .map_err(|_| corrupt("channel", row.id))?,
                    text_in_voice: row.text_in_voice,
                };
                Ok((channel.id, channel))
            })
            .collect::<Result<HashMap<_, _>, GuildError>>()?;

        let mut role_ids: HashMap<i64, Vec<i64>> = HashMap::new();
        for assignment in members::list_role_assignments(conn).await? {
            role_ids
                .entry(assignment.user_id)
                .or_default()
                .push(assignment.role_id);
        }
        let members = members::list(conn)
            .await?
            .into_iter()
            .map(|row| {
                let member = Member {
                    user: User {
                        id: row.user_id,
                        public_key: row.public_key,
                        display_name: row.display_name,
                    },
                    nickname: row.nickname,
                    role_ids: role_ids.remove(&row.user_id).unwrap_or_default(),
                    joined_at: row.joined_at,
                };
                (member.user.id, member)
            })
            .collect();

        Ok(Self {
            meta,
            voice_settings: VoiceSettings::load(conn).await?,
            roles,
            channels,
            members,
        })
    }

    pub fn is_owner(&self, user_id: i64) -> bool {
        self.meta.owner_id == Some(user_id)
    }

    pub fn everyone_role(&self) -> Option<&Role> {
        self.roles.get(&self.meta.everyone_role_id)
    }

    /// Server-wide permissions; empty for non-members.
    pub fn base_permissions(&self, user_id: i64) -> Permissions {
        self.with_member_context(user_id, |context| context.base_permissions())
            .unwrap_or_else(Permissions::empty)
    }

    /// Permissions in a channel; empty for non-members and unknown channels.
    pub fn channel_permissions(&self, user_id: i64, channel_id: i64) -> Permissions {
        let Some(channel) = self.channels.get(&channel_id) else {
            return Permissions::empty();
        };
        self.with_member_context(user_id, |context| {
            context.channel_permissions(&channel.overwrites)
        })
        .unwrap_or_else(Permissions::empty)
    }

    pub fn can_view(&self, user_id: i64, channel_id: i64) -> bool {
        self.channel_permissions(user_id, channel_id)
            .contains(Permissions::VIEW_CHANNEL)
    }

    pub fn visible_channel_ids(&self, user_id: i64) -> HashSet<i64> {
        self.channels
            .keys()
            .copied()
            .filter(|channel_id| self.can_view(user_id, *channel_id))
            .collect()
    }

    /// Position of the member's highest role (0 without roles).
    pub fn highest_position(&self, user_id: i64) -> i32 {
        self.with_member_context(user_id, |context| context.highest_position())
            .unwrap_or(0)
    }

    /// Runs `f` with the member's permission context.
    pub fn with_member_context<R>(
        &self,
        user_id: i64,
        f: impl FnOnce(&MemberContext<'_>) -> R,
    ) -> Option<R> {
        let member = self.members.get(&user_id)?;
        let roles: Vec<RoleRef> = member
            .role_ids
            .iter()
            .filter_map(|role_id| self.roles.get(role_id))
            .map(|role| RoleRef {
                id: role.id,
                position: role.position,
                permissions: role.permissions,
            })
            .collect();
        let context = MemberContext {
            user_id,
            is_owner: self.is_owner(user_id),
            everyone_role_id: self.meta.everyone_role_id,
            everyone: self
                .everyone_role()
                .map_or(Permissions::empty(), |role| role.permissions),
            roles: &roles,
        };
        Some(f(&context))
    }

    pub fn server_info(&self) -> proto::ServerInfo {
        proto::ServerInfo {
            server_id: self.meta.server_id.to_vec(),
            name: self.meta.name.clone(),
            description: self.meta.description.clone(),
            owner_id: self.meta.owner_id,
            open_join: self.meta.open_join,
            everyone_role_id: self.meta.everyone_role_id,
        }
    }
}

impl User {
    pub fn to_proto(&self) -> proto::User {
        proto::User {
            id: self.id,
            public_key: self.public_key.clone(),
            display_name: self.display_name.clone(),
        }
    }
}

impl Member {
    pub fn to_proto(&self) -> proto::Member {
        proto::Member {
            user: Some(self.user.to_proto()),
            nickname: self.nickname.clone(),
            role_ids: self.role_ids.clone(),
            joined_at_ms: self.joined_at,
        }
    }
}

impl Role {
    pub fn to_proto(&self) -> proto::Role {
        proto::Role {
            id: self.id,
            name: self.name.clone(),
            color: self.color,
            position: self.position,
            permissions: self.permissions.bits(),
            hoist: self.hoist,
            mentionable: self.mentionable,
        }
    }
}

impl Role {
    pub fn to_row(&self) -> RoleRow {
        RoleRow {
            id: self.id,
            name: self.name.clone(),
            color: i64::from(self.color),
            position: i64::from(self.position),
            permissions: permissions_to_db(self.permissions),
            hoist: self.hoist,
            mentionable: self.mentionable,
        }
    }
}

impl Channel {
    pub fn to_row(&self) -> ChannelRow {
        ChannelRow {
            id: self.id,
            kind: self.kind.as_str().to_owned(),
            name: self.name.clone(),
            topic: self.topic.clone(),
            parent_id: self.parent_id,
            position: i64::from(self.position),
            bitrate: i64::from(self.bitrate),
            user_limit: i64::from(self.user_limit),
            text_in_voice: self.text_in_voice,
        }
    }

    pub fn to_proto(&self) -> proto::Channel {
        proto::Channel {
            id: self.id,
            kind: channel_kind_to_proto(self.kind) as i32,
            name: self.name.clone(),
            topic: self.topic.clone(),
            parent_id: self.parent_id,
            position: self.position,
            overwrites: self.overwrites.iter().map(overwrite_to_proto).collect(),
            bitrate: self.voice(self.bitrate),
            user_limit: self.voice(self.user_limit),
            text_in_voice: self.kind == ChannelKind::Voice && self.text_in_voice,
        }
    }

    /// `value` for voice channels, 0 for others.
    fn voice(&self, value: u32) -> u32 {
        if self.kind == ChannelKind::Voice {
            value
        } else {
            0
        }
    }
}

pub fn channel_kind_to_proto(kind: ChannelKind) -> proto::ChannelKind {
    match kind {
        ChannelKind::Text => proto::ChannelKind::Text,
        ChannelKind::Voice => proto::ChannelKind::Voice,
        ChannelKind::Category => proto::ChannelKind::Category,
    }
}

pub fn overwrite_to_proto(overwrite: &Overwrite) -> proto::PermissionOverwrite {
    let (target_kind, target_id) = match overwrite.target {
        OverwriteTarget::Role(id) => (proto::OverwriteTarget::Role, id),
        OverwriteTarget::Member(id) => (proto::OverwriteTarget::Member, id),
    };
    proto::PermissionOverwrite {
        target_kind: target_kind as i32,
        target_id,
        allow: overwrite.allow.bits(),
        deny: overwrite.deny.bits(),
    }
}

/// The stored form of an overwrite on `channel_id`.
pub fn overwrite_row(channel_id: i64, overwrite: &Overwrite) -> OverwriteRow {
    let (target_kind, target_id) = overwrite_target_to_db(overwrite.target);
    OverwriteRow {
        channel_id,
        target_kind: target_kind.to_owned(),
        target_id,
        allow: permissions_to_db(overwrite.allow),
        deny: permissions_to_db(overwrite.deny),
    }
}

pub fn overwrite_target_to_db(target: OverwriteTarget) -> (&'static str, i64) {
    match target {
        OverwriteTarget::Role(id) => ("role", id),
        OverwriteTarget::Member(id) => ("member", id),
    }
}

fn corrupt(what: &'static str, id: i64) -> GuildError {
    GuildError::Corrupt { what, id }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::bootstrap;
    use crate::db::test_pool;
    use opencord_common::snowflake::SnowflakeGenerator;

    const OWNER: i64 = 1;
    const ALICE: i64 = 2;
    const OUTSIDER: i64 = 3;
    const MOD_ROLE: i64 = 50;

    fn user(id: i64) -> User {
        User {
            id,
            public_key: vec![u8::try_from(id).unwrap(); 32],
            display_name: format!("user{id}"),
        }
    }

    fn member(id: i64, role_ids: Vec<i64>) -> Member {
        Member {
            user: user(id),
            nickname: None,
            role_ids,
            joined_at: 0,
        }
    }

    /// Bootstrapped guild with an owner, a plain member (ALICE) and a
    /// moderator role at position 1.
    pub(crate) async fn sample_guild() -> Guild {
        let (pool, _dir) = test_pool().await;
        let ids = SnowflakeGenerator::new(0).unwrap();
        let bootstrap = bootstrap::initialize(&pool, "Test", &ids, 0).await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let mut guild = Guild::load(&mut conn, bootstrap.meta).await.unwrap();
        guild.meta.owner_id = Some(OWNER);
        guild.roles.insert(
            MOD_ROLE,
            Role {
                id: MOD_ROLE,
                name: "mod".to_owned(),
                color: 0,
                position: 1,
                permissions: Permissions::KICK_MEMBERS,
                hoist: true,
                mentionable: false,
            },
        );
        guild.members.insert(OWNER, member(OWNER, vec![]));
        guild.members.insert(ALICE, member(ALICE, vec![]));
        guild
    }

    fn channel_named(guild: &Guild, name: &str) -> i64 {
        guild
            .channels
            .values()
            .find(|channel| channel.name == name)
            .unwrap()
            .id
    }

    #[tokio::test]
    async fn loads_the_bootstrapped_server() {
        let guild = sample_guild().await;

        assert_eq!(
            guild.everyone_role().unwrap().permissions,
            Permissions::DEFAULT_EVERYONE
        );
        assert_eq!(guild.channels.len(), 2);
    }

    #[tokio::test]
    async fn members_get_everyone_and_role_permissions() {
        let mut guild = sample_guild().await;
        guild.members.get_mut(&ALICE).unwrap().role_ids = vec![MOD_ROLE];

        let permissions = guild.base_permissions(ALICE);

        assert!(permissions.contains(Permissions::DEFAULT_EVERYONE | Permissions::KICK_MEMBERS));
        assert!(!permissions.contains(Permissions::BAN_MEMBERS));
        assert_eq!(guild.base_permissions(OWNER), Permissions::all());
        assert_eq!(guild.base_permissions(OUTSIDER), Permissions::empty());
    }

    #[tokio::test]
    async fn channel_overwrites_hide_channels() {
        let mut guild = sample_guild().await;
        let general = channel_named(&guild, "general");
        let everyone = guild.meta.everyone_role_id;
        guild.channels.get_mut(&general).unwrap().overwrites = vec![Overwrite {
            target: OverwriteTarget::Role(everyone),
            allow: Permissions::empty(),
            deny: Permissions::VIEW_CHANNEL,
        }];

        assert!(!guild.can_view(ALICE, general));
        assert!(guild.can_view(OWNER, general));
        assert_eq!(
            guild.channel_permissions(ALICE, general),
            Permissions::empty()
        );
        assert_eq!(guild.visible_channel_ids(ALICE).len(), 1);
        assert_eq!(guild.visible_channel_ids(OWNER).len(), 2);
        assert!(guild.visible_channel_ids(OUTSIDER).is_empty());
    }

    #[tokio::test]
    async fn unknown_channels_have_no_permissions() {
        let guild = sample_guild().await;

        assert_eq!(guild.channel_permissions(ALICE, 999), Permissions::empty());
    }

    #[tokio::test]
    async fn highest_position_follows_roles() {
        let mut guild = sample_guild().await;
        assert_eq!(guild.highest_position(ALICE), 0);

        guild.members.get_mut(&ALICE).unwrap().role_ids = vec![MOD_ROLE];

        assert_eq!(guild.highest_position(ALICE), 1);
    }
}
