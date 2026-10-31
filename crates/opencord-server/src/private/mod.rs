//! Private messages and friends (design 2026-10-22): the privacy settings
//! and blocks the server enforces, kept in memory, and the events waiting
//! for each device until it acknowledges them.

use std::collections::{HashMap, HashSet};
use std::sync::{PoisonError, RwLock};
use std::time::Instant;

use opencord_common::private::RECENT_SEND_TTL_MS;
use opencord_proto::v1 as proto;
use prost::Message as _;
use sqlx::{Sqlite, SqliteConnection, Transaction};

use crate::db;
use crate::error::ApiError;
use crate::state::{AppState, now_ms};

const DAY_MS: i64 = 86_400_000;

/// An event to store for one device.
#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    pub user_id: i64,
    pub device_id: [u8; 16],
    pub kind: proto::pending_event::Kind,
    /// On `FriendRequestReceived`, so a cancel can take it back.
    pub friend_request_id: Option<i64>,
}

/// Stores each event under a new id, then drops each device's oldest
/// beyond `max_pending_per_device`. Returns what to push, as (user id,
/// device id, event). A device that is gone fails on its foreign key.
pub async fn insert_pending(
    tx: &mut Transaction<'_, Sqlite>,
    state: &AppState,
    items: Vec<Pending>,
    now_ms: i64,
) -> Result<Vec<(i64, [u8; 16], proto::PendingEvent)>, sqlx::Error> {
    let mut devices: Vec<(i64, [u8; 16])> = Vec::new();
    let mut pushed = Vec::with_capacity(items.len());
    for item in items {
        let event = proto::PendingEvent {
            id: state.ids.next_id(),
            created_at_ms: now_ms,
            kind: Some(item.kind),
        };
        db::private::insert_pending(
            tx,
            event.id,
            item.user_id,
            &item.device_id,
            &event.encode_to_vec(),
            item.friend_request_id,
            now_ms,
        )
        .await?;
        if !devices.contains(&(item.user_id, item.device_id)) {
            devices.push((item.user_id, item.device_id));
        }
        pushed.push((item.user_id, item.device_id, event));
    }
    let keep = i64::from(state.config.private_messages.max_pending_per_device);
    for (user_id, device_id) in devices {
        db::private::trim_pending(tx, user_id, &device_id, keep).await?;
    }
    Ok(pushed)
}

/// Sends each event to the sessions holding its device. After the commit.
pub fn push_pending(state: &AppState, pushed: &[(i64, [u8; 16], proto::PendingEvent)]) {
    let now = Instant::now();
    for (user_id, device_id, event) in pushed {
        let event = proto::Event {
            kind: Some(proto::event::Kind::PendingEvent(event.clone())),
        };
        for session in state.sessions.for_user(*user_id) {
            if session.private_device() == Some(*device_id) {
                session.push_event(&event, now);
            }
        }
    }
}

/// Stores the events, then pushes them to the sessions holding their
/// devices.
pub async fn deliver(state: &AppState, items: Vec<Pending>) -> Result<(), ApiError> {
    let mut tx = state.db.begin().await?;
    let pushed = insert_pending(&mut tx, state, items, now_ms()).await?;
    tx.commit().await?;
    push_pending(state, &pushed);
    Ok(())
}

/// The member's devices, oldest first.
pub async fn devices_of(
    conn: &mut SqliteConnection,
    user_id: i64,
) -> Result<Vec<[u8; 16]>, sqlx::Error> {
    db::private::device_ids(conn, user_id)
        .await?
        .into_iter()
        .map(|id| {
            <[u8; 16]>::try_from(id.as_slice())
                .map_err(|_| sqlx::Error::Decode("a stored device id is not 16 bytes".into()))
        })
        .collect()
}

/// Deletes events older than `undelivered_days`, devices unseen for
/// `device_expiry_days` (with their keys and events), and send ids older
/// than a day. Sessions holding a deleted device no longer do.
pub async fn sweep(state: &AppState, now_ms: i64) -> Result<(), sqlx::Error> {
    let config = &state.config.private_messages;
    let mut conn = state.db.acquire().await?;
    let undelivered = i64::from(config.undelivered_days) * DAY_MS;
    db::private::delete_pending_before(&mut conn, now_ms - undelivered).await?;
    let expiry = i64::from(config.device_expiry_days) * DAY_MS;
    let removed = db::private::delete_devices_unseen_since(&mut conn, now_ms - expiry).await?;
    db::private::delete_recent_sends_before(&mut conn, now_ms - RECENT_SEND_TTL_MS).await?;
    for (user_id, device_id) in removed {
        for session in state.sessions.for_user(user_id) {
            if session
                .private_device()
                .is_some_and(|held| held.as_slice() == device_id)
            {
                session.set_private_device(None);
            }
        }
    }
    Ok(())
}

/// Deletes what the server keeps for a member who was kicked or banned.
/// Call [`PrivateMessages::forget`] after the commit.
pub async fn forget_member(
    tx: &mut Transaction<'_, Sqlite>,
    user_id: i64,
) -> Result<(), sqlx::Error> {
    db::private::forget_member(tx, user_id).await
}

/// A stored event, decoded.
pub(crate) fn decode_pending(event: &[u8]) -> Result<proto::PendingEvent, sqlx::Error> {
    proto::PendingEvent::decode(event).map_err(|error| sqlx::Error::Decode(Box::new(error)))
}

/// One member's privacy settings and block list on this server, and their
/// friends when the server relays only between friends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Privacy {
    pub allow_friend_requests: bool,
    pub allow_message_requests: bool,
    pub blocked: HashSet<i64>,
    pub friends: HashSet<i64>,
}

impl Default for Privacy {
    fn default() -> Self {
        Self {
            allow_friend_requests: true,
            allow_message_requests: true,
            blocked: HashSet::new(),
            friends: HashSet::new(),
        }
    }
}

const SETTINGS_PREFIX: &str = "private.";
const ENABLED_KEY: &str = "private.enabled";
const FRIENDS_ONLY_KEY: &str = "private.friends_only";

/// What the owner chose in Server settings, kept in `server_meta`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnerSettings {
    pub enabled: bool,
    pub friends_only: bool,
}

impl Default for OwnerSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            friends_only: false,
        }
    }
}

impl OwnerSettings {
    pub async fn load(conn: &mut SqliteConnection) -> Result<Self, sqlx::Error> {
        let values = db::meta::load_prefixed(conn, SETTINGS_PREFIX).await?;
        let defaults = Self::default();
        let flag =
            |key: &str, default: bool| values.get(key).map_or(default, |value| value == "true");
        Ok(Self {
            enabled: flag(ENABLED_KEY, defaults.enabled),
            friends_only: flag(FRIENDS_ONLY_KEY, defaults.friends_only),
        })
    }

    pub async fn save(&self, conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
        db::meta::put(conn, ENABLED_KEY, Some(&self.enabled.to_string())).await?;
        db::meta::put(conn, FRIENDS_ONLY_KEY, Some(&self.friends_only.to_string())).await
    }
}

/// What the server holds for one member. Intended for tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrivateCounts {
    pub devices: i64,
    /// Unclaimed, on every device.
    pub one_time_keys: i64,
    /// Waiting for any of the member's devices.
    pub pending: i64,
    /// Sent or received.
    pub friend_requests: i64,
}

#[derive(Debug)]
pub struct PrivateMessages {
    /// `[private_messages] enabled`.
    allowed_by_config: bool,
    owner: RwLock<OwnerSettings>,
    /// Members who sent `UpdatePrivacy`. An entry is replaced by the next
    /// one and removed by [`Self::forget`]; everyone else has the default.
    privacy: RwLock<HashMap<i64, Privacy>>,
}

impl PrivateMessages {
    pub fn new(allowed_by_config: bool) -> Self {
        Self {
            allowed_by_config,
            owner: RwLock::new(OwnerSettings::default()),
            privacy: RwLock::new(HashMap::new()),
        }
    }

    /// Whether this server relays private messages: the config file allows
    /// them and the owner has them on.
    pub fn enabled(&self) -> bool {
        self.allowed_by_config && self.owner_settings().enabled
    }

    /// Messages and typing go only between two members who list each other.
    pub fn friends_only(&self) -> bool {
        self.owner_settings().friends_only
    }

    pub fn owner_settings(&self) -> OwnerSettings {
        *self.owner.read().unwrap_or_else(PoisonError::into_inner)
    }

    /// After the database has the same.
    pub fn set_owner_settings(&self, settings: OwnerSettings) {
        *self.owner.write().unwrap_or_else(PoisonError::into_inner) = settings;
    }

    pub fn settings_proto(&self) -> proto::PrivateSettings {
        let owner = self.owner_settings();
        proto::PrivateSettings {
            enabled: self.enabled(),
            friends_only: owner.friends_only,
            allowed_by_config: self.allowed_by_config,
        }
    }

    /// Reads the owner's settings, and every member's saved settings,
    /// blocks and friends, at startup.
    pub async fn load(&self, conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
        self.set_owner_settings(OwnerSettings::load(conn).await?);
        let settings = db::private::privacy_settings(conn).await?;
        let blocks = db::private::blocks(conn).await?;
        let friends = db::private::friends(conn).await?;
        let mut privacy: HashMap<i64, Privacy> = settings
            .into_iter()
            .map(|row| {
                let saved = Privacy {
                    allow_friend_requests: row.allow_friend_requests,
                    allow_message_requests: row.allow_message_requests,
                    ..Privacy::default()
                };
                (row.user_id, saved)
            })
            .collect();
        for (owner, blocked) in blocks {
            privacy.entry(owner).or_default().blocked.insert(blocked);
        }
        for (owner, friend) in friends {
            privacy.entry(owner).or_default().friends.insert(friend);
        }
        *self.privacy.write().unwrap_or_else(PoisonError::into_inner) = privacy;
        Ok(())
    }

    /// Whether `a` and `b` each list the other as a friend.
    pub fn are_friends(&self, a: i64, b: i64) -> bool {
        let privacy = self.privacy.read().unwrap_or_else(PoisonError::into_inner);
        let lists = |owner: i64, other: i64| {
            privacy
                .get(&owner)
                .is_some_and(|saved| saved.friends.contains(&other))
        };
        lists(a, b) && lists(b, a)
    }

    /// After the database has the same: a request between them was
    /// accepted here (`true`), or one removed the other (`false`).
    pub fn set_friends(&self, a: i64, b: i64, friends: bool) {
        let mut privacy = self.privacy.write().unwrap_or_else(PoisonError::into_inner);
        for (owner, other) in [(a, b), (b, a)] {
            let list = &mut privacy.entry(owner).or_default().friends;
            if friends {
                list.insert(other);
            } else {
                list.remove(&other);
            }
        }
    }

    /// After the database has the same.
    pub fn clear_friends(&self) {
        let mut privacy = self.privacy.write().unwrap_or_else(PoisonError::into_inner);
        for saved in privacy.values_mut() {
            saved.friends.clear();
        }
    }

    pub fn privacy(&self, user_id: i64) -> Privacy {
        self.saved(user_id).unwrap_or_default()
    }

    /// Whether `owner` blocked `other`.
    pub fn has_blocked(&self, owner: i64, other: i64) -> bool {
        self.privacy
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&owner)
            .is_some_and(|privacy| privacy.blocked.contains(&other))
    }

    /// After the database has the same.
    pub fn set_privacy(&self, user_id: i64, privacy: Privacy) {
        self.privacy
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(user_id, privacy);
    }

    /// For a member who was kicked or banned: their own entry, and their
    /// place in others' friend lists.
    pub fn forget(&self, user_id: i64) {
        let mut privacy = self.privacy.write().unwrap_or_else(PoisonError::into_inner);
        privacy.remove(&user_id);
        for saved in privacy.values_mut() {
            saved.friends.remove(&user_id);
        }
    }

    /// The member's own settings, if they sent any.
    pub(crate) fn saved(&self, user_id: i64) -> Option<Privacy> {
        self.privacy
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&user_id)
            .cloned()
    }
}
