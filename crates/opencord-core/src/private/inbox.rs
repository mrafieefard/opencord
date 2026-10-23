//! Receiving (design 2026-10-22 §10.5). Each private message the worker
//! hands over is decrypted under the device lock, then saved with the
//! advanced sessions, the account and the event's record in one
//! transaction. Only then is it shown and acknowledged, so a crash before
//! the commit leaves it on the server to decrypt the same way again.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use opencord_common::private::{DEVICE_ID_LEN, KEY_LEN, MESSAGE_ID_LEN, verify_slip};
use opencord_proto::v1 as proto;
use proto::private_payload::Body;
use proto::request::Kind as Request;
use proto::response::Result as Response;
use sqlx::SqliteConnection;

use super::PrivateError;
use super::contacts::{self, Contact, Conversation, IncomingMessage, PrivacySettings};
use super::device::Device;
use super::outbox::{self, DIVIDER, MOVED, RECEIVED, SENT, TEXT, UNDECRYPTABLE, VerifiedDevice};
use super::payload::{self, Parsed};
use super::seal::Sealer;
use super::service::{
    MESSAGE_BODY_LABEL, Open, PrivateService, now_ms, record_received, warn_core,
};
use super::sessions::{Decrypted, PeerSessions};
use crate::api::types::{CoreError, PrivateEvent};

/// At most one session reset per peer device in this long.
const RESET_EVERY: Duration = Duration::from_secs(60 * 60);
/// Olm's prekey message, which may start a session and so use up one of
/// this device's one-time keys.
const PREKEY_MESSAGE: u32 = 0;

/// When a session reset last went to each peer device, by its Curve25519
/// key. An entry leaves once it's an hour old: those are pruned on each
/// insert, so the map never outgrows the devices reset in the last hour.
#[derive(Debug, Default)]
pub struct ResetLimiter {
    sent: Mutex<HashMap<[u8; KEY_LEN], Instant>>,
}

impl ResetLimiter {
    /// Takes the device's one reset this hour: `false` if it went already.
    fn take(&self, device: [u8; KEY_LEN], now: Instant) -> bool {
        let mut sent = self.lock();
        sent.retain(|_, at| now.saturating_duration_since(*at) < RESET_EVERY);
        if sent.contains_key(&device) {
            return false;
        }
        sent.insert(device, now);
        true
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<[u8; KEY_LEN], Instant>> {
        self.sent.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A session reset to send once the event that asked for it is saved.
pub struct Reset {
    server_key: String,
    peer: [u8; KEY_LEN],
    /// The peer device's Curve25519 key.
    device: [u8; KEY_LEN],
}

/// Sends the reset, unless that device had one this hour.
pub async fn reset_session(service: &PrivateService, reset: Reset) -> Result<(), CoreError> {
    if !service.resets().take(reset.device, Instant::now()) {
        return Ok(());
    }
    outbox::reset_session(service, reset.peer, &reset.server_key, reset.device).await
}

/// Where and when an event came from.
struct Arrival<'a> {
    server_id: &'a str,
    event_id: i64,
    /// The server's time.
    sent_at: i64,
    sender: [u8; KEY_LEN],
    /// A copy from another of this person's devices.
    from_me: bool,
}

/// What the plaintext turned out to be.
enum Content {
    /// Decrypted before: nothing to show.
    Duplicate,
    Undecryptable,
    Payload {
        /// The conversation's other person.
        peer: [u8; KEY_LEN],
        parsed: Box<Parsed>,
    },
}

/// What applying it left to show once committed.
#[derive(Default)]
struct Applied {
    shown: Vec<Shown>,
    /// The contact, when it changed.
    contact: Option<Contact>,
    /// An undecryptable line was added for the sender.
    line: bool,
}

enum Shown {
    Added([u8; MESSAGE_ID_LEN]),
    Changed([u8; MESSAGE_ID_LEN]),
    Deleted {
        peer: [u8; KEY_LEN],
        id: [u8; MESSAGE_ID_LEN],
    },
}

/// Steps 2–6 for one `PrivateMessage` from `server_key`. Every `Ok` is
/// acknowledged, and may ask for a session reset; an error leaves the event
/// on the server for the next catch-up.
pub async fn receive(
    service: &PrivateService,
    open: &Open,
    server_key: &str,
    arrival: (&str, i64),
    message: proto::PrivateMessage,
    privacy: &PrivacySettings,
) -> Result<Option<Reset>, PrivateError> {
    let (server_id, event_id) = arrival;
    // 2.
    let sender = message.sender.as_ref();
    let Some(sender_key) =
        sender.and_then(|user| <[u8; KEY_LEN]>::try_from(user.public_key.as_slice()).ok())
    else {
        tracing::warn!(
            server = %server_key,
            event_id,
            "a private message whose sender has no valid key was dropped"
        );
        return Ok(None);
    };
    let sender_id = sender.map_or(0, |user| user.id);
    let me = open.identity();
    let at = Arrival {
        server_id,
        event_id,
        sent_at: message.sent_at_ms,
        sender: sender_key,
        from_me: sender_key == me,
    };
    let device = known_device(
        service,
        open,
        server_key,
        (sender_id, sender_key),
        &message.sender_device_id,
    )
    .await?;
    let Some(device) = device else {
        tracing::warn!(
            server = %server_key,
            event_id,
            "a private message from a device its sender's identity didn't sign"
        );
        let applied = save(open, &at, Content::Undecryptable, privacy, None).await?;
        show(service, open, server_key, applied).await;
        return Ok(None);
    };

    // 3–6.
    let pickle_key = open.store().pickle_key();
    let mut current = open.device().lock().await;
    let mut sessions = {
        let mut conn = open.store().pool().acquire().await?;
        PeerSessions::load(&mut conn, pickle_key, device.curve25519).await?
    };
    // A prekey message may use up a one-time key: that happens on a copy,
    // which replaces the account only once it's saved.
    let mut changed = (message.olm_type == PREKEY_MESSAGE).then(|| current.duplicate());
    let decrypted = sessions.decrypt(
        changed.as_mut().unwrap_or(&mut *current),
        message.olm_type,
        &message.body,
        now_ms(),
    );
    let failed = matches!(decrypted, Decrypted::Failed(_));
    let content = match decrypted {
        Decrypted::Duplicate => Content::Duplicate,
        Decrypted::Failed(reason) => {
            tracing::warn!(
                server = %server_key,
                event_id,
                %reason,
                "a private message couldn't be decrypted"
            );
            Content::Undecryptable
        }
        Decrypted::Plaintext(plaintext) => read(&plaintext, &at, me, server_key),
    };
    let applied = save(
        open,
        &at,
        content,
        privacy,
        Some((&mut sessions, changed.as_ref())),
    )
    .await?;
    if let Some(changed) = changed {
        *current = changed;
    }
    drop(current);

    let reset = (failed && applied.line).then(|| Reset {
        server_key: server_key.to_owned(),
        peer: sender_key,
        device: device.curve25519,
    });
    show(service, open, server_key, applied).await;
    Ok(reset)
}

/// The sender's device among the verified ones, else fetched from the
/// server it came through, checked against the sender's key, and kept.
async fn known_device(
    service: &PrivateService,
    open: &Open,
    server_key: &str,
    (sender_id, sender_key): (i64, [u8; KEY_LEN]),
    device_id: &[u8],
) -> Result<Option<VerifiedDevice>, PrivateError> {
    let Ok(device_id) = <[u8; DEVICE_ID_LEN]>::try_from(device_id) else {
        return Ok(None);
    };
    if let Some(device) = stored_device(open, &sender_key, device_id).await? {
        return Ok(Some(device));
    }
    let response = service
        .request(
            server_key,
            Request::FetchDevices(proto::FetchDevices {
                user_ids: vec![sender_id],
            }),
        )
        .await
        .map_err(PrivateError::Request)?;
    let Response::DeviceLists(lists) = response else {
        return Err(PrivateError::Request(crate::client::unexpected(&response)));
    };
    let this_device = open.device().lock().await.id();
    let devices = outbox::verified(
        server_key,
        outbox::listed(&lists, sender_id),
        &sender_key,
        Some(this_device),
    );
    outbox::remember(open, &sender_key, &devices).await?;
    Ok(devices
        .into_iter()
        .find(|device| device.device_id == device_id))
}

async fn stored_device(
    open: &Open,
    owner: &[u8; KEY_LEN],
    device_id: [u8; DEVICE_ID_LEN],
) -> Result<Option<VerifiedDevice>, PrivateError> {
    let row: Option<(Vec<u8>, Vec<u8>)> = sqlx::query_as(
        "SELECT curve25519, ed25519 FROM peer_devices WHERE public_key = $1 AND device_id = $2",
    )
    .bind(owner.as_slice())
    .bind(device_id.as_slice())
    .fetch_optional(open.store().pool())
    .await?;
    let Some((curve25519, ed25519)) = row else {
        return Ok(None);
    };
    let key = |bytes: Vec<u8>| {
        <[u8; KEY_LEN]>::try_from(bytes.as_slice())
            .map_err(|_| PrivateError::Storage("a stored device key isn't 32 bytes".to_owned()))
    };
    Ok(Some(VerifiedDevice {
        device_id,
        curve25519: key(curve25519)?,
        ed25519: key(ed25519)?,
    }))
}

/// Step 4: the payload, and the conversation it belongs to: the sender's,
/// sent to this person, or for a copy from another of this person's
/// devices, the one it names.
fn read(plaintext: &[u8], at: &Arrival<'_>, me: [u8; KEY_LEN], server_key: &str) -> Content {
    let parsed = match payload::parse(plaintext) {
        Ok(parsed) => parsed,
        Err(error) => {
            tracing::warn!(
                server = %server_key,
                event_id = at.event_id,
                %error,
                "a private message's payload isn't valid"
            );
            return Content::Undecryptable;
        }
    };
    let peer = if at.from_me {
        (parsed.recipient != me).then_some(parsed.recipient)
    } else {
        (parsed.recipient == me).then_some(at.sender)
    };
    match peer {
        Some(peer) => Content::Payload {
            peer,
            parsed: Box::new(parsed),
        },
        None => {
            tracing::warn!(
                server = %server_key,
                event_id = at.event_id,
                "a private message names the wrong recipient"
            );
            Content::Undecryptable
        }
    }
}

/// Step 6: the sessions, the account, the change and the event's record,
/// in one transaction.
async fn save(
    open: &Open,
    at: &Arrival<'_>,
    content: Content,
    privacy: &PrivacySettings,
    crypto: Option<(&mut PeerSessions, Option<&Device>)>,
) -> Result<Applied, PrivateError> {
    let pickle_key = open.store().pickle_key();
    let sealer = open.store().sealer();
    let now = now_ms();
    // It reads the contact, then writes: take the write lock up front.
    let mut tx = open.store().pool().begin_with("BEGIN IMMEDIATE").await?;
    if let Some((sessions, device)) = crypto {
        sessions.save(&mut tx, pickle_key).await?;
        if let Some(device) = device {
            device.save(&mut tx, pickle_key).await?;
        }
    }
    let applied = match content {
        Content::Duplicate => Applied::default(),
        Content::Undecryptable => undecryptable(&mut tx, sealer, at, privacy, now).await?,
        Content::Payload { peer, parsed } if at.from_me => {
            own_copy(&mut tx, sealer, at, peer, *parsed, now).await?
        }
        Content::Payload { parsed, .. } => {
            from_peer(&mut tx, sealer, at, *parsed, open.identity(), privacy, now).await?
        }
    };
    record_received(&mut tx, at.server_id, at.event_id, now).await?;
    tx.commit().await?;
    Ok(applied)
}

/// A "couldn't be decrypted" line in the sender's conversation. A copy from
/// another of this person's devices doesn't say whose conversation it is,
/// so it shows nothing.
async fn undecryptable(
    conn: &mut SqliteConnection,
    sealer: &Sealer,
    at: &Arrival<'_>,
    privacy: &PrivacySettings,
    now: i64,
) -> Result<Applied, PrivateError> {
    if at.from_me {
        tracing::warn!(
            event_id = at.event_id,
            "a copy from another of this person's devices couldn't be read"
        );
        return Ok(Applied::default());
    }
    let mut contact = load_or_new(conn, sealer, at.sender).await?;
    if contacts::incoming_message(&contact, privacy) == IncomingMessage::Drop {
        return Ok(Applied::default());
    }
    let id = random_id()?;
    insert(
        conn,
        sealer,
        Insert {
            id,
            peer: at.sender,
            outgoing: false,
            kind: UNDECRYPTABLE,
            body: None,
            server_id: None,
            sent_at: at.sent_at,
            state: RECEIVED,
        },
    )
    .await?;
    arrived(&mut contact, at.sent_at);
    contacts::save(conn, sealer, &contact, now).await?;
    Ok(Applied {
        shown: vec![Shown::Added(id)],
        contact: Some(contact),
        line: true,
    })
}

/// A payload from the conversation's other person, unless the contact's
/// rules drop what they send.
async fn from_peer(
    conn: &mut SqliteConnection,
    sealer: &Sealer,
    at: &Arrival<'_>,
    parsed: Parsed,
    me: [u8; KEY_LEN],
    privacy: &PrivacySettings,
    now: i64,
) -> Result<Applied, PrivateError> {
    let peer = at.sender;
    let mut contact = load_or_new(conn, sealer, peer).await?;
    if contacts::incoming_message(&contact, privacy) == IncomingMessage::Drop {
        return Ok(Applied::default());
    }
    // 5.
    let mut changed = false;
    if let Some(slip) = parsed.slip
        && contact.their_slip != Some(slip)
        && verify_slip(&peer, &me, &slip)
    {
        contact.their_slip = Some(slip);
        changed = true;
    }
    let id = parsed.message_id;
    let mut shown = Vec::new();
    match parsed.body {
        Body::Text(text) => {
            let new = Insert {
                id,
                peer,
                outgoing: false,
                kind: TEXT,
                body: Some(&text.content),
                server_id: None,
                sent_at: at.sent_at,
                state: RECEIVED,
            };
            if insert(conn, sealer, new).await? {
                shown.push(Shown::Added(id));
                arrived(&mut contact, at.sent_at);
                changed = true;
            }
        }
        Body::Edit(edit) => {
            if let Some(edited) = edit_own(conn, sealer, (peer, false), &edit, at.sent_at).await? {
                shown.push(Shown::Changed(edited));
            }
        }
        Body::Delete(delete) => {
            if let Some(deleted) = delete_own(conn, (peer, false), &delete).await? {
                shown.push(Shown::Deleted { peer, id: deleted });
            }
        }
        Body::RelayMoved(moved) => {
            let server_id = hex::encode(&moved.server_id);
            let card = Insert {
                id,
                peer,
                outgoing: false,
                kind: MOVED,
                body: Some(&moved.server_name),
                server_id: Some(&server_id),
                sent_at: at.sent_at,
                state: RECEIVED,
            };
            if insert(conn, sealer, card).await? {
                shown.push(Shown::Added(id));
            }
        }
        // They accepted, so it's mutual.
        Body::RequestAccepted(_) => {
            if contact.conversation != Conversation::Accepted {
                contact.conversation = Conversation::Accepted;
                changed = true;
            }
        }
        // The new session is saved already.
        Body::SessionReset(_) => {}
    }
    if changed {
        contacts::save(conn, sealer, &contact, now).await?;
    }
    Ok(Applied {
        shown,
        contact: changed.then_some(contact),
        line: false,
    })
}

/// A copy of what this person sent `peer` from another of their devices.
async fn own_copy(
    conn: &mut SqliteConnection,
    sealer: &Sealer,
    at: &Arrival<'_>,
    peer: [u8; KEY_LEN],
    parsed: Parsed,
    now: i64,
) -> Result<Applied, PrivateError> {
    let mut contact = load_or_new(conn, sealer, peer).await?;
    let id = parsed.message_id;
    let mut changed = false;
    let mut shown = Vec::new();
    match parsed.body {
        Body::Text(text) => {
            let new = Insert {
                id,
                peer,
                outgoing: true,
                kind: TEXT,
                body: Some(&text.content),
                server_id: None,
                sent_at: at.sent_at,
                state: SENT,
            };
            if insert(conn, sealer, new).await? {
                shown.push(Shown::Added(id));
            }
            contact.conversation = Conversation::Accepted;
            contact.hidden = false;
            contact.last_activity_at = Some(latest(contact.last_activity_at, at.sent_at));
            changed = true;
        }
        Body::Edit(edit) => {
            if let Some(edited) = edit_own(conn, sealer, (peer, true), &edit, at.sent_at).await? {
                shown.push(Shown::Changed(edited));
            }
        }
        Body::Delete(delete) => {
            if let Some(deleted) = delete_own(conn, (peer, true), &delete).await? {
                shown.push(Shown::Deleted { peer, id: deleted });
            }
        }
        Body::RelayMoved(moved) => {
            let server_id = hex::encode(&moved.server_id);
            let divider = Insert {
                id,
                peer,
                outgoing: true,
                kind: DIVIDER,
                body: Some(&moved.server_name),
                server_id: Some(&server_id),
                sent_at: at.sent_at,
                state: SENT,
            };
            if insert(conn, sealer, divider).await? {
                shown.push(Shown::Added(id));
            }
            contact.relay_server_id = Some(server_id);
            changed = true;
        }
        Body::RequestAccepted(_) => {
            if contact.conversation != Conversation::Accepted {
                contact.conversation = Conversation::Accepted;
                changed = true;
            }
        }
        Body::SessionReset(_) => {}
    }
    if changed {
        contacts::save(conn, sealer, &contact, now).await?;
    }
    Ok(Applied {
        shown,
        contact: changed.then_some(contact),
        line: false,
    })
}

/// Shows what was saved. A view that can't be read is logged: the event is
/// saved, and the app reads the conversation again when it opens it.
async fn show(service: &PrivateService, open: &Open, server_key: &str, applied: Applied) {
    for shown in applied.shown {
        let event = match shown {
            Shown::Added(id) => outbox::view(service, open, id)
                .await
                .map(PrivateEvent::MessageAdded),
            Shown::Changed(id) => outbox::view(service, open, id)
                .await
                .map(PrivateEvent::MessageChanged),
            Shown::Deleted { peer, id } => Ok(PrivateEvent::MessageDeleted {
                peer: hex::encode(peer),
                message_id: hex::encode(id),
            }),
        };
        match event {
            Ok(event) => service.emit(event),
            Err(error) => warn_core("showing a received message failed", server_key, &error),
        }
    }
    if let Some(contact) = applied.contact
        && let Err(error) = service.emit_contact(open, &contact).await
    {
        warn_core(
            "showing a changed contact failed",
            server_key,
            &CoreError::from(error),
        );
    }
}

/// A message to insert; `body` is sealed with its id.
struct Insert<'a> {
    id: [u8; MESSAGE_ID_LEN],
    peer: [u8; KEY_LEN],
    outgoing: bool,
    kind: i64,
    body: Option<&'a str>,
    server_id: Option<&'a str>,
    sent_at: i64,
    state: i64,
}

/// `false` when a message with that id is there already.
async fn insert(
    conn: &mut SqliteConnection,
    sealer: &Sealer,
    message: Insert<'_>,
) -> Result<bool, PrivateError> {
    let body = message
        .body
        .map(|body| sealer.seal(MESSAGE_BODY_LABEL, &message.id, body.as_bytes()));
    Ok(sqlx::query(
        "INSERT INTO messages (id, peer, outgoing, kind, body, server_id, sent_at, state) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) ON CONFLICT (id) DO NOTHING",
    )
    .bind(message.id.as_slice())
    .bind(message.peer.as_slice())
    .bind(i64::from(message.outgoing))
    .bind(message.kind)
    .bind(body)
    .bind(message.server_id)
    .bind(message.sent_at)
    .bind(message.state)
    .execute(&mut *conn)
    .await?
    .rows_affected()
        == 1)
}

/// Edits a text by the same author: `(peer, outgoing)` says whose. Answers
/// its id when there was one.
async fn edit_own(
    conn: &mut SqliteConnection,
    sealer: &Sealer,
    (peer, outgoing): ([u8; KEY_LEN], bool),
    edit: &proto::PrivateEdit,
    edited_at: i64,
) -> Result<Option<[u8; MESSAGE_ID_LEN]>, PrivateError> {
    let id = message_id(&edit.message_id)?;
    let body = sealer.seal(MESSAGE_BODY_LABEL, &id, edit.content.as_bytes());
    let edited = sqlx::query(
        "UPDATE messages SET body = $1, edited_at = $2 \
         WHERE id = $3 AND peer = $4 AND outgoing = $5 AND kind = $6",
    )
    .bind(body)
    .bind(edited_at)
    .bind(id.as_slice())
    .bind(peer.as_slice())
    .bind(i64::from(outgoing))
    .bind(TEXT)
    .execute(&mut *conn)
    .await?
    .rows_affected()
        == 1;
    Ok(edited.then_some(id))
}

/// Deletes a text by the same author, as `edit_own`.
async fn delete_own(
    conn: &mut SqliteConnection,
    (peer, outgoing): ([u8; KEY_LEN], bool),
    delete: &proto::PrivateDelete,
) -> Result<Option<[u8; MESSAGE_ID_LEN]>, PrivateError> {
    let id = message_id(&delete.message_id)?;
    let deleted = sqlx::query(
        "DELETE FROM messages WHERE id = $1 AND peer = $2 AND outgoing = $3 AND kind = $4",
    )
    .bind(id.as_slice())
    .bind(peer.as_slice())
    .bind(i64::from(outgoing))
    .bind(TEXT)
    .execute(&mut *conn)
    .await?
    .rows_affected()
        == 1;
    Ok(deleted.then_some(id))
}

async fn load_or_new(
    conn: &mut SqliteConnection,
    sealer: &Sealer,
    peer: [u8; KEY_LEN],
) -> Result<Contact, PrivateError> {
    Ok(contacts::load(conn, sealer, peer)
        .await?
        .unwrap_or_else(|| Contact::new(peer)))
}

/// Something new arrived from them: the conversation shows, as a request
/// if it wasn't one yet.
fn arrived(contact: &mut Contact, sent_at: i64) {
    if contact.conversation == Conversation::None {
        contact.conversation = Conversation::Request;
    }
    contact.last_activity_at = Some(latest(contact.last_activity_at, sent_at));
    contact.hidden = false;
}

fn latest(last: Option<i64>, at: i64) -> i64 {
    last.map_or(at, |last| last.max(at))
}

/// `payload::parse` checked the length already.
fn message_id(bytes: &[u8]) -> Result<[u8; MESSAGE_ID_LEN], PrivateError> {
    <[u8; MESSAGE_ID_LEN]>::try_from(bytes)
        .map_err(|_| PrivateError::Invalid("a message id isn't 16 bytes".to_owned()))
}

fn random_id() -> Result<[u8; MESSAGE_ID_LEN], PrivateError> {
    let mut id = [0; MESSAGE_ID_LEN];
    getrandom::fill(&mut id)
        .map_err(|_| PrivateError::Crypto("the random source failed".to_owned()))?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_resets_wait_an_hour_per_device_and_old_ones_leave() {
        let limiter = ResetLimiter::default();
        let start = Instant::now();
        assert!(limiter.take([1; 32], start));
        assert!(!limiter.take([1; 32], start + RESET_EVERY / 2));
        assert!(limiter.take([2; 32], start + RESET_EVERY / 2));
        assert_eq!(limiter.lock().len(), 2);

        // An hour on, the first device may be reset again, and the second's
        // entry stays until its own hour is up.
        assert!(limiter.take([1; 32], start + RESET_EVERY));
        assert_eq!(limiter.lock().len(), 2);
        assert!(limiter.take([3; 32], start + RESET_EVERY * 2));
        assert_eq!(limiter.lock().len(), 1);
    }
}
