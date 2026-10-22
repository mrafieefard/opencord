//! The status each user picked. Whether they are online at all depends on
//! having a session.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use opencord_proto::v1::PresenceStatus;

#[derive(Debug, Default)]
pub struct Presence {
    chosen: Mutex<HashMap<i64, PresenceStatus>>,
}

impl Presence {
    /// Online unless the user picked something else.
    pub fn chosen(&self, user_id: i64) -> PresenceStatus {
        self.chosen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&user_id)
            .copied()
            .unwrap_or(PresenceStatus::Online)
    }

    pub fn set(&self, user_id: i64, status: PresenceStatus) {
        self.chosen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(user_id, status);
    }

    pub fn clear(&self, user_id: i64) {
        self.chosen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&user_id);
    }
}
