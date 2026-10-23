//! Who is a member where (design 2026-10-22 §9): for each server, this
//! person's id there, and every member's id, public key and name. Private
//! messages reach a person through a server they share.
//!
//! Entries leave as follows: each `Ready` replaces its server's entry, a
//! `MemberLeave` removes that member, and removing the server drops its
//! entry. A dropped connection only marks the entry disconnected.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use opencord_proto::v1 as proto;

use crate::api::types::SharedServer;

/// One member, as last seen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberInfo {
    pub user_id: i64,
    pub public_key: [u8; 32],
    pub name: String,
}

impl MemberInfo {
    /// `None` for a user whose public key isn't 32 bytes.
    pub fn of(user: &proto::User) -> Option<Self> {
        Some(Self {
            user_id: user.id,
            public_key: user.public_key.as_slice().try_into().ok()?,
            name: user.display_name.clone(),
        })
    }
}

/// One server's entry.
#[derive(Debug, Default)]
struct ServerMembers {
    self_id: i64,
    /// The server's id from `Hello`, hex.
    server_id: String,
    name: String,
    connected: bool,
    private_messages: bool,
    by_id: HashMap<i64, MemberInfo>,
    by_key: HashMap<[u8; 32], i64>,
}

impl ServerMembers {
    fn insert(&mut self, member: MemberInfo) {
        if let Some(earlier) = self.by_id.get(&member.user_id)
            && earlier.public_key != member.public_key
        {
            self.by_key.remove(&earlier.public_key);
        }
        self.by_key.insert(member.public_key, member.user_id);
        self.by_id.insert(member.user_id, member);
    }

    fn remove(&mut self, user_id: i64) {
        if let Some(member) = self.by_id.remove(&user_id) {
            self.by_key.remove(&member.public_key);
        }
    }
}

/// The member lists, by server key.
#[derive(Debug, Default)]
pub struct MemberIndex {
    servers: Mutex<HashMap<String, ServerMembers>>,
}

impl MemberIndex {
    /// Replaces the server's entry with what `ready` says. Returns its
    /// members.
    pub fn on_ready(&self, key: &str, server_id: &str, ready: &proto::Ready) -> Vec<MemberInfo> {
        let mut entry = ServerMembers {
            self_id: ready.self_user.as_ref().map_or(0, |user| user.id),
            server_id: server_id.to_owned(),
            name: ready
                .server
                .as_ref()
                .map(|server| server.name.clone())
                .unwrap_or_default(),
            connected: true,
            private_messages: ready.private_messages,
            ..ServerMembers::default()
        };
        let members: Vec<MemberInfo> = ready
            .members
            .iter()
            .filter_map(|member| member.user.as_ref().and_then(MemberInfo::of))
            .collect();
        for member in &members {
            entry.insert(member.clone());
        }
        self.lock().insert(key.to_owned(), entry);
        members
    }

    /// A member joined or changed.
    pub fn upsert(&self, key: &str, member: MemberInfo) {
        if let Some(entry) = self.lock().get_mut(key) {
            entry.insert(member);
        }
    }

    pub fn remove_member(&self, key: &str, user_id: i64) {
        if let Some(entry) = self.lock().get_mut(key) {
            entry.remove(user_id);
        }
    }

    pub fn set_connected(&self, key: &str, connected: bool) {
        if let Some(entry) = self.lock().get_mut(key) {
            entry.connected = connected;
        }
    }

    /// The server was removed.
    pub fn remove_server(&self, key: &str) {
        self.lock().remove(key);
    }

    /// The server's id from `Hello`, hex, once it was ready.
    pub fn server_id(&self, key: &str) -> Option<String> {
        self.lock()
            .get(key)
            .map(|entry| entry.server_id.clone())
            .filter(|id| !id.is_empty())
    }

    /// The servers connected now that relay private messages.
    pub fn private_servers(&self) -> Vec<String> {
        self.lock()
            .iter()
            .filter(|(_, entry)| entry.connected && entry.private_messages)
            .map(|(key, _)| key.clone())
            .collect()
    }

    pub fn self_id(&self, key: &str) -> Option<i64> {
        self.lock().get(key).map(|entry| entry.self_id)
    }

    pub fn public_key_of(&self, key: &str, user_id: i64) -> Option<[u8; 32]> {
        self.lock()
            .get(key)?
            .by_id
            .get(&user_id)
            .map(|member| member.public_key)
    }

    pub fn user_id_on(&self, key: &str, peer: &[u8; 32]) -> Option<i64> {
        self.lock().get(key)?.by_key.get(peer).copied()
    }

    /// Every server `peer` is a member of, by server key.
    pub fn shared_servers(&self, peer: &[u8; 32]) -> Vec<SharedServer> {
        let mut shared: Vec<SharedServer> = self
            .lock()
            .iter()
            .filter_map(|(key, entry)| {
                entry.by_key.get(peer).map(|user_id| SharedServer {
                    server_key: key.clone(),
                    name: entry.name.clone(),
                    connected: entry.connected,
                    private_messages: entry.private_messages,
                    peer_user_id: *user_id,
                })
            })
            .collect();
        shared.sort_by(|a, b| a.server_key.cmp(&b.server_key));
        shared
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, ServerMembers>> {
        self.servers.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(id: i64, key: u8, name: &str) -> proto::User {
        proto::User {
            id,
            public_key: vec![key; 32],
            display_name: name.to_owned(),
            avatar_hash: String::new(),
        }
    }

    fn ready(self_id: i64, members: &[proto::User], private_messages: bool) -> proto::Ready {
        proto::Ready {
            self_user: Some(user(self_id, 0, "me")),
            members: members
                .iter()
                .map(|user| proto::Member {
                    user: Some(user.clone()),
                    ..proto::Member::default()
                })
                .collect(),
            private_messages,
            ..proto::Ready::default()
        }
    }

    #[test]
    fn entries_leave_on_ready_leave_and_removal_but_not_on_disconnect() {
        let index = MemberIndex::default();
        index.on_ready(
            "a",
            "aa",
            &ready(1, &[user(2, 2, "two"), user(3, 3, "three")], true),
        );
        index.on_ready("b", "bb", &ready(5, &[user(6, 2, "two")], false));
        assert_eq!(index.user_id_on("a", &[2; 32]), Some(2));
        assert_eq!(index.shared_servers(&[2; 32]).len(), 2);
        assert_eq!(index.private_servers(), vec!["a".to_owned()]);

        // A member leaves.
        index.remove_member("a", 3);
        assert_eq!(index.user_id_on("a", &[3; 32]), None);
        assert_eq!(index.public_key_of("a", 3), None);

        // A dropped connection keeps the members.
        index.set_connected("a", false);
        assert_eq!(index.user_id_on("a", &[2; 32]), Some(2));
        assert!(!index.shared_servers(&[2; 32])[0].connected);
        assert!(index.private_servers().is_empty());

        // The next Ready replaces the entry.
        index.on_ready("a", "aa", &ready(1, &[user(4, 4, "four")], true));
        assert_eq!(index.user_id_on("a", &[2; 32]), None);
        assert_eq!(index.user_id_on("a", &[4; 32]), Some(4));
        assert_eq!(index.private_servers(), vec!["a".to_owned()]);

        // Removing the server drops its entry.
        index.remove_server("b");
        assert!(index.shared_servers(&[2; 32]).is_empty());
        assert_eq!(index.server_id("b"), None);
    }

    #[test]
    fn a_member_with_a_new_key_is_found_only_by_the_new_one() {
        let index = MemberIndex::default();
        index.on_ready("a", "aa", &ready(1, &[user(2, 2, "two")], true));
        index.upsert("a", MemberInfo::of(&user(2, 9, "two")).unwrap());
        assert_eq!(index.user_id_on("a", &[2; 32]), None);
        assert_eq!(index.user_id_on("a", &[9; 32]), Some(2));
    }

    #[test]
    fn a_member_whose_key_is_not_32_bytes_is_left_out() {
        let index = MemberIndex::default();
        let mut odd = user(2, 2, "two");
        odd.public_key.truncate(31);
        let members = index.on_ready("a", "aa", &ready(1, &[odd], true));
        assert!(members.is_empty());
        assert_eq!(index.public_key_of("a", 2), None);
    }
}
