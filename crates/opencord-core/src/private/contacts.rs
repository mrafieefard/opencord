//! Contacts and friend rules (design 2026-10-22 §7, §8).

use opencord_common::private::{SPAM_AFTER_DECLINES, verify_slip};
use opencord_proto::v1 as proto;
use sqlx::{Row, SqliteConnection};

use super::PrivateError;
use super::seal::Sealer;

pub const CONTACT_NAME_LABEL: &str = "contacts.name";
pub const PRIVACY_FRIEND_REQUESTS: &str = "privacy.allow_friend_requests";
pub const PRIVACY_MESSAGE_REQUESTS: &str = "privacy.allow_message_requests";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FriendState {
    None = 0,
    Outgoing = 1,
    Incoming = 2,
    Friends = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conversation {
    None = 0,
    Request = 1,
    Accepted = 2,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestRef {
    pub server_id: String,
    pub request_id: i64,
    pub spam: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    pub public_key: [u8; 32],
    pub name: Option<String>,
    pub friend_state: FriendState,
    pub request: Option<RequestRef>,
    pub friend_server_id: Option<String>,
    pub blocked: bool,
    pub declines: u32,
    pub conversation: Conversation,
    pub relay_server_id: Option<String>,
    pub their_slip: Option<[u8; 64]>,
    pub last_activity_at: Option<i64>,
    pub last_read_at: i64,
    pub muted: bool,
    pub hidden: bool,
}

impl Contact {
    pub fn new(public_key: [u8; 32]) -> Self {
        Self {
            public_key,
            name: None,
            friend_state: FriendState::None,
            request: None,
            friend_server_id: None,
            blocked: false,
            declines: 0,
            conversation: Conversation::None,
            relay_server_id: None,
            their_slip: None,
            last_activity_at: None,
            last_read_at: 0,
            muted: false,
            hidden: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivacySettings {
    pub allow_friend_requests: bool,
    pub allow_message_requests: bool,
}

impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            allow_friend_requests: true,
            allow_message_requests: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncomingMessage {
    Drop,
    Request,
    Conversation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncomingRequest {
    Drop,
    AutoAccept,
    CrossedAccept,
    Spam,
    Pending,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FollowUp {
    Answer {
        server_id: String,
        request_id: i64,
        accept: bool,
    },
    Cancel {
        server_id: String,
        request_id: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FriendOutcome {
    pub changed: Option<Contact>,
    pub new_request: Option<bool /* spam */>,
    pub follow_ups: Vec<FollowUp>,
}

pub enum FriendEvent<'a> {
    Received(&'a proto::FriendRequestReceived),
    Sent(&'a proto::FriendRequestSent),
    Answered(&'a proto::FriendRequestAnswered),
    Cancelled(&'a proto::FriendRequestCancelled),
    Removed(&'a proto::FriendRemoved),
}

pub fn is_accepted(contact: &Contact) -> bool {
    contact.friend_state == FriendState::Friends || contact.conversation == Conversation::Accepted
}

pub fn incoming_message(contact: &Contact, privacy: &PrivacySettings) -> IncomingMessage {
    if contact.blocked {
        IncomingMessage::Drop
    } else if is_accepted(contact) {
        IncomingMessage::Conversation
    } else if !privacy.allow_message_requests {
        IncomingMessage::Drop
    } else {
        IncomingMessage::Request
    }
}

pub fn incoming_request(contact: &Contact, privacy: &PrivacySettings) -> IncomingRequest {
    if contact.blocked || !privacy.allow_friend_requests {
        IncomingRequest::Drop
    } else if contact.friend_state == FriendState::Friends {
        IncomingRequest::AutoAccept
    } else if contact.friend_state == FriendState::Outgoing {
        IncomingRequest::CrossedAccept
    } else if contact.declines >= SPAM_AFTER_DECLINES {
        IncomingRequest::Spam
    } else {
        IncomingRequest::Pending
    }
}

pub fn after_my_request(contact: &mut Contact, server_id: &str, request_id: i64) {
    contact.friend_state = FriendState::Outgoing;
    contact.request = Some(RequestRef {
        server_id: server_id.to_string(),
        request_id,
        spam: false,
    });
    if contact.conversation == Conversation::None {
        contact.conversation = Conversation::Accepted;
    }
}

pub fn after_my_answer(contact: &mut Contact, accept: bool, server_id: &str) {
    if accept {
        contact.friend_state = FriendState::Friends;
        contact.friend_server_id = Some(server_id.to_string());
        contact.conversation = Conversation::Accepted;
    } else {
        contact.friend_state = FriendState::None;
        contact.declines = contact.declines.saturating_add(1);
    }
    contact.request = None;
}

pub fn after_my_cancel(contact: &mut Contact) {
    contact.friend_state = FriendState::None;
    contact.request = None;
}

pub fn after_block(contact: &mut Contact) {
    contact.blocked = true;
    contact.friend_state = FriendState::None;
    contact.request = None;
}

fn row_to_contact(row: &sqlx::sqlite::SqliteRow, sealer: &Sealer) -> Result<Contact, PrivateError> {
    let public_key_raw: Vec<u8> = row.try_get("public_key")?;
    let public_key: [u8; 32] = public_key_raw
        .as_slice()
        .try_into()
        .map_err(|_| PrivateError::Storage("stored public_key is not 32 bytes".into()))?;

    let name_sealed: Option<Vec<u8>> = row.try_get("name")?;
    let name = match name_sealed {
        Some(sealed) => {
            let unsealed = sealer.open(CONTACT_NAME_LABEL, &public_key, &sealed)?;
            let text = String::from_utf8(unsealed)
                .map_err(|_| PrivateError::Invalid("sealed name is not utf-8".into()))?;
            Some(text)
        }
        None => None,
    };

    let friend_state_raw: i64 = row.try_get("friend_state")?;
    let friend_state = match friend_state_raw {
        1 => FriendState::Outgoing,
        2 => FriendState::Incoming,
        3 => FriendState::Friends,
        _ => FriendState::None,
    };

    let request_server_id: Option<String> = row.try_get("request_server_id")?;
    let request_id: Option<i64> = row.try_get("request_id")?;
    let request_spam: i64 = row.try_get("request_spam")?;
    let request = match (request_server_id, request_id) {
        (Some(server_id), Some(request_id)) => Some(RequestRef {
            server_id,
            request_id,
            spam: request_spam != 0,
        }),
        _ => None,
    };

    let friend_server_id: Option<String> = row.try_get("friend_server_id")?;
    let blocked: i64 = row.try_get("blocked")?;
    let declines: i64 = row.try_get("declines")?;

    let conversation_raw: i64 = row.try_get("conversation")?;
    let conversation = match conversation_raw {
        1 => Conversation::Request,
        2 => Conversation::Accepted,
        _ => Conversation::None,
    };

    let relay_server_id: Option<String> = row.try_get("relay_server_id")?;
    let their_slip_raw: Option<Vec<u8>> = row.try_get("their_slip")?;
    let their_slip = match their_slip_raw {
        Some(raw) => Some(
            raw.as_slice()
                .try_into()
                .map_err(|_| PrivateError::Storage("stored their_slip is not 64 bytes".into()))?,
        ),
        None => None,
    };

    let last_activity_at: Option<i64> = row.try_get("last_activity_at")?;
    let last_read_at: i64 = row.try_get("last_read_at")?;
    let muted: i64 = row.try_get("muted")?;
    let hidden: i64 = row.try_get("hidden")?;

    Ok(Contact {
        public_key,
        name,
        friend_state,
        request,
        friend_server_id,
        blocked: blocked != 0,
        declines: declines as u32,
        conversation,
        relay_server_id,
        their_slip,
        last_activity_at,
        last_read_at,
        muted: muted != 0,
        hidden: hidden != 0,
    })
}

pub async fn load(
    conn: &mut SqliteConnection,
    sealer: &Sealer,
    public_key: [u8; 32],
) -> Result<Option<Contact>, PrivateError> {
    let row = sqlx::query(
        "SELECT public_key, name, friend_state, request_server_id, request_id, request_spam, \
         friend_server_id, blocked, declines, conversation, relay_server_id, their_slip, \
         last_activity_at, last_read_at, muted, hidden \
         FROM contacts WHERE public_key = $1",
    )
    .bind(&public_key[..])
    .fetch_optional(&mut *conn)
    .await?;

    match row {
        Some(row) => Ok(Some(row_to_contact(&row, sealer)?)),
        None => Ok(None),
    }
}

pub async fn load_all(
    conn: &mut SqliteConnection,
    sealer: &Sealer,
) -> Result<Vec<Contact>, PrivateError> {
    let rows = sqlx::query(
        "SELECT public_key, name, friend_state, request_server_id, request_id, request_spam, \
         friend_server_id, blocked, declines, conversation, relay_server_id, their_slip, \
         last_activity_at, last_read_at, muted, hidden \
         FROM contacts ORDER BY public_key ASC",
    )
    .fetch_all(&mut *conn)
    .await?;

    let mut contacts = Vec::with_capacity(rows.len());
    for row in rows {
        contacts.push(row_to_contact(&row, sealer)?);
    }
    Ok(contacts)
}

pub async fn save(
    conn: &mut SqliteConnection,
    sealer: &Sealer,
    contact: &Contact,
    now_ms: i64,
) -> Result<(), PrivateError> {
    let sealed_name = contact
        .name
        .as_deref()
        .map(|name| sealer.seal(CONTACT_NAME_LABEL, &contact.public_key, name.as_bytes()));

    let (req_server, req_id, req_spam) = match &contact.request {
        Some(r) => (
            Some(r.server_id.as_str()),
            Some(r.request_id),
            if r.spam { 1i64 } else { 0i64 },
        ),
        None => (None, None, 0i64),
    };

    let slip_slice = contact.their_slip.as_ref().map(|s| &s[..]);

    sqlx::query(
        "INSERT INTO contacts (
            public_key, name, friend_state, request_server_id, request_id, request_spam,
            friend_server_id, blocked, declines, conversation, relay_server_id, their_slip,
            last_activity_at, last_read_at, muted, hidden, updated_at
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17
        )
        ON CONFLICT (public_key) DO UPDATE SET
            name = excluded.name,
            friend_state = excluded.friend_state,
            request_server_id = excluded.request_server_id,
            request_id = excluded.request_id,
            request_spam = excluded.request_spam,
            friend_server_id = excluded.friend_server_id,
            blocked = excluded.blocked,
            declines = excluded.declines,
            conversation = excluded.conversation,
            relay_server_id = excluded.relay_server_id,
            their_slip = excluded.their_slip,
            last_activity_at = excluded.last_activity_at,
            last_read_at = excluded.last_read_at,
            muted = excluded.muted,
            hidden = excluded.hidden,
            updated_at = excluded.updated_at",
    )
    .bind(&contact.public_key[..])
    .bind(sealed_name)
    .bind(contact.friend_state as i64)
    .bind(req_server)
    .bind(req_id)
    .bind(req_spam)
    .bind(contact.friend_server_id.as_deref())
    .bind(if contact.blocked { 1i64 } else { 0i64 })
    .bind(contact.declines as i64)
    .bind(contact.conversation as i64)
    .bind(contact.relay_server_id.as_deref())
    .bind(slip_slice)
    .bind(contact.last_activity_at)
    .bind(contact.last_read_at)
    .bind(if contact.muted { 1i64 } else { 0i64 })
    .bind(if contact.hidden { 1i64 } else { 0i64 })
    .bind(now_ms)
    .execute(&mut *conn)
    .await?;

    Ok(())
}

pub async fn delete(conn: &mut SqliteConnection, public_key: [u8; 32]) -> Result<(), PrivateError> {
    sqlx::query("DELETE FROM contacts WHERE public_key = $1")
        .bind(&public_key[..])
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn privacy(conn: &mut SqliteConnection) -> Result<PrivacySettings, PrivateError> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT key, value FROM settings WHERE key IN ($1, $2)")
            .bind(PRIVACY_FRIEND_REQUESTS)
            .bind(PRIVACY_MESSAGE_REQUESTS)
            .fetch_all(&mut *conn)
            .await?;

    let mut settings = PrivacySettings::default();
    for (k, v) in rows {
        if k == PRIVACY_FRIEND_REQUESTS {
            settings.allow_friend_requests = v != "0";
        } else if k == PRIVACY_MESSAGE_REQUESTS {
            settings.allow_message_requests = v != "0";
        }
    }
    Ok(settings)
}

pub async fn set_privacy(
    conn: &mut SqliteConnection,
    settings: &PrivacySettings,
) -> Result<(), PrivateError> {
    let friend_val = if settings.allow_friend_requests {
        "1"
    } else {
        "0"
    };
    let msg_val = if settings.allow_message_requests {
        "1"
    } else {
        "0"
    };

    sqlx::query(
        "INSERT INTO settings (key, value) VALUES ($1, $2) \
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
    )
    .bind(PRIVACY_FRIEND_REQUESTS)
    .bind(friend_val)
    .execute(&mut *conn)
    .await?;

    sqlx::query(
        "INSERT INTO settings (key, value) VALUES ($1, $2) \
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
    )
    .bind(PRIVACY_MESSAGE_REQUESTS)
    .bind(msg_val)
    .execute(&mut *conn)
    .await?;

    Ok(())
}

fn user_key(user: Option<&proto::User>) -> Option<[u8; 32]> {
    let user = user?;
    user.public_key.as_slice().try_into().ok()
}

fn update_name(contact: &mut Contact, user: &proto::User) {
    if !user.display_name.is_empty() {
        contact.name = Some(user.display_name.clone());
    }
}

pub async fn apply_friend_event(
    conn: &mut SqliteConnection,
    sealer: &Sealer,
    me: &[u8; 32],
    server_id: &str,
    event: FriendEvent<'_>,
    privacy: &PrivacySettings,
    now_ms: i64,
) -> Result<FriendOutcome, PrivateError> {
    let none_outcome = FriendOutcome {
        changed: None,
        new_request: None,
        follow_ups: Vec::new(),
    };

    match event {
        FriendEvent::Received(ev) => {
            let Some(requester_key) = user_key(ev.from.as_ref()) else {
                return Ok(none_outcome);
            };
            if requester_key == *me {
                return Ok(none_outcome);
            }
            let mut contact = load(conn, sealer, requester_key)
                .await?
                .unwrap_or_else(|| Contact::new(requester_key));

            update_name(&mut contact, ev.from.as_ref().unwrap());

            if ev.slip.len() == 64 && verify_slip(&requester_key, me, &ev.slip) {
                let mut slip_arr = [0u8; 64];
                slip_arr.copy_from_slice(&ev.slip);
                contact.their_slip = Some(slip_arr);
            }

            match incoming_request(&contact, privacy) {
                IncomingRequest::Drop => Ok(none_outcome),
                IncomingRequest::AutoAccept => {
                    save(conn, sealer, &contact, now_ms).await?;
                    Ok(FriendOutcome {
                        changed: Some(contact),
                        new_request: None,
                        follow_ups: vec![FollowUp::Answer {
                            server_id: server_id.to_string(),
                            request_id: ev.request_id,
                            accept: true,
                        }],
                    })
                }
                IncomingRequest::CrossedAccept => {
                    let cancel_follow_up = contact.request.as_ref().map(|r| FollowUp::Cancel {
                        server_id: r.server_id.clone(),
                        request_id: r.request_id,
                    });
                    save(conn, sealer, &contact, now_ms).await?;
                    let mut follow_ups = vec![FollowUp::Answer {
                        server_id: server_id.to_string(),
                        request_id: ev.request_id,
                        accept: true,
                    }];
                    if let Some(cancel) = cancel_follow_up {
                        follow_ups.push(cancel);
                    }
                    Ok(FriendOutcome {
                        changed: Some(contact),
                        new_request: None,
                        follow_ups,
                    })
                }
                IncomingRequest::Spam => {
                    contact.friend_state = FriendState::Incoming;
                    contact.request = Some(RequestRef {
                        server_id: server_id.to_string(),
                        request_id: ev.request_id,
                        spam: true,
                    });
                    save(conn, sealer, &contact, now_ms).await?;
                    Ok(FriendOutcome {
                        changed: Some(contact),
                        new_request: Some(true),
                        follow_ups: Vec::new(),
                    })
                }
                IncomingRequest::Pending => {
                    contact.friend_state = FriendState::Incoming;
                    contact.request = Some(RequestRef {
                        server_id: server_id.to_string(),
                        request_id: ev.request_id,
                        spam: false,
                    });
                    save(conn, sealer, &contact, now_ms).await?;
                    Ok(FriendOutcome {
                        changed: Some(contact),
                        new_request: Some(false),
                        follow_ups: Vec::new(),
                    })
                }
            }
        }
        FriendEvent::Sent(ev) => {
            let Some(target_key) = user_key(ev.to.as_ref()) else {
                return Ok(none_outcome);
            };
            if target_key == *me {
                return Ok(none_outcome);
            }
            let mut contact = load(conn, sealer, target_key)
                .await?
                .unwrap_or_else(|| Contact::new(target_key));

            update_name(&mut contact, ev.to.as_ref().unwrap());
            after_my_request(&mut contact, server_id, ev.request_id);
            save(conn, sealer, &contact, now_ms).await?;

            Ok(FriendOutcome {
                changed: Some(contact),
                new_request: None,
                follow_ups: Vec::new(),
            })
        }
        FriendEvent::Answered(ev) => {
            let Some(requester_key) = user_key(ev.requester.as_ref()) else {
                return Ok(none_outcome);
            };
            let Some(answerer_key) = user_key(ev.answerer.as_ref()) else {
                return Ok(none_outcome);
            };

            if *me == requester_key {
                // This device is the requester
                let mut contact = load(conn, sealer, answerer_key)
                    .await?
                    .unwrap_or_else(|| Contact::new(answerer_key));

                let matches = contact.friend_state == FriendState::Outgoing
                    && contact
                        .request
                        .as_ref()
                        .is_some_and(|r| r.server_id == server_id && r.request_id == ev.request_id);

                if matches {
                    update_name(&mut contact, ev.answerer.as_ref().unwrap());
                    if ev.accepted {
                        contact.friend_state = FriendState::Friends;
                        contact.friend_server_id = Some(server_id.to_string());
                        contact.conversation = Conversation::Accepted;
                        contact.request = None;
                        if ev.slip.len() == 64 && verify_slip(&answerer_key, me, &ev.slip) {
                            let mut slip_arr = [0u8; 64];
                            slip_arr.copy_from_slice(&ev.slip);
                            contact.their_slip = Some(slip_arr);
                        }
                    } else {
                        contact.friend_state = FriendState::None;
                        contact.request = None;
                    }
                    save(conn, sealer, &contact, now_ms).await?;
                    Ok(FriendOutcome {
                        changed: Some(contact),
                        new_request: None,
                        follow_ups: Vec::new(),
                    })
                } else {
                    Ok(none_outcome)
                }
            } else if *me == answerer_key {
                // This device is the answerer's other device
                let mut contact = load(conn, sealer, requester_key)
                    .await?
                    .unwrap_or_else(|| Contact::new(requester_key));

                let matches = contact.friend_state == FriendState::Incoming
                    && contact
                        .request
                        .as_ref()
                        .is_some_and(|r| r.server_id == server_id && r.request_id == ev.request_id);

                if matches {
                    update_name(&mut contact, ev.requester.as_ref().unwrap());
                    after_my_answer(&mut contact, ev.accepted, server_id);
                    save(conn, sealer, &contact, now_ms).await?;
                    Ok(FriendOutcome {
                        changed: Some(contact),
                        new_request: None,
                        follow_ups: Vec::new(),
                    })
                } else {
                    Ok(none_outcome)
                }
            } else {
                Ok(none_outcome)
            }
        }
        FriendEvent::Cancelled(ev) => {
            let Some(requester_key) = user_key(ev.requester.as_ref()) else {
                return Ok(none_outcome);
            };
            let Some(target_key) = user_key(ev.target.as_ref()) else {
                return Ok(none_outcome);
            };

            if *me == target_key {
                // This device is the target
                let mut contact = load(conn, sealer, requester_key)
                    .await?
                    .unwrap_or_else(|| Contact::new(requester_key));

                let matches = contact.friend_state == FriendState::Incoming
                    && contact
                        .request
                        .as_ref()
                        .is_some_and(|r| r.server_id == server_id && r.request_id == ev.request_id);

                if matches {
                    update_name(&mut contact, ev.requester.as_ref().unwrap());
                    contact.friend_state = FriendState::None;
                    contact.request = None;
                    save(conn, sealer, &contact, now_ms).await?;
                    Ok(FriendOutcome {
                        changed: Some(contact),
                        new_request: None,
                        follow_ups: Vec::new(),
                    })
                } else {
                    Ok(none_outcome)
                }
            } else if *me == requester_key {
                // This device is the requester's other device
                let mut contact = load(conn, sealer, target_key)
                    .await?
                    .unwrap_or_else(|| Contact::new(target_key));

                let matches = contact.friend_state == FriendState::Outgoing
                    && contact
                        .request
                        .as_ref()
                        .is_some_and(|r| r.server_id == server_id && r.request_id == ev.request_id);

                if matches {
                    update_name(&mut contact, ev.target.as_ref().unwrap());
                    contact.friend_state = FriendState::None;
                    contact.request = None;
                    save(conn, sealer, &contact, now_ms).await?;
                    Ok(FriendOutcome {
                        changed: Some(contact),
                        new_request: None,
                        follow_ups: Vec::new(),
                    })
                } else {
                    Ok(none_outcome)
                }
            } else {
                Ok(none_outcome)
            }
        }
        FriendEvent::Removed(ev) => {
            let Some(remover_key) = user_key(ev.remover.as_ref()) else {
                return Ok(none_outcome);
            };
            let Some(removed_key) = user_key(ev.removed.as_ref()) else {
                return Ok(none_outcome);
            };

            let (peer_key, peer_user) = if *me == removed_key {
                (remover_key, ev.remover.as_ref().unwrap())
            } else if *me == remover_key {
                (removed_key, ev.removed.as_ref().unwrap())
            } else {
                return Ok(none_outcome);
            };

            let mut contact = load(conn, sealer, peer_key)
                .await?
                .unwrap_or_else(|| Contact::new(peer_key));

            if contact.friend_state == FriendState::Friends {
                update_name(&mut contact, peer_user);
                contact.friend_state = FriendState::None;
                save(conn, sealer, &contact, now_ms).await?;
                Ok(FriendOutcome {
                    changed: Some(contact),
                    new_request: None,
                    follow_ups: Vec::new(),
                })
            } else {
                Ok(none_outcome)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Identity;
    use crate::private::store::PrivateStore;

    const SERVER: &str = "srv-1";

    fn test_user(id: &Identity, name: &str) -> proto::User {
        proto::User {
            id: 1,
            public_key: id.public_key().to_vec(),
            display_name: name.to_string(),
            avatar_hash: String::new(),
        }
    }

    #[test]
    fn incoming_messages_follow_blocks_acceptance_and_privacy() {
        let key = [1u8; 32];
        let mut contact = Contact::new(key);
        let mut settings = PrivacySettings::default();

        // 1. Blocked -> Drop
        contact.blocked = true;
        assert_eq!(incoming_message(&contact, &settings), IncomingMessage::Drop);

        // 2. Unblocked and is_accepted -> Conversation
        contact.blocked = false;
        contact.friend_state = FriendState::Friends;
        assert_eq!(
            incoming_message(&contact, &settings),
            IncomingMessage::Conversation
        );

        contact.friend_state = FriendState::None;
        contact.conversation = Conversation::Accepted;
        assert_eq!(
            incoming_message(&contact, &settings),
            IncomingMessage::Conversation
        );

        // 3. Not accepted and message requests off -> Drop
        contact.conversation = Conversation::None;
        settings.allow_message_requests = false;
        assert_eq!(incoming_message(&contact, &settings), IncomingMessage::Drop);

        // 4. Message requests on -> Request
        settings.allow_message_requests = true;
        assert_eq!(
            incoming_message(&contact, &settings),
            IncomingMessage::Request
        );
    }

    #[test]
    fn incoming_requests_follow_blocks_privacy_friendship_crossing_and_spam() {
        let key = [2u8; 32];
        let mut contact = Contact::new(key);
        let mut settings = PrivacySettings::default();

        // 1. Blocked -> Drop
        contact.blocked = true;
        assert_eq!(incoming_request(&contact, &settings), IncomingRequest::Drop);

        // Friend requests off -> Drop
        contact.blocked = false;
        settings.allow_friend_requests = false;
        assert_eq!(incoming_request(&contact, &settings), IncomingRequest::Drop);

        // 2. Friends -> AutoAccept
        settings.allow_friend_requests = true;
        contact.friend_state = FriendState::Friends;
        assert_eq!(
            incoming_request(&contact, &settings),
            IncomingRequest::AutoAccept
        );

        // 3. Outgoing -> CrossedAccept
        contact.friend_state = FriendState::Outgoing;
        assert_eq!(
            incoming_request(&contact, &settings),
            IncomingRequest::CrossedAccept
        );

        // 4. Declines >= 3 -> Spam
        contact.friend_state = FriendState::None;
        contact.declines = 3;
        assert_eq!(incoming_request(&contact, &settings), IncomingRequest::Spam);

        // 5. Declines < 3 -> Pending
        contact.declines = 2;
        assert_eq!(
            incoming_request(&contact, &settings),
            IncomingRequest::Pending
        );
    }

    #[tokio::test]
    async fn a_received_request_is_pending_and_keeps_a_valid_slip() {
        let dir = tempfile::tempdir().unwrap();
        let alice = Identity::generate();
        let bob = Identity::generate();
        let opened = PrivateStore::open(dir.path(), &alice.public_key(), &[1; 32], 1_000)
            .await
            .unwrap();
        let mut conn = opened.store.pool().acquire().await.unwrap();
        let sealer = opened.store.sealer();
        let privacy = PrivacySettings::default();

        let valid_slip = bob.slip_for(&alice.public_key());
        let received = proto::FriendRequestReceived {
            request_id: 101,
            from: Some(test_user(&bob, "Bob")),
            slip: valid_slip.to_vec(),
        };

        let outcome = apply_friend_event(
            &mut conn,
            sealer,
            &alice.public_key(),
            SERVER,
            FriendEvent::Received(&received),
            &privacy,
            1_000,
        )
        .await
        .unwrap();

        assert_eq!(outcome.new_request, Some(false));
        let changed = outcome.changed.expect("contact changed");
        assert_eq!(changed.friend_state, FriendState::Incoming);
        assert_eq!(changed.their_slip, Some(valid_slip));
        assert_eq!(changed.name.as_deref(), Some("Bob"));

        // Invalid slip isn't kept
        let received_bad_slip = proto::FriendRequestReceived {
            request_id: 102,
            from: Some(test_user(&bob, "Bob")),
            slip: vec![0u8; 64],
        };
        let contact = Contact::new(bob.public_key());
        save(&mut conn, sealer, &contact, 1_000).await.unwrap();

        let outcome = apply_friend_event(
            &mut conn,
            sealer,
            &alice.public_key(),
            SERVER,
            FriendEvent::Received(&received_bad_slip),
            &privacy,
            1_000,
        )
        .await
        .unwrap();
        let changed = outcome.changed.expect("contact changed");
        assert_eq!(changed.their_slip, None);

        drop(conn);
        opened.store.close().await;
    }

    #[tokio::test]
    async fn the_third_decline_makes_the_next_request_spam() {
        let dir = tempfile::tempdir().unwrap();
        let alice = Identity::generate();
        let bob = Identity::generate();
        let opened = PrivateStore::open(dir.path(), &alice.public_key(), &[1; 32], 1_000)
            .await
            .unwrap();
        let mut conn = opened.store.pool().acquire().await.unwrap();
        let sealer = opened.store.sealer();
        let privacy = PrivacySettings::default();

        let mut contact = Contact::new(bob.public_key());
        contact.declines = 2;
        save(&mut conn, sealer, &contact, 1_000).await.unwrap();

        let slip = bob.slip_for(&alice.public_key());
        let received = proto::FriendRequestReceived {
            request_id: 201,
            from: Some(test_user(&bob, "Bob")),
            slip: slip.to_vec(),
        };

        // At 2 declines: still pending (not spam)
        let outcome = apply_friend_event(
            &mut conn,
            sealer,
            &alice.public_key(),
            SERVER,
            FriendEvent::Received(&received),
            &privacy,
            1_000,
        )
        .await
        .unwrap();
        assert_eq!(outcome.new_request, Some(false));

        // Now decline it
        let mut c = outcome.changed.unwrap();
        after_my_answer(&mut c, false, SERVER);
        assert_eq!(c.declines, 3);
        save(&mut conn, sealer, &c, 1_000).await.unwrap();

        // 3rd decline makes subsequent request spam
        let received2 = proto::FriendRequestReceived {
            request_id: 202,
            from: Some(test_user(&bob, "Bob")),
            slip: slip.to_vec(),
        };
        let outcome2 = apply_friend_event(
            &mut conn,
            sealer,
            &alice.public_key(),
            SERVER,
            FriendEvent::Received(&received2),
            &privacy,
            1_000,
        )
        .await
        .unwrap();
        assert_eq!(outcome2.new_request, Some(true));
        assert!(outcome2.changed.unwrap().request.unwrap().spam);

        drop(conn);
        opened.store.close().await;
    }

    #[tokio::test]
    async fn an_accept_makes_friends_on_the_requesters_side_and_on_the_answerers_other_device() {
        let dir = tempfile::tempdir().unwrap();
        let alice = Identity::generate();
        let bob = Identity::generate();
        let opened = PrivateStore::open(dir.path(), &alice.public_key(), &[1; 32], 1_000)
            .await
            .unwrap();
        let mut conn = opened.store.pool().acquire().await.unwrap();
        let sealer = opened.store.sealer();
        let privacy = PrivacySettings::default();

        // 1. Requester's side (Alice sent to Bob, Bob accepted)
        let mut contact = Contact::new(bob.public_key());
        after_my_request(&mut contact, SERVER, 301);
        save(&mut conn, sealer, &contact, 1_000).await.unwrap();

        let bob_slip = bob.slip_for(&alice.public_key());
        let answered = proto::FriendRequestAnswered {
            request_id: 301,
            requester: Some(test_user(&alice, "Alice")),
            answerer: Some(test_user(&bob, "Bob")),
            accepted: true,
            slip: bob_slip.to_vec(),
        };

        let outcome = apply_friend_event(
            &mut conn,
            sealer,
            &alice.public_key(),
            SERVER,
            FriendEvent::Answered(&answered),
            &privacy,
            1_000,
        )
        .await
        .unwrap();

        let c = outcome.changed.expect("requester changed");
        assert_eq!(c.friend_state, FriendState::Friends);
        assert_eq!(c.friend_server_id.as_deref(), Some(SERVER));
        assert_eq!(c.conversation, Conversation::Accepted);
        assert_eq!(c.their_slip, Some(bob_slip));

        // 2. Answerer's other device (Bob's device receives Answered { requester: Alice, answerer: Bob })
        let mut bob_contact = Contact::new(alice.public_key());
        bob_contact.friend_state = FriendState::Incoming;
        bob_contact.request = Some(RequestRef {
            server_id: SERVER.to_string(),
            request_id: 301,
            spam: false,
        });
        save(&mut conn, sealer, &bob_contact, 1_000).await.unwrap();

        let outcome = apply_friend_event(
            &mut conn,
            sealer,
            &bob.public_key(),
            SERVER,
            FriendEvent::Answered(&answered),
            &privacy,
            1_000,
        )
        .await
        .unwrap();

        let c2 = outcome.changed.expect("answerer other device changed");
        assert_eq!(c2.friend_state, FriendState::Friends);
        assert_eq!(c2.friend_server_id.as_deref(), Some(SERVER));
        assert_eq!(c2.conversation, Conversation::Accepted);

        drop(conn);
        opened.store.close().await;
    }

    #[tokio::test]
    async fn a_decline_is_silent_for_the_requester() {
        let dir = tempfile::tempdir().unwrap();
        let alice = Identity::generate();
        let bob = Identity::generate();
        let opened = PrivateStore::open(dir.path(), &alice.public_key(), &[1; 32], 1_000)
            .await
            .unwrap();
        let mut conn = opened.store.pool().acquire().await.unwrap();
        let sealer = opened.store.sealer();
        let privacy = PrivacySettings::default();

        let mut contact = Contact::new(bob.public_key());
        after_my_request(&mut contact, SERVER, 401);
        save(&mut conn, sealer, &contact, 1_000).await.unwrap();

        let answered = proto::FriendRequestAnswered {
            request_id: 401,
            requester: Some(test_user(&alice, "Alice")),
            answerer: Some(test_user(&bob, "Bob")),
            accepted: false,
            slip: Vec::new(),
        };

        let outcome = apply_friend_event(
            &mut conn,
            sealer,
            &alice.public_key(),
            SERVER,
            FriendEvent::Answered(&answered),
            &privacy,
            1_000,
        )
        .await
        .unwrap();

        let c = outcome.changed.expect("contact cleared");
        assert_eq!(c.friend_state, FriendState::None);
        assert_eq!(c.request, None);
        assert_eq!(c.declines, 0); // requester declines count unchanged
        assert!(outcome.follow_ups.is_empty());
        assert_eq!(outcome.new_request, None);

        drop(conn);
        opened.store.close().await;
    }

    #[tokio::test]
    async fn an_answer_for_another_request_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let alice = Identity::generate();
        let bob = Identity::generate();
        let opened = PrivateStore::open(dir.path(), &alice.public_key(), &[1; 32], 1_000)
            .await
            .unwrap();
        let mut conn = opened.store.pool().acquire().await.unwrap();
        let sealer = opened.store.sealer();
        let privacy = PrivacySettings::default();

        let mut contact = Contact::new(bob.public_key());
        after_my_request(&mut contact, SERVER, 501);
        save(&mut conn, sealer, &contact, 1_000).await.unwrap();

        // Answered has request_id 999 (not 501)
        let answered = proto::FriendRequestAnswered {
            request_id: 999,
            requester: Some(test_user(&alice, "Alice")),
            answerer: Some(test_user(&bob, "Bob")),
            accepted: true,
            slip: Vec::new(),
        };

        let outcome = apply_friend_event(
            &mut conn,
            sealer,
            &alice.public_key(),
            SERVER,
            FriendEvent::Answered(&answered),
            &privacy,
            1_000,
        )
        .await
        .unwrap();

        assert!(outcome.changed.is_none());

        drop(conn);
        opened.store.close().await;
    }

    #[tokio::test]
    async fn cancellations_and_removals_only_undo_what_exists() {
        let dir = tempfile::tempdir().unwrap();
        let alice = Identity::generate();
        let bob = Identity::generate();
        let opened = PrivateStore::open(dir.path(), &alice.public_key(), &[1; 32], 1_000)
            .await
            .unwrap();
        let mut conn = opened.store.pool().acquire().await.unwrap();
        let sealer = opened.store.sealer();
        let privacy = PrivacySettings::default();

        // Cancel on non-existent request changes nothing
        let cancelled = proto::FriendRequestCancelled {
            request_id: 601,
            requester: Some(test_user(&bob, "Bob")),
            target: Some(test_user(&alice, "Alice")),
        };
        let outcome = apply_friend_event(
            &mut conn,
            sealer,
            &alice.public_key(),
            SERVER,
            FriendEvent::Cancelled(&cancelled),
            &privacy,
            1_000,
        )
        .await
        .unwrap();
        assert!(outcome.changed.is_none());

        // Removal on non-friends changes nothing
        let removed = proto::FriendRemoved {
            remover: Some(test_user(&bob, "Bob")),
            removed: Some(test_user(&alice, "Alice")),
        };
        let outcome = apply_friend_event(
            &mut conn,
            sealer,
            &alice.public_key(),
            SERVER,
            FriendEvent::Removed(&removed),
            &privacy,
            1_000,
        )
        .await
        .unwrap();
        assert!(outcome.changed.is_none());

        // Cancel when request exists clears it
        let mut contact = Contact::new(bob.public_key());
        contact.friend_state = FriendState::Incoming;
        contact.request = Some(RequestRef {
            server_id: SERVER.to_string(),
            request_id: 601,
            spam: false,
        });
        save(&mut conn, sealer, &contact, 1_000).await.unwrap();

        let outcome = apply_friend_event(
            &mut conn,
            sealer,
            &alice.public_key(),
            SERVER,
            FriendEvent::Cancelled(&cancelled),
            &privacy,
            1_000,
        )
        .await
        .unwrap();
        let c = outcome.changed.expect("cancelled changed");
        assert_eq!(c.friend_state, FriendState::None);
        assert_eq!(c.request, None);

        // Removal when Friends clears friend_state
        contact.friend_state = FriendState::Friends;
        save(&mut conn, sealer, &contact, 1_000).await.unwrap();

        let outcome = apply_friend_event(
            &mut conn,
            sealer,
            &alice.public_key(),
            SERVER,
            FriendEvent::Removed(&removed),
            &privacy,
            1_000,
        )
        .await
        .unwrap();
        let c = outcome.changed.expect("removed changed");
        assert_eq!(c.friend_state, FriendState::None);

        drop(conn);
        opened.store.close().await;
    }

    #[tokio::test]
    async fn crossed_requests_ask_for_an_accept_and_a_cancel() {
        let dir = tempfile::tempdir().unwrap();
        let alice = Identity::generate();
        let bob = Identity::generate();
        let opened = PrivateStore::open(dir.path(), &alice.public_key(), &[1; 32], 1_000)
            .await
            .unwrap();
        let mut conn = opened.store.pool().acquire().await.unwrap();
        let sealer = opened.store.sealer();
        let privacy = PrivacySettings::default();

        // Alice sent a request to Bob
        let mut contact = Contact::new(bob.public_key());
        after_my_request(&mut contact, SERVER, 701);
        save(&mut conn, sealer, &contact, 1_000).await.unwrap();

        // Alice receives Bob's request
        let bob_slip = bob.slip_for(&alice.public_key());
        let received = proto::FriendRequestReceived {
            request_id: 702,
            from: Some(test_user(&bob, "Bob")),
            slip: bob_slip.to_vec(),
        };

        let outcome = apply_friend_event(
            &mut conn,
            sealer,
            &alice.public_key(),
            SERVER,
            FriendEvent::Received(&received),
            &privacy,
            1_000,
        )
        .await
        .unwrap();

        assert_eq!(
            outcome.follow_ups,
            vec![
                FollowUp::Answer {
                    server_id: SERVER.to_string(),
                    request_id: 702,
                    accept: true,
                },
                FollowUp::Cancel {
                    server_id: SERVER.to_string(),
                    request_id: 701,
                },
            ]
        );

        drop(conn);
        opened.store.close().await;
    }

    #[tokio::test]
    async fn names_are_sealed_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let alice = Identity::generate();
        let bob = Identity::generate();
        let opened = PrivateStore::open(dir.path(), &alice.public_key(), &[1; 32], 1_000)
            .await
            .unwrap();
        let mut conn = opened.store.pool().acquire().await.unwrap();
        let sealer = opened.store.sealer();

        let secret_name = "SuperSecretContactName";
        let mut contact = Contact::new(bob.public_key());
        contact.name = Some(secret_name.to_string());
        save(&mut conn, sealer, &contact, 1_000).await.unwrap();

        // Raw bytes from database do not contain secret_name as plaintext
        let raw_name: Vec<u8> =
            sqlx::query_scalar("SELECT name FROM contacts WHERE public_key = $1")
                .bind(&bob.public_key()[..])
                .fetch_one(&mut *conn)
                .await
                .unwrap();

        assert!(
            !raw_name
                .windows(secret_name.len())
                .any(|w| w == secret_name.as_bytes())
        );

        // Loading via load unseals it properly
        let loaded = load(&mut conn, sealer, bob.public_key())
            .await
            .unwrap()
            .expect("contact exists");
        assert_eq!(loaded.name.as_deref(), Some(secret_name));

        drop(conn);
        opened.store.close().await;
    }

    #[tokio::test]
    async fn privacy_settings_default_to_open_and_persist() {
        let dir = tempfile::tempdir().unwrap();
        let alice = Identity::generate();
        let opened = PrivateStore::open(dir.path(), &alice.public_key(), &[1; 32], 1_000)
            .await
            .unwrap();
        let mut conn = opened.store.pool().acquire().await.unwrap();

        let initial = privacy(&mut conn).await.unwrap();
        assert_eq!(
            initial,
            PrivacySettings {
                allow_friend_requests: true,
                allow_message_requests: true,
            }
        );

        let updated = PrivacySettings {
            allow_friend_requests: false,
            allow_message_requests: true,
        };
        set_privacy(&mut conn, &updated).await.unwrap();

        let reloaded = privacy(&mut conn).await.unwrap();
        assert_eq!(reloaded, updated);

        drop(conn);
        opened.store.close().await;
    }
}
