//! Private messages and friends (design 2026-10-22): the privacy settings
//! and blocks the server enforces, kept in memory.

use std::collections::{HashMap, HashSet};
use std::sync::{PoisonError, RwLock};

use sqlx::SqliteConnection;

use crate::db;

/// One member's privacy settings and block list on this server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Privacy {
    pub allow_friend_requests: bool,
    pub allow_message_requests: bool,
    pub blocked: HashSet<i64>,
}

impl Default for Privacy {
    fn default() -> Self {
        Self {
            allow_friend_requests: true,
            allow_message_requests: true,
            blocked: HashSet::new(),
        }
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
    enabled: bool,
    /// Members who sent `UpdatePrivacy`. An entry is replaced by the next
    /// one and removed by [`Self::forget`]; everyone else has the default.
    privacy: RwLock<HashMap<i64, Privacy>>,
}

impl PrivateMessages {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            privacy: RwLock::new(HashMap::new()),
        }
    }

    /// `[private_messages] enabled`.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Reads the saved settings and blocks, at startup.
    pub async fn load(&self, conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
        let settings = db::private::privacy_settings(conn).await?;
        let blocks = db::private::blocks(conn).await?;
        let mut privacy: HashMap<i64, Privacy> = settings
            .into_iter()
            .map(|row| {
                let saved = Privacy {
                    allow_friend_requests: row.allow_friend_requests,
                    allow_message_requests: row.allow_message_requests,
                    blocked: HashSet::new(),
                };
                (row.user_id, saved)
            })
            .collect();
        for (owner, blocked) in blocks {
            privacy.entry(owner).or_default().blocked.insert(blocked);
        }
        *self.privacy.write().unwrap_or_else(PoisonError::into_inner) = privacy;
        Ok(())
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

    /// For a member who was kicked or banned.
    pub fn forget(&self, user_id: i64) {
        self.privacy
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&user_id);
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
