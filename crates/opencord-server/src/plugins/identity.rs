//! Plugin users and managed roles (design 2026-11-07 §8.2): who a plugin
//! acts as, and the role that decides what it may do and see.

use std::collections::HashSet;

use opencord_common::limits::MAX_ROLES;
use opencord_common::permissions::OverwriteTarget;
use opencord_common::validation;
use opencord_proto::v1 as proto;
use proto::event::Kind as Event;

use super::permissions::{PluginPermission, role_permissions};
use crate::db::users::UserRow;
use crate::db::{channels, plugins as db, roles};
use crate::error::ApiError;
use crate::guild::{Guild, PluginUser, Role};
use crate::random;
use crate::state::{AppState, Audience, now_ms};
use crate::visibility::Visibility;

/// The user and role a plugin acts through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginIdentity {
    pub user_id: i64,
    pub role_id: i64,
}

/// Every plugin user, installed or not, for `Ready` and `PluginUsersUpdate`.
pub fn plugin_users_proto(guild: &Guild) -> Vec<proto::PluginUser> {
    let mut users: Vec<proto::PluginUser> = guild
        .plugin_users
        .iter()
        .map(|(user_id, user)| proto::PluginUser {
            user_id: *user_id,
            plugin_id: user.plugin_id.clone(),
            name: user.name.clone(),
            avatar_hash: user.avatar_hash.clone().unwrap_or_default(),
            installed: user.installed,
        })
        .collect();
    users.sort_unstable_by_key(|user| user.user_id);
    users
}

/// Makes sure the plugin has its user and managed role. A reinstall gets
/// its old user back; a role left by an install that never finished is
/// taken over, so a plugin never has two.
pub async fn ensure(
    state: &AppState,
    plugin_id: &str,
    name: &str,
    avatar_hash: Option<&str>,
    installer_id: i64,
    granted: &HashSet<PluginPermission>,
) -> Result<PluginIdentity, ApiError> {
    let role_name = role_name(name)?;
    let permissions = role_permissions(granted);
    let _writes = state.write_lock().await;
    let (existing_user, role, shifted, is_new_role) = {
        let guild = state.guild();
        let existing_user = guild
            .plugin_users
            .iter()
            .find(|(_, user)| user.plugin_id == plugin_id)
            .map(|(user_id, _)| *user_id);
        let left = guild
            .roles
            .values()
            .find(|role| role.plugin_id.as_deref() == Some(plugin_id));
        match left {
            Some(role) => {
                let role = Role {
                    name: role_name,
                    permissions,
                    ..role.clone()
                };
                (existing_user, role, Vec::new(), false)
            }
            None => {
                if guild.roles.len() >= MAX_ROLES {
                    return Err(ApiError::conflict(format!(
                        "a server can have at most {MAX_ROLES} roles"
                    )));
                }
                let position = new_role_position(&guild, installer_id);
                let shifted: Vec<Role> = guild
                    .roles
                    .values()
                    .filter(|role| role.position >= position)
                    .map(|role| Role {
                        position: role.position + 1,
                        ..role.clone()
                    })
                    .collect();
                let role = Role {
                    id: state.ids.next_id(),
                    name: role_name,
                    color: 0,
                    position,
                    permissions,
                    hoist: false,
                    mentionable: false,
                    max_upload_bytes: None,
                    plugin_id: Some(plugin_id.to_owned()),
                };
                (existing_user, role, shifted, true)
            }
        }
    };
    let user_id = existing_user.unwrap_or_else(|| state.ids.next_id());
    let mut tx = state.db.begin().await?;
    if existing_user.is_some() {
        db::set_plugin_user_name(&mut tx, user_id, name, avatar_hash).await?;
    } else {
        let user = UserRow {
            id: user_id,
            // No private half exists, so nobody can sign in as it.
            public_key: random::bytes::<32>().to_vec(),
            display_name: name.to_owned(),
            avatar_hash: avatar_hash.map(str::to_owned),
        };
        db::insert_plugin_user(&mut tx, &user, plugin_id, now_ms()).await?;
    }
    for moved in &shifted {
        roles::update(&mut tx, &moved.to_row()).await?;
    }
    if is_new_role {
        roles::insert(&mut tx, &role.to_row()).await?;
    } else {
        roles::update(&mut tx, &role.to_row()).await?;
    }
    tx.commit().await?;

    {
        let mut guild = state.guild_mut();
        for moved in &shifted {
            guild.roles.insert(moved.id, moved.clone());
        }
        guild.roles.insert(role.id, role.clone());
        guild.plugin_users.insert(
            user_id,
            PluginUser {
                plugin_id: plugin_id.to_owned(),
                role_id: role.id,
                name: name.to_owned(),
                avatar_hash: avatar_hash.map(str::to_owned),
                installed: true,
            },
        );
    }
    let role_proto = Some(role.to_proto());
    let event = if is_new_role {
        Event::RoleCreate(proto::RoleCreate { role: role_proto })
    } else {
        Event::RoleUpdate(proto::RoleUpdate { role: role_proto })
    };
    state.broadcast(event, Audience::Everyone);
    broadcast_role_updates(state, &shifted);
    broadcast_plugin_users(state);
    Ok(PluginIdentity {
        user_id,
        role_id: role.id,
    })
}

/// The managed role's permissions follow the plugin's granted ones.
pub async fn set_permissions(
    state: &AppState,
    plugin_id: &str,
    granted: &HashSet<PluginPermission>,
) -> Result<(), ApiError> {
    let permissions = role_permissions(granted);
    let _writes = state.write_lock().await;
    let updated = {
        let guild = state.guild();
        let role = managed_role(&guild, plugin_id)?;
        if role.permissions == permissions {
            return Ok(());
        }
        Role {
            permissions,
            ..role.clone()
        }
    };
    roles::update(&mut *state.db.acquire().await?, &updated.to_row()).await?;

    state.guild_mut().roles.insert(updated.id, updated.clone());
    broadcast_role_updates(state, std::slice::from_ref(&updated));
    Ok(())
}

/// An update's name and icon. The role is renamed only when the plugin's
/// name changed, so a name an admin gave it survives other updates.
pub async fn rename(
    state: &AppState,
    plugin_id: &str,
    name: &str,
    avatar_hash: Option<&str>,
) -> Result<(), ApiError> {
    let role_name = role_name(name)?;
    let _writes = state.write_lock().await;
    let (user_id, user, role) = {
        let guild = state.guild();
        let (user_id, user) = installed(&guild, plugin_id)?;
        let role = guild
            .roles
            .get(&user.role_id)
            .ok_or_else(ApiError::internal)?;
        let role = (user.name != name).then(|| Role {
            name: role_name,
            ..role.clone()
        });
        let user = PluginUser {
            name: name.to_owned(),
            avatar_hash: avatar_hash.map(str::to_owned),
            ..user.clone()
        };
        (user_id, user, role)
    };
    let mut tx = state.db.begin().await?;
    db::set_plugin_user_name(&mut tx, user_id, name, avatar_hash).await?;
    if let Some(role) = &role {
        roles::update(&mut tx, &role.to_row()).await?;
    }
    tx.commit().await?;

    {
        let mut guild = state.guild_mut();
        guild.plugin_users.insert(user_id, user);
        if let Some(role) = &role {
            guild.roles.insert(role.id, role.clone());
        }
    }
    broadcast_role_updates(state, role.as_slice());
    broadcast_plugin_users(state);
    Ok(())
}

/// Uninstalling: the managed role goes as `DeleteRole` removes a role
/// (its overwrites too), and with it the plugin's row, which points at
/// it. The plugin user stays, not installed. The caller reads what it
/// keeps from the plugin's row first.
pub async fn retire(state: &AppState, plugin_id: &str) -> Result<(), ApiError> {
    let _writes = state.write_lock().await;
    let (user_id, role_id, channels_with_overwrites) = {
        let guild = state.guild();
        let (user_id, user) = installed(&guild, plugin_id)?;
        let target = OverwriteTarget::Role(user.role_id);
        let channels: Vec<i64> = guild
            .channels
            .values()
            .filter(|channel| channel.overwrites.iter().any(|o| o.target == target))
            .map(|channel| channel.id)
            .collect();
        (user_id, user.role_id, channels)
    };
    let mut tx = state.db.begin().await?;
    channels::delete_overwrites_for_target(&mut tx, "role", role_id).await?;
    db::delete_plugin(&mut tx, plugin_id).await?;
    roles::delete(&mut tx, role_id).await?;
    tx.commit().await?;

    let before = Visibility::capture(state);
    {
        let mut guild = state.guild_mut();
        guild.roles.remove(&role_id);
        let target = OverwriteTarget::Role(role_id);
        for channel_id in &channels_with_overwrites {
            if let Some(channel) = guild.channels.get_mut(channel_id) {
                channel
                    .overwrites
                    .retain(|overwrite| overwrite.target != target);
            }
        }
        if let Some(user) = guild.plugin_users.get_mut(&user_id) {
            user.installed = false;
            user.role_id = 0;
        }
    }
    state.broadcast(
        Event::RoleDelete(proto::RoleDelete { role_id }),
        Audience::Everyone,
    );
    before.announce(state, &channels_with_overwrites);
    broadcast_plugin_users(state);
    Ok(())
}

fn role_name(plugin_name: &str) -> Result<String, ApiError> {
    Ok(validation::role_name(&format!("Plugin: {plugin_name}"))?)
}

/// Just below the installer's highest role; the owner's is the top.
fn new_role_position(guild: &Guild, installer_id: i64) -> i32 {
    if guild.is_owner(installer_id) {
        let top = guild.roles.values().map(|role| role.position).max();
        top.unwrap_or(0) + 1
    } else {
        // Above @everyone even for an installer without roles.
        guild.highest_position(installer_id).max(1)
    }
}

fn installed<'a>(guild: &'a Guild, plugin_id: &str) -> Result<(i64, &'a PluginUser), ApiError> {
    guild
        .plugin_users
        .iter()
        .find(|(_, user)| user.installed && user.plugin_id == plugin_id)
        .map(|(user_id, user)| (*user_id, user))
        .ok_or_else(|| ApiError::not_found("plugin"))
}

fn managed_role<'a>(guild: &'a Guild, plugin_id: &str) -> Result<&'a Role, ApiError> {
    let (_, user) = installed(guild, plugin_id)?;
    guild
        .roles
        .get(&user.role_id)
        .ok_or_else(ApiError::internal)
}

fn broadcast_role_updates(state: &AppState, roles: &[Role]) {
    for role in roles {
        state.broadcast(
            Event::RoleUpdate(proto::RoleUpdate {
                role: Some(role.to_proto()),
            }),
            Audience::Everyone,
        );
    }
}

fn broadcast_plugin_users(state: &AppState) {
    let users = plugin_users_proto(&state.guild());
    state.broadcast(
        Event::PluginUsersUpdate(proto::PluginUsersUpdate { users }),
        Audience::Everyone,
    );
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Arc;

    use bytes::Bytes;
    use ed25519_dalek::SigningKey;
    use opencord_common::permissions::{Overwrite, OverwriteTarget, Permissions};
    use opencord_common::snowflake::SnowflakeGenerator;
    use prost::Message as _;
    use proto::event::Kind as Event;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use super::PluginPermission as P;
    use super::*;
    use crate::bootstrap;
    use crate::config::Config;
    use crate::db::{channels, roles, test_pool};
    use crate::gateway::session::{CloseHandle, Connection, Session};
    use crate::guild::{Member, PluginUser, Role, User};
    use crate::voice::nodes::VoiceNodes;

    pub(crate) const OWNER: i64 = 1;
    pub(crate) const ADMIN: i64 = 2;
    const ADMIN_ROLE: i64 = 20;
    const LOW_ROLE: i64 = 21;

    /// A bootstrapped server whose owner is OWNER, with ADMIN holding
    /// ADMIN_ROLE (ADMINISTRATOR, position 2) above LOW_ROLE (position 1).
    pub(crate) async fn state() -> (Arc<AppState>, tempfile::TempDir) {
        let (pool, dir) = test_pool().await;
        let ids = SnowflakeGenerator::new(0).unwrap();
        let bootstrap = bootstrap::initialize(&pool, "Test", &ids, 0).await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let mut guild = Guild::load(&mut conn, bootstrap.meta).await.unwrap();
        guild.meta.owner_id = Some(OWNER);
        for (id, position, permissions) in [
            (ADMIN_ROLE, 2, Permissions::ADMINISTRATOR),
            (LOW_ROLE, 1, Permissions::empty()),
        ] {
            let role = Role {
                id,
                name: format!("role{id}"),
                color: 0,
                position,
                permissions,
                hoist: false,
                mentionable: false,
                max_upload_bytes: None,
                plugin_id: None,
            };
            roles::insert(&mut conn, &role.to_row()).await.unwrap();
            guild.roles.insert(id, role);
        }
        for (id, role_ids) in [(OWNER, vec![]), (ADMIN, vec![ADMIN_ROLE])] {
            guild.members.insert(
                id,
                Member {
                    user: User {
                        id,
                        public_key: vec![u8::try_from(id).unwrap(); 32],
                        display_name: format!("user{id}"),
                        avatar_hash: None,
                    },
                    nickname: None,
                    role_ids,
                    joined_at: 0,
                },
            );
        }
        drop(conn);
        let state = AppState::new(
            Config::default(),
            pool,
            ids,
            [0; 32],
            guild,
            SigningKey::from_bytes(&[1; 32]),
            VoiceNodes::new(None, [0; 32]),
        );
        (Arc::new(state), dir)
    }

    /// The events a session of OWNER receives from now on.
    fn listen(state: &AppState) -> mpsc::Receiver<Bytes> {
        let (outbound, frames) = mpsc::channel(64);
        let connection = Connection {
            id: 1,
            outbound,
            closer: CloseHandle::new(CancellationToken::new()),
        };
        state.sessions.insert(Arc::new(Session::new(
            "watcher".to_owned(),
            OWNER,
            b"token",
            connection,
        )));
        frames
    }

    fn received(frames: &mut mpsc::Receiver<Bytes>) -> Vec<Event> {
        let mut events = Vec::new();
        while let Ok(frame) = frames.try_recv() {
            if let Some(proto::envelope::Payload::Event(event)) =
                proto::Envelope::decode(frame).unwrap().payload
            {
                events.push(event.kind.unwrap());
            }
        }
        events
    }

    /// The guild as the database holds it.
    async fn reloaded(state: &AppState) -> Guild {
        let meta = state.guild().meta.clone();
        let mut conn = state.db.acquire().await.unwrap();
        Guild::load(&mut conn, meta).await.unwrap()
    }

    async fn assert_saved(state: &AppState) {
        let saved = reloaded(state).await;
        let guild = state.guild();
        assert_eq!(saved.roles, guild.roles);
        assert_eq!(saved.plugin_users, guild.plugin_users);
        assert_eq!(saved.channels, guild.channels);
    }

    fn position(state: &AppState, role_id: i64) -> i32 {
        state.guild().roles[&role_id].position
    }

    /// The plugins row Task 8 writes after `ensure`.
    async fn record_install(state: &AppState, plugin_id: &str, identity: PluginIdentity) {
        sqlx::query(
            "INSERT INTO plugins (id, user_id, role_id, version, folder, manifest, signature,
                                  running, installed_at, updated_at)
             VALUES ($1, $2, $3, '1.0.0', 'v1', '{}', 'unsigned', 1, 0, 0)",
        )
        .bind(plugin_id)
        .bind(identity.user_id)
        .bind(identity.role_id)
        .execute(&state.db)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn ensure_places_the_role_just_below_the_installer() {
        // Catches: a plugin role placed above the admin who installed it
        // (it could then manage that admin's role), or roles left sharing
        // a position.
        let (state, _dir) = state().await;
        let mut frames = listen(&state);
        let granted = [P::SendMessages, P::KickMembers].into();

        let identity = ensure(&state, "tickets", "Tickets", Some("abc"), ADMIN, &granted)
            .await
            .unwrap();

        assert_eq!(position(&state, identity.role_id), 2);
        assert_eq!(position(&state, ADMIN_ROLE), 3);
        assert_eq!(position(&state, LOW_ROLE), 1);
        let role = state.guild().roles[&identity.role_id].clone();
        assert_eq!(role.name, "Plugin: Tickets");
        assert_eq!(
            role.permissions,
            Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES | Permissions::KICK_MEMBERS
        );
        assert!(!role.hoist && !role.mentionable && role.color == 0);
        assert_eq!(role.plugin_id.as_deref(), Some("tickets"));
        assert_eq!(
            state.guild().plugin_users[&identity.user_id],
            PluginUser {
                plugin_id: "tickets".to_owned(),
                role_id: identity.role_id,
                name: "Tickets".to_owned(),
                avatar_hash: Some("abc".to_owned()),
                installed: true,
            }
        );
        assert!(!state.guild().members.contains_key(&identity.user_id));
        assert_saved(&state).await;
        let events = received(&mut frames);
        assert!(matches!(&events[0], Event::RoleCreate(created)
            if created.role.as_ref().unwrap().id == identity.role_id));
        assert!(matches!(&events[1], Event::RoleUpdate(updated)
            if updated.role.as_ref().unwrap().id == ADMIN_ROLE));
        let Event::PluginUsersUpdate(update) = &events[2] else {
            panic!("{events:?}");
        };
        assert_eq!(update.users, plugin_users_proto(&state.guild()));
        assert_eq!(update.users.len(), 1);
        assert_eq!(update.users[0].user_id, identity.user_id);
        assert_eq!(events.len(), 3);
    }

    #[tokio::test]
    async fn the_owner_puts_it_at_the_top() {
        let (state, _dir) = state().await;

        let identity = ensure(&state, "tickets", "Tickets", None, OWNER, &HashSet::new())
            .await
            .unwrap();

        assert_eq!(position(&state, identity.role_id), 3);
        assert_eq!(position(&state, ADMIN_ROLE), 2);
        assert_eq!(position(&state, LOW_ROLE), 1);
        assert_saved(&state).await;
    }

    #[tokio::test]
    async fn ensure_twice_keeps_one_user_and_one_role() {
        // Catches: a second user or role for one plugin, which would leave
        // a role nobody can delete.
        let (state, _dir) = state().await;
        let first = ensure(&state, "tickets", "Tickets", None, OWNER, &HashSet::new())
            .await
            .unwrap();
        let roles = state.guild().roles.len();

        let second = ensure(
            &state,
            "tickets",
            "Tickets",
            None,
            OWNER,
            &[P::BanMembers].into(),
        )
        .await
        .unwrap();

        assert_eq!(second, first);
        assert_eq!(state.guild().roles.len(), roles);
        assert_eq!(
            state.guild().roles[&first.role_id].permissions,
            Permissions::VIEW_CHANNEL | Permissions::BAN_MEMBERS
        );
        assert_saved(&state).await;
    }

    #[tokio::test]
    async fn retire_removes_the_role_and_keeps_the_user() {
        // Catches: a retired plugin keeping its role or its overwrites, or
        // its user disappearing (its old messages would lose their author).
        let (state, _dir) = state().await;
        let identity = ensure(&state, "tickets", "Tickets", None, OWNER, &HashSet::new())
            .await
            .unwrap();
        record_install(&state, "tickets", identity).await;
        let general = state
            .guild()
            .channels
            .values()
            .find(|channel| channel.name == "general")
            .unwrap()
            .id;
        let overwrite = Overwrite {
            target: OverwriteTarget::Role(identity.role_id),
            allow: Permissions::empty(),
            deny: Permissions::VIEW_CHANNEL,
        };
        channels::upsert_overwrite(
            &mut state.db.acquire().await.unwrap(),
            &crate::guild::overwrite_row(general, &overwrite),
        )
        .await
        .unwrap();
        state
            .guild_mut()
            .channels
            .get_mut(&general)
            .unwrap()
            .overwrites = vec![overwrite];
        let mut frames = listen(&state);

        retire(&state, "tickets").await.unwrap();

        assert!(!state.guild().roles.contains_key(&identity.role_id));
        assert!(state.guild().channels[&general].overwrites.is_empty());
        let user = state.guild().plugin_users[&identity.user_id].clone();
        assert!(!user.installed);
        assert_eq!(user.role_id, 0);
        assert_eq!(
            state.guild().base_permissions(identity.user_id),
            Permissions::empty()
        );
        let plugins: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM plugins")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(plugins, 0);
        assert_saved(&state).await;
        let events = received(&mut frames);
        assert!(events.iter().any(|event| matches!(event,
            Event::RoleDelete(deleted) if deleted.role_id == identity.role_id)));
        assert!(events.iter().any(|event| matches!(event,
            Event::ChannelUpdate(updated) if updated.channel.as_ref().unwrap().id == general)));
        assert!(
            matches!(events.last(), Some(Event::PluginUsersUpdate(update))
            if !update.users[0].installed)
        );

        let again = ensure(&state, "tickets", "Tickets", None, OWNER, &HashSet::new())
            .await
            .unwrap();

        assert_eq!(again.user_id, identity.user_id);
        assert_ne!(again.role_id, identity.role_id);
        assert!(state.guild().plugin_users[&identity.user_id].installed);
        assert_saved(&state).await;
    }

    #[tokio::test]
    async fn set_permissions_moves_the_role_with_the_grant() {
        // Catches: a revoked permission whose member bit the role keeps.
        let (state, _dir) = state().await;
        let identity = ensure(
            &state,
            "tickets",
            "Tickets",
            None,
            OWNER,
            &[P::KickMembers].into(),
        )
        .await
        .unwrap();

        set_permissions(&state, "tickets", &[P::ManageMessages].into())
            .await
            .unwrap();

        assert_eq!(
            state.guild().roles[&identity.role_id].permissions,
            Permissions::VIEW_CHANNEL | Permissions::MANAGE_MESSAGES
        );
        assert_saved(&state).await;
        assert!(
            set_permissions(&state, "other", &HashSet::new())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn rename_renames_the_user_and_the_role() {
        let (state, _dir) = state().await;
        let identity = ensure(&state, "tickets", "Tickets", None, OWNER, &HashSet::new())
            .await
            .unwrap();

        rename(&state, "tickets", "Help desk", Some("def"))
            .await
            .unwrap();

        let user = state.guild().plugin_users[&identity.user_id].clone();
        assert_eq!(user.name, "Help desk");
        assert_eq!(user.avatar_hash.as_deref(), Some("def"));
        assert_eq!(
            state.guild().roles[&identity.role_id].name,
            "Plugin: Help desk"
        );
        assert_saved(&state).await;
    }

    #[tokio::test]
    async fn a_new_icon_keeps_the_name_an_admin_gave_the_role() {
        // Catches: every update renaming the role back over an admin's
        // name when the plugin's own name didn't change.
        let (state, _dir) = state().await;
        let identity = ensure(&state, "tickets", "Tickets", None, OWNER, &HashSet::new())
            .await
            .unwrap();
        let renamed = Role {
            name: "Bots".to_owned(),
            ..state.guild().roles[&identity.role_id].clone()
        };
        roles::update(&mut state.db.acquire().await.unwrap(), &renamed.to_row())
            .await
            .unwrap();
        state.guild_mut().roles.insert(renamed.id, renamed);

        rename(&state, "tickets", "Tickets", Some("def"))
            .await
            .unwrap();

        assert_eq!(state.guild().roles[&identity.role_id].name, "Bots");
        assert_eq!(
            state.guild().plugin_users[&identity.user_id]
                .avatar_hash
                .as_deref(),
            Some("def")
        );
        assert_saved(&state).await;
    }
}
