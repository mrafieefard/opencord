//! Sending (design 2026-10-22 §10.4). Every payload goes out the same way:
//! encrypted once per device of both people, with the advanced sessions
//! (and, for a text, the encrypted request) saved before the request leaves.
//! A message key is never used twice, and a request whose answer was lost
//! goes out again exactly as it was.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use opencord_common::private::{
    DEVICE_ID_LEN, DEVICE_LIST_TTL_MS, KEY_LEN, MESSAGE_ID_LEN, SIGNATURE_LEN, UNSENT_RESEND_MS,
    verify_device, verify_prekey,
};
use opencord_common::validation;
use opencord_proto::v1 as proto;
use prost::Message;
use proto::private_payload::Body;
use proto::request::Kind as Request;
use proto::response::Result as Response;

use super::PrivateError;
use super::contacts::{self, Contact, Conversation};
use super::payload;
use super::service::{
    MESSAGE_BODY_LABEL, MessageRow, Open, PrivateService, now_ms, open_text, warn_core,
};
use super::sessions::PeerSessions;
use crate::api::types::{CoreError, ErrorCode, PrivateEvent, PrivateMessage};

/// The label a message's saved request is sealed with (its id is the row).
pub const MESSAGE_OUTBOX_LABEL: &str = "messages.outbox";
/// How long a fetched device list is used.
const DEVICE_LIST_TTL: Duration = Duration::from_millis(DEVICE_LIST_TTL_MS.unsigned_abs());
const NO_RELAY: &str = "You don't share a connected server with them right now.";

// `messages.state`.
pub(super) const RECEIVED: i64 = 0;
const SENDING: i64 = 1;
pub(super) const SENT: i64 = 2;
const FAILED: i64 = 3;
// `messages.kind`.
pub(super) const TEXT: i64 = 0;
pub(super) const DIVIDER: i64 = 1;
pub(super) const MOVED: i64 = 2;
pub(super) const UNDECRYPTABLE: i64 = 3;

#[derive(Debug, Clone, Default)]
pub struct SendOptions {
    /// Send through this server key instead of the conversation's relay.
    pub relay: Option<String>,
}

/// What a send left behind.
#[derive(Debug, Clone)]
pub struct Sent {
    /// The stored message, for a text.
    pub message: Option<PrivateMessage>,
    /// The server's time, once it accepted the payload.
    pub sent_at_ms: Option<i64>,
}

/// One device whose keys its owner's identity key signed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedDevice {
    pub device_id: [u8; DEVICE_ID_LEN],
    pub curve25519: [u8; KEY_LEN],
    pub ed25519: [u8; KEY_LEN],
}

/// Verified device lists by (server key, user id).
///
/// An entry is replaced once it's 10 minutes old, removed when a send
/// through its server is refused with `DEVICE_LIST_CHANGED`, and removed
/// with its server. Entries past their 10 minutes are pruned on each insert,
/// so the map never outgrows the people written to in the last 10 minutes.
#[derive(Debug, Default)]
pub struct DeviceCache {
    lists: Mutex<DeviceLists>,
}

/// (server key, user id) → (devices, fetched at).
type DeviceLists = HashMap<(String, i64), (Vec<VerifiedDevice>, Instant)>;

impl DeviceCache {
    fn get(&self, key: &str, user_id: i64, now: Instant) -> Option<Vec<VerifiedDevice>> {
        self.lock()
            .get(&(key.to_owned(), user_id))
            .filter(|(_, fetched_at)| fresh(*fetched_at, now))
            .map(|(devices, _)| devices.clone())
    }

    fn put(&self, key: &str, user_id: i64, devices: Vec<VerifiedDevice>, now: Instant) {
        let mut lists = self.lock();
        lists.retain(|_, (_, fetched_at)| fresh(*fetched_at, now));
        lists.insert((key.to_owned(), user_id), (devices, now));
    }

    fn forget(&self, key: &str, user_id: i64) {
        self.lock().remove(&(key.to_owned(), user_id));
    }

    /// The server was removed.
    pub fn forget_server(&self, key: &str) {
        self.lock().retain(|(server, _), _| server != key);
    }

    fn lock(&self) -> MutexGuard<'_, DeviceLists> {
        self.lists.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn fresh(fetched_at: Instant, now: Instant) -> bool {
    now.saturating_duration_since(fetched_at) < DEVICE_LIST_TTL
}

/// A server that can carry the conversation now.
struct Relay {
    key: String,
    /// The server's id, hex: where a saved request belongs.
    server_id: String,
    recipient_id: i64,
    self_id: i64,
}

struct Target {
    user_id: i64,
    device: VerifiedDevice,
}

/// How one send ended.
enum Outcome {
    /// The server stored it, at this time.
    Sent(i64),
    /// The connection ended or timed out after the request was saved: it
    /// stays sending, and goes out again as it is.
    Unsent(CoreError),
    /// Refused, or it couldn't be encrypted and saved.
    Failed(CoreError),
}

/// A stored message, with its saved request.
struct Stored {
    row: MessageRow,
    outbox: Option<Vec<u8>>,
    outbox_server_id: Option<String>,
}

/// The one path for every payload: the stored message for a text, `None`
/// otherwise.
pub async fn send(
    service: &PrivateService,
    peer: [u8; KEY_LEN],
    body: Body,
    options: SendOptions,
) -> Result<Option<PrivateMessage>, CoreError> {
    Ok(send_timed(service, peer, body, options).await?.message)
}

/// As `send`, with the server's time. A text that couldn't go out for the
/// connection stays sending and comes back as such; any other failure of a
/// text marks it failed and is returned. Every failure of another payload
/// is returned.
pub async fn send_timed(
    service: &PrivateService,
    peer: [u8; KEY_LEN],
    body: Body,
    options: SendOptions,
) -> Result<Sent, CoreError> {
    let open = service.opened()?;
    let identity = service.identity_for(&open)?;
    // 1.
    let contact = load_contact(&open, peer).await?;
    let relay = current_relay(service, contact.as_ref(), &peer, options.relay.as_deref())?;
    let their_slip = contact.and_then(|contact| contact.their_slip);
    let message_id = new_message_id()?;
    let now = now_ms();
    let slip = Some(identity.slip_for(&peer));
    // 2.
    let Body::Text(text) = body else {
        let payload = payload::build(message_id, &peer, slip, body, now);
        let outcome = deliver(
            service,
            &open,
            &relay,
            &Encrypting {
                message_id,
                peer,
                payload: &payload,
                their_slip,
                text: false,
                fresh: None,
            },
            None,
        )
        .await;
        return match outcome {
            Outcome::Sent(sent_at_ms) => Ok(Sent {
                message: None,
                sent_at_ms: Some(sent_at_ms),
            }),
            Outcome::Unsent(error) | Outcome::Failed(error) => Err(error),
        };
    };
    let content = checked_content(&text.content)?;
    insert_sending(&open, message_id, peer, &content, now).await?;
    let added = view(service, &open, message_id).await?;
    service.emit(PrivateEvent::MessageAdded(added));
    let payload = payload::build(
        message_id,
        &peer,
        slip,
        Body::Text(proto::PrivateText { content }),
        now,
    );
    let outcome = deliver(
        service,
        &open,
        &relay,
        &Encrypting {
            message_id,
            peer,
            payload: &payload,
            their_slip,
            text: true,
            fresh: None,
        },
        None,
    )
    .await;
    let (message, sent_at_ms) = finish_text(service, &open, message_id, outcome).await?;
    Ok(Sent {
        message: Some(message),
        sent_at_ms,
    })
}

/// Sends a failed or unsent text again: its saved request when the server
/// it was made for can carry it now, else encrypted again for the current
/// relay. A text already sent is left alone.
pub async fn retry(
    service: &PrivateService,
    message_id: [u8; MESSAGE_ID_LEN],
) -> Result<(), CoreError> {
    let open = service.opened()?;
    let identity = service.identity_for(&open)?;
    let stored = stored(&open, message_id)
        .await?
        .ok_or_else(|| invalid_input("That message isn't there."))?;
    if !stored.row.outgoing || stored.row.kind != TEXT {
        return Err(invalid_input("Only your own messages can be sent again."));
    }
    if stored.row.state == SENT {
        return Ok(());
    }
    let peer = peer_of(&stored.row)?;
    let contact = load_contact(&open, peer).await?;
    let saved = match (&stored.outbox, &stored.outbox_server_id) {
        (Some(outbox), Some(server_id)) => service
            .transport()
            .server_key_for(server_id)
            .and_then(|key| relay_on(service, &key, &peer))
            .map(|relay| (relay, outbox)),
        _ => None,
    };
    let (relay, first) = match saved {
        Some((relay, outbox)) => (relay, Some(open_request(&open, message_id, outbox)?)),
        None => (current_relay(service, contact.as_ref(), &peer, None)?, None),
    };
    let restarted =
        sqlx::query("UPDATE messages SET state = $1 WHERE id = $2 AND state IN ($1, $3)")
            .bind(SENDING)
            .bind(message_id.as_slice())
            .bind(FAILED)
            .execute(open.store().pool())
            .await
            .map_err(PrivateError::from)?
            .rows_affected()
            == 1;
    if !restarted {
        // Sent or deleted meanwhile.
        return Ok(());
    }
    service.emit(PrivateEvent::MessageChanged(
        view(service, &open, message_id).await?,
    ));
    let payload = text_payload(&open, &identity, &stored.row, message_id, peer)?;
    let outcome = deliver(
        service,
        &open,
        &relay,
        &Encrypting {
            message_id,
            peer,
            payload: &payload,
            their_slip: contact.and_then(|contact| contact.their_slip),
            text: true,
            fresh: None,
        },
        first,
    )
    .await;
    finish_text(service, &open, message_id, outcome).await?;
    Ok(())
}

/// After a catch-up on `server_key`: every text still sending whose saved
/// request belongs to that server goes out again as it is, unless it's
/// over 24 hours old, which fails it.
pub async fn resend_unsent(service: &PrivateService, server_key: &str) -> Result<(), CoreError> {
    let open = service.opened()?;
    let Some(server_id) = service
        .members()
        .server_id(server_key)
        .or_else(|| service.transport().server_id_of(server_key))
    else {
        return Ok(());
    };
    let rows: Vec<(Vec<u8>, i64)> = sqlx::query_as(
        "SELECT id, sent_at FROM messages WHERE state = $1 AND outbox_server_id = $2 \
         ORDER BY sent_at, id",
    )
    .bind(SENDING)
    .bind(&server_id)
    .fetch_all(open.store().pool())
    .await
    .map_err(PrivateError::from)?;
    let now = now_ms();
    for (id, sent_at) in rows {
        let Ok(id) = <[u8; MESSAGE_ID_LEN]>::try_from(id.as_slice()) else {
            tracing::warn!("an unsent message has an id that isn't 16 bytes");
            continue;
        };
        let result = if now.saturating_sub(sent_at) > UNSENT_RESEND_MS {
            fail(service, &open, id).await
        } else {
            resend(service, &open, server_key, id).await
        };
        if let Err(error) = result {
            warn_core("resending a private message failed", server_key, &error);
        }
    }
    Ok(())
}

/// Replaces the text of one's own sent message, once the server accepted
/// the edit.
pub async fn edit(
    service: &PrivateService,
    peer: [u8; KEY_LEN],
    message_id: [u8; MESSAGE_ID_LEN],
    content: String,
) -> Result<(), CoreError> {
    let content = checked_content(&content)?;
    let open = service.opened()?;
    let stored = own_message(&open, peer, message_id).await?;
    if stored.row.state != SENT {
        return Err(invalid_input("Only sent messages can be edited."));
    }
    let sent = send_timed(
        service,
        peer,
        Body::Edit(proto::PrivateEdit {
            message_id: message_id.to_vec(),
            content: content.clone(),
        }),
        SendOptions::default(),
    )
    .await?;
    let sealed = open
        .store()
        .sealer()
        .seal(MESSAGE_BODY_LABEL, &message_id, content.as_bytes());
    let edited =
        sqlx::query("UPDATE messages SET body = $1, edited_at = $2 WHERE id = $3 AND peer = $4")
            .bind(sealed)
            .bind(sent.sent_at_ms.unwrap_or_else(now_ms))
            .bind(message_id.as_slice())
            .bind(peer.as_slice())
            .execute(open.store().pool())
            .await
            .map_err(PrivateError::from)?
            .rows_affected()
            == 1;
    if edited {
        service.emit(PrivateEvent::MessageChanged(
            view(service, &open, message_id).await?,
        ));
    }
    Ok(())
}

/// Deletes one's own message for everyone, once the server accepted the
/// delete.
pub async fn delete(
    service: &PrivateService,
    peer: [u8; KEY_LEN],
    message_id: [u8; MESSAGE_ID_LEN],
) -> Result<(), CoreError> {
    let open = service.opened()?;
    own_message(&open, peer, message_id).await?;
    send_timed(
        service,
        peer,
        Body::Delete(proto::PrivateDelete {
            message_id: message_id.to_vec(),
        }),
        SendOptions::default(),
    )
    .await?;
    let deleted = sqlx::query("DELETE FROM messages WHERE id = $1 AND peer = $2")
        .bind(message_id.as_slice())
        .bind(peer.as_slice())
        .execute(open.store().pool())
        .await
        .map_err(PrivateError::from)?
        .rows_affected()
        == 1;
    if deleted {
        service.emit(PrivateEvent::MessageDeleted {
            peer: hex::encode(peer),
            message_id: hex::encode(message_id),
        });
    }
    Ok(())
}

/// What one payload is encrypted as.
struct Encrypting<'a> {
    peer: [u8; KEY_LEN],
    message_id: [u8; MESSAGE_ID_LEN],
    payload: &'a [u8],
    /// The peer's permission slip for this person, for the request.
    their_slip: Option<[u8; SIGNATURE_LEN]>,
    /// A text keeps its request until the server accepts it.
    text: bool,
    /// A session reset: the device with this Curve25519 key gets it on a
    /// new session.
    fresh: Option<[u8; KEY_LEN]>,
}

/// A session reset (design 2026-10-22 §10.8), after a message from the
/// peer's device with the Curve25519 key `device` couldn't be decrypted: a
/// `SessionReset` through `server_key`, on a new session for that device.
/// The server takes a copy for every device of both people only, so the
/// others get one too, on their usual sessions; it shows nothing.
pub async fn reset_session(
    service: &PrivateService,
    peer: [u8; KEY_LEN],
    server_key: &str,
    device: [u8; KEY_LEN],
) -> Result<(), CoreError> {
    let open = service.opened()?;
    let contact = load_contact(&open, peer).await?;
    let relay = current_relay(service, contact.as_ref(), &peer, Some(server_key))?;
    let message_id = new_message_id()?;
    // A reset grants nothing, so it carries no slip.
    let payload = payload::build(
        message_id,
        &peer,
        None,
        Body::SessionReset(proto::SessionReset {}),
        now_ms(),
    );
    let outcome = deliver(
        service,
        &open,
        &relay,
        &Encrypting {
            message_id,
            peer,
            payload: &payload,
            their_slip: contact.and_then(|contact| contact.their_slip),
            text: false,
            fresh: Some(device),
        },
        None,
    )
    .await;
    match outcome {
        Outcome::Sent(_) => Ok(()),
        Outcome::Unsent(error) | Outcome::Failed(error) => Err(error),
    }
}

/// Steps 3–6: `first` (a saved request) or a fresh encryption goes out. A
/// `DEVICE_LIST_CHANGED` refusal encrypts again for the devices as they
/// are now, once.
async fn deliver(
    service: &PrivateService,
    open: &Open,
    relay: &Relay,
    encrypting: &Encrypting<'_>,
    first: Option<proto::SendPrivateMessage>,
) -> Outcome {
    let mut first = first;
    let mut refused = false;
    loop {
        let request = match first.take() {
            Some(request) => request,
            None => match encrypt(service, open, relay, encrypting).await {
                Ok(request) => request,
                Err(error) => return Outcome::Failed(error),
            },
        };
        match service
            .request(&relay.key, Request::SendPrivateMessage(request))
            .await
        {
            Ok(Response::PrivateMessageSent(sent)) => return Outcome::Sent(sent.sent_at_ms),
            Ok(other) => return Outcome::Failed(crate::client::unexpected(&other)),
            Err(
                error @ CoreError::Server {
                    code: ErrorCode::DeviceListChanged,
                    ..
                },
            ) => {
                let cache = service.device_cache();
                cache.forget(&relay.key, relay.recipient_id);
                cache.forget(&relay.key, relay.self_id);
                if refused {
                    return Outcome::Failed(error);
                }
                refused = true;
            }
            Err(error @ (CoreError::NotConnected | CoreError::Timeout)) => {
                return Outcome::Unsent(error);
            }
            Err(error) => return Outcome::Failed(error),
        }
    }
}

/// Steps 3–5: the devices, their keys, then under the device lock the
/// encryption, saved with the sessions (and a text's request) in one
/// transaction before anything is sent.
async fn encrypt(
    service: &PrivateService,
    open: &Open,
    relay: &Relay,
    encrypting: &Encrypting<'_>,
) -> Result<proto::SendPrivateMessage, CoreError> {
    let this_device = open.device().lock().await.id();
    // 3.
    let targets = devices(service, open, relay, &encrypting.peer, this_device).await?;
    // 4.
    let prekeys = claim(service, open, relay, &targets, encrypting.fresh).await?;
    // 5.
    let pickle_key = open.store().pickle_key();
    let device = open.device().lock().await;
    let mut conn = open
        .store()
        .pool()
        .acquire()
        .await
        .map_err(PrivateError::from)?;
    let now = now_ms();
    let mut copies = Vec::with_capacity(targets.len());
    let mut changed = Vec::with_capacity(targets.len());
    for target in &targets {
        let mut sessions =
            PeerSessions::load(&mut conn, pickle_key, target.device.curve25519).await?;
        let prekey = prekeys.get(&(target.user_id, target.device.device_id));
        let (olm_type, body) = if encrypting.fresh == Some(target.device.curve25519) {
            let prekey = prekey.ok_or_else(|| {
                PrivateError::Crypto("no key to reset the session with".to_owned())
            })?;
            sessions.encrypt_on_new(&device, *prekey, encrypting.payload, now)?
        } else {
            if !sessions.has_session() {
                let Some(prekey) = prekey else {
                    // No usable key for it: it's skipped for this send.
                    continue;
                };
                sessions.add_outbound(&device, *prekey, now)?;
            }
            sessions.encrypt(encrypting.payload)?
        };
        copies.push(proto::DeviceCiphertext {
            user_id: target.user_id,
            device_id: target.device.device_id.to_vec(),
            olm_type,
            body,
        });
        changed.push(sessions);
    }
    drop(conn);
    if copies.is_empty() {
        return Err(CoreError::Server {
            code: ErrorCode::PrivateMessagesUnavailable,
            message: "they can't get private messages on this server".to_owned(),
            retry_after_ms: None,
        });
    }
    let request = proto::SendPrivateMessage {
        recipient_id: relay.recipient_id,
        send_id: encrypting.message_id.to_vec(),
        ciphertexts: copies,
        slip: encrypting
            .their_slip
            .map(|slip| slip.to_vec())
            .unwrap_or_default(),
    };
    let mut tx = open
        .store()
        .pool()
        .begin()
        .await
        .map_err(PrivateError::from)?;
    for sessions in &mut changed {
        sessions.save(&mut tx, pickle_key).await?;
    }
    if encrypting.text {
        let sealed = open.store().sealer().seal(
            MESSAGE_OUTBOX_LABEL,
            &encrypting.message_id,
            &request.encode_to_vec(),
        );
        let saved = sqlx::query(
            "UPDATE messages SET outbox = $1, outbox_server_id = $2 WHERE id = $3 AND state = $4",
        )
        .bind(sealed)
        .bind(&relay.server_id)
        .bind(encrypting.message_id.as_slice())
        .bind(SENDING)
        .execute(&mut *tx)
        .await
        .map_err(PrivateError::from)?
        .rows_affected()
            == 1;
        if !saved {
            // Deleted meanwhile: nothing is sent, and the sessions stay as
            // they were.
            return Err(invalid_input("That message is no longer being sent."));
        }
    }
    tx.commit().await.map_err(PrivateError::from)?;
    drop(device);
    Ok(request)
}

/// The peer's devices and this person's other ones on the relay, from the
/// cache or fetched. Only devices whose signature verifies against their
/// owner's key are kept, and stored as known.
async fn devices(
    service: &PrivateService,
    open: &Open,
    relay: &Relay,
    peer: &[u8; KEY_LEN],
    this_device: [u8; DEVICE_ID_LEN],
) -> Result<Vec<Target>, CoreError> {
    let cache = service.device_cache();
    let now = Instant::now();
    let me = open.identity();
    let cached = cache
        .get(&relay.key, relay.recipient_id, now)
        .zip(cache.get(&relay.key, relay.self_id, now));
    let (theirs, mine) = match cached {
        Some(lists) => lists,
        None => {
            let response = service
                .request(
                    &relay.key,
                    Request::FetchDevices(proto::FetchDevices {
                        user_ids: vec![relay.recipient_id, relay.self_id],
                    }),
                )
                .await?;
            let Response::DeviceLists(lists) = response else {
                return Err(crate::client::unexpected(&response));
            };
            let theirs = verified(&relay.key, listed(&lists, relay.recipient_id), peer, None);
            let mine = verified(
                &relay.key,
                listed(&lists, relay.self_id),
                &me,
                Some(this_device),
            );
            remember(open, peer, &theirs).await?;
            remember(open, &me, &mine).await?;
            cache.put(&relay.key, relay.recipient_id, theirs.clone(), now);
            cache.put(&relay.key, relay.self_id, mine.clone(), now);
            (theirs, mine)
        }
    };
    let theirs = theirs.into_iter().map(|device| Target {
        user_id: relay.recipient_id,
        device,
    });
    let mine = mine.into_iter().map(|device| Target {
        user_id: relay.self_id,
        device,
    });
    Ok(theirs.chain(mine).collect())
}

pub(super) fn listed(
    lists: &proto::DeviceLists,
    user_id: i64,
) -> impl Iterator<Item = &proto::DeviceKeys> {
    lists
        .users
        .iter()
        .filter(move |user| user.user_id == user_id)
        .flat_map(|user| &user.devices)
}

/// The devices `owner`'s identity key signed, but not `skip`.
pub(super) fn verified<'a>(
    key: &str,
    listed: impl Iterator<Item = &'a proto::DeviceKeys>,
    owner: &[u8; KEY_LEN],
    skip: Option<[u8; DEVICE_ID_LEN]>,
) -> Vec<VerifiedDevice> {
    let mut unverified = 0;
    let mut devices = Vec::new();
    for keys in listed {
        let device = verify_device(
            owner,
            &keys.device_id,
            &keys.curve25519,
            &keys.ed25519,
            &keys.signature,
        )
        .then(|| {
            Some(VerifiedDevice {
                device_id: keys.device_id.as_slice().try_into().ok()?,
                curve25519: keys.curve25519.as_slice().try_into().ok()?,
                ed25519: keys.ed25519.as_slice().try_into().ok()?,
            })
        })
        .flatten();
        match device {
            Some(device) if Some(device.device_id) == skip => {}
            Some(device) => devices.push(device),
            None => unverified += 1,
        }
    }
    if unverified > 0 {
        tracing::warn!(
            server = %key,
            count = unverified,
            "devices whose signature doesn't verify were left out"
        );
    }
    devices
}

/// Verified devices, kept for knowing who sent what.
pub(super) async fn remember(
    open: &Open,
    owner: &[u8; KEY_LEN],
    devices: &[VerifiedDevice],
) -> Result<(), PrivateError> {
    if devices.is_empty() {
        return Ok(());
    }
    let now = now_ms();
    let mut tx = open.store().pool().begin().await?;
    for device in devices {
        sqlx::query(
            "INSERT INTO peer_devices (public_key, device_id, curve25519, ed25519, first_seen_at) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (public_key, device_id) DO UPDATE SET \
             curve25519 = excluded.curve25519, ed25519 = excluded.ed25519",
        )
        .bind(owner.as_slice())
        .bind(device.device_id.as_slice())
        .bind(device.curve25519.as_slice())
        .bind(device.ed25519.as_slice())
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// A prekey for each target without a session, and for the one getting a
/// `fresh` session, by (user id, device id): only those signed by that
/// device, as one-time or fallback key as the server says.
async fn claim(
    service: &PrivateService,
    open: &Open,
    relay: &Relay,
    targets: &[Target],
    fresh: Option<[u8; KEY_LEN]>,
) -> Result<HashMap<(i64, [u8; DEVICE_ID_LEN]), [u8; KEY_LEN]>, CoreError> {
    let mut wanted = Vec::new();
    {
        let mut conn = open
            .store()
            .pool()
            .acquire()
            .await
            .map_err(PrivateError::from)?;
        for target in targets {
            let session: Option<i64> =
                sqlx::query_scalar("SELECT 1 FROM sessions WHERE peer_curve25519 = $1 LIMIT 1")
                    .bind(target.device.curve25519.as_slice())
                    .fetch_optional(&mut *conn)
                    .await
                    .map_err(PrivateError::from)?;
            if session.is_none() || fresh == Some(target.device.curve25519) {
                wanted.push(proto::DeviceRef {
                    user_id: target.user_id,
                    device_id: target.device.device_id.to_vec(),
                });
            }
        }
    }
    if wanted.is_empty() {
        return Ok(HashMap::new());
    }
    let response = service
        .request(
            &relay.key,
            Request::ClaimKeys(proto::ClaimKeys { devices: wanted }),
        )
        .await?;
    let Response::ClaimedKeys(claimed) = response else {
        return Err(crate::client::unexpected(&response));
    };
    let mut prekeys = HashMap::new();
    let mut unverified = 0;
    for bundle in claimed.bundles {
        let (Some(keys), Some(prekey)) = (bundle.device, bundle.prekey) else {
            unverified += 1;
            continue;
        };
        let target = targets.iter().find(|target| {
            target.user_id == bundle.user_id && target.device.device_id.as_slice() == keys.device_id
        });
        let usable = target.filter(|target| {
            verify_prekey(
                &target.device.ed25519,
                &target.device.device_id,
                &prekey.key,
                bundle.fallback,
                &prekey.signature,
            )
        });
        match (usable, <[u8; KEY_LEN]>::try_from(prekey.key.as_slice())) {
            (Some(target), Ok(key)) => {
                prekeys.insert((target.user_id, target.device.device_id), key);
            }
            _ => unverified += 1,
        }
    }
    if unverified > 0 {
        tracing::warn!(
            server = %relay.key,
            count = unverified,
            "claimed keys that don't verify were skipped"
        );
    }
    Ok(prekeys)
}

/// After a text's send: sent with the server's time, left sending, or
/// failed with the error returned.
async fn finish_text(
    service: &PrivateService,
    open: &Open,
    message_id: [u8; MESSAGE_ID_LEN],
    outcome: Outcome,
) -> Result<(PrivateMessage, Option<i64>), CoreError> {
    match outcome {
        Outcome::Sent(sent_at_ms) => {
            if let Some(contact) = mark_sent(open, message_id, sent_at_ms).await? {
                service.emit(PrivateEvent::MessageChanged(
                    view(service, open, message_id).await?,
                ));
                service.emit_contact(open, &contact).await?;
            }
            Ok((view(service, open, message_id).await?, Some(sent_at_ms)))
        }
        Outcome::Unsent(error) => {
            tracing::debug!(
                error = %error,
                "a private message waits for its server to answer"
            );
            Ok((view(service, open, message_id).await?, None))
        }
        Outcome::Failed(error) => {
            fail(service, open, message_id).await?;
            Err(error)
        }
    }
}

/// Sent at the server's time, its request dropped; the conversation is
/// accepted and shown. Answers the contact, or `None` when the message was
/// sent or deleted meanwhile.
async fn mark_sent(
    open: &Open,
    message_id: [u8; MESSAGE_ID_LEN],
    sent_at_ms: i64,
) -> Result<Option<Contact>, PrivateError> {
    let sealer = open.store().sealer();
    // It reads the contact, then writes: take the write lock up front.
    let mut tx = open.store().pool().begin_with("BEGIN IMMEDIATE").await?;
    let peer: Option<Vec<u8>> = sqlx::query_scalar(
        "UPDATE messages SET state = $1, sent_at = $2, outbox = NULL, outbox_server_id = NULL \
         WHERE id = $3 AND state IN ($4, $5) RETURNING peer",
    )
    .bind(SENT)
    .bind(sent_at_ms)
    .bind(message_id.as_slice())
    .bind(SENDING)
    .bind(FAILED)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(peer) = peer else {
        return Ok(None);
    };
    let peer = <[u8; KEY_LEN]>::try_from(peer.as_slice())
        .map_err(|_| PrivateError::Storage("a stored message's peer isn't 32 bytes".to_owned()))?;
    let mut contact = contacts::load(&mut tx, sealer, peer)
        .await?
        .unwrap_or_else(|| Contact::new(peer));
    contact.conversation = Conversation::Accepted;
    contact.hidden = false;
    contact.last_activity_at = Some(
        contact
            .last_activity_at
            .map_or(sent_at_ms, |at| at.max(sent_at_ms)),
    );
    contacts::save(&mut tx, sealer, &contact, now_ms()).await?;
    tx.commit().await?;
    Ok(Some(contact))
}

/// A text still sending becomes failed, and the app hears of it.
async fn fail(
    service: &PrivateService,
    open: &Open,
    message_id: [u8; MESSAGE_ID_LEN],
) -> Result<(), CoreError> {
    let failed = sqlx::query("UPDATE messages SET state = $1 WHERE id = $2 AND state = $3")
        .bind(FAILED)
        .bind(message_id.as_slice())
        .bind(SENDING)
        .execute(open.store().pool())
        .await
        .map_err(PrivateError::from)?
        .rows_affected()
        == 1;
    if failed {
        service.emit(PrivateEvent::MessageChanged(
            view(service, open, message_id).await?,
        ));
    }
    Ok(())
}

/// One unsent text's saved request goes out again through `server_key`.
async fn resend(
    service: &PrivateService,
    open: &Open,
    server_key: &str,
    message_id: [u8; MESSAGE_ID_LEN],
) -> Result<(), CoreError> {
    let identity = service.identity_for(open)?;
    let Some(stored) = stored(open, message_id).await? else {
        return Ok(());
    };
    let Some(outbox) = stored
        .outbox
        .as_ref()
        .filter(|_| stored.row.state == SENDING)
    else {
        return Ok(());
    };
    let peer = peer_of(&stored.row)?;
    let Some(relay) = relay_on(service, server_key, &peer) else {
        // They aren't reachable there any more.
        return fail(service, open, message_id).await;
    };
    let first = open_request(open, message_id, outbox)?;
    let their_slip = load_contact(open, peer)
        .await?
        .and_then(|contact| contact.their_slip);
    let payload = text_payload(open, &identity, &stored.row, message_id, peer)?;
    let outcome = deliver(
        service,
        open,
        &relay,
        &Encrypting {
            message_id,
            peer,
            payload: &payload,
            their_slip,
            text: true,
            fresh: None,
        },
        Some(first),
    )
    .await;
    finish_text(service, open, message_id, outcome).await?;
    Ok(())
}

/// The relay chosen, else the conversation's, when it can carry the
/// conversation now.
fn current_relay(
    service: &PrivateService,
    contact: Option<&Contact>,
    peer: &[u8; KEY_LEN],
    chosen: Option<&str>,
) -> Result<Relay, CoreError> {
    let key = match chosen {
        Some(key) => Some(key.to_owned()),
        None => contact
            .and_then(|contact| contact.relay_server_id.as_deref())
            .and_then(|server_id| service.transport().server_key_for(server_id)),
    };
    key.and_then(|key| relay_on(service, &key, peer))
        .ok_or_else(|| invalid_input(NO_RELAY))
}

/// `key`, when it's connected, relays private messages, and has the peer
/// as a member.
fn relay_on(service: &PrivateService, key: &str, peer: &[u8; KEY_LEN]) -> Option<Relay> {
    let members = service.members();
    let shared = members
        .shared_servers(peer)
        .into_iter()
        .find(|shared| shared.server_key == key)
        .filter(|shared| shared.connected && shared.private_messages)?;
    Some(Relay {
        key: key.to_owned(),
        server_id: members
            .server_id(key)
            .or_else(|| service.transport().server_id_of(key))?,
        recipient_id: shared.peer_user_id,
        self_id: members.self_id(key)?,
    })
}

async fn load_contact(open: &Open, peer: [u8; KEY_LEN]) -> Result<Option<Contact>, CoreError> {
    let mut conn = open
        .store()
        .pool()
        .acquire()
        .await
        .map_err(PrivateError::from)?;
    Ok(contacts::load(&mut conn, open.store().sealer(), peer).await?)
}

async fn insert_sending(
    open: &Open,
    message_id: [u8; MESSAGE_ID_LEN],
    peer: [u8; KEY_LEN],
    content: &str,
    now: i64,
) -> Result<(), PrivateError> {
    let body = open
        .store()
        .sealer()
        .seal(MESSAGE_BODY_LABEL, &message_id, content.as_bytes());
    sqlx::query(
        "INSERT INTO messages (id, peer, outgoing, kind, body, sent_at, state) \
         VALUES ($1, $2, 1, $3, $4, $5, $6)",
    )
    .bind(message_id.as_slice())
    .bind(peer.as_slice())
    .bind(TEXT)
    .bind(body)
    .bind(now)
    .bind(SENDING)
    .execute(open.store().pool())
    .await?;
    Ok(())
}

async fn stored(
    open: &Open,
    message_id: [u8; MESSAGE_ID_LEN],
) -> Result<Option<Stored>, CoreError> {
    let row = sqlx::query(
        "SELECT id, peer, outgoing, kind, body, server_id, sent_at, edited_at, state, \
         outbox, outbox_server_id FROM messages WHERE id = $1",
    )
    .bind(message_id.as_slice())
    .fetch_optional(open.store().pool())
    .await
    .map_err(PrivateError::from)?;
    let Some(row) = row else {
        return Ok(None);
    };
    use sqlx::Row;
    Ok(Some(Stored {
        outbox: row.try_get("outbox").map_err(PrivateError::from)?,
        outbox_server_id: row
            .try_get("outbox_server_id")
            .map_err(PrivateError::from)?,
        row: MessageRow::from_row(&row)?,
    }))
}

/// One's own text in the conversation with `peer`.
async fn own_message(
    open: &Open,
    peer: [u8; KEY_LEN],
    message_id: [u8; MESSAGE_ID_LEN],
) -> Result<Stored, CoreError> {
    stored(open, message_id)
        .await?
        .filter(|stored| {
            stored.row.outgoing && stored.row.kind == TEXT && stored.row.peer == peer.as_slice()
        })
        .ok_or_else(|| invalid_input("You can only change your own messages."))
}

pub(super) async fn view(
    service: &PrivateService,
    open: &Open,
    message_id: [u8; MESSAGE_ID_LEN],
) -> Result<PrivateMessage, CoreError> {
    let stored = stored(open, message_id)
        .await?
        .ok_or_else(|| invalid_input("That message was deleted."))?;
    Ok(service.message_view(open, &stored.row)?)
}

/// A stored text's payload, with the same id, for encrypting it again.
fn text_payload(
    open: &Open,
    identity: &crate::identity::Identity,
    row: &MessageRow,
    message_id: [u8; MESSAGE_ID_LEN],
    peer: [u8; KEY_LEN],
) -> Result<Vec<u8>, CoreError> {
    let body = row
        .body
        .as_deref()
        .ok_or_else(|| PrivateError::Storage("a stored text has no body".to_owned()))?;
    let content = open_text(open.store(), &message_id, body)?;
    Ok(payload::build(
        message_id,
        &peer,
        Some(identity.slip_for(&peer)),
        Body::Text(proto::PrivateText { content }),
        now_ms(),
    ))
}

fn open_request(
    open: &Open,
    message_id: [u8; MESSAGE_ID_LEN],
    outbox: &[u8],
) -> Result<proto::SendPrivateMessage, CoreError> {
    let bytes = open
        .store()
        .sealer()
        .open(MESSAGE_OUTBOX_LABEL, &message_id, outbox)?;
    Ok(proto::SendPrivateMessage::decode(bytes.as_slice())
        .map_err(|_| PrivateError::Storage("a saved request doesn't decode".to_owned()))?)
}

fn peer_of(row: &MessageRow) -> Result<[u8; KEY_LEN], CoreError> {
    Ok(<[u8; KEY_LEN]>::try_from(row.peer.as_slice())
        .map_err(|_| PrivateError::Storage("a stored message's peer isn't 32 bytes".to_owned()))?)
}

fn checked_content(content: &str) -> Result<String, CoreError> {
    validation::message_content(content).map_err(|error| invalid_input(&error.to_string()))
}

fn new_message_id() -> Result<[u8; MESSAGE_ID_LEN], PrivateError> {
    let mut id = [0; MESSAGE_ID_LEN];
    getrandom::fill(&mut id)
        .map_err(|_| PrivateError::Crypto("the random source failed".to_owned()))?;
    Ok(id)
}

fn invalid_input(message: &str) -> CoreError {
    CoreError::InvalidInput {
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(byte: u8) -> VerifiedDevice {
        VerifiedDevice {
            device_id: [byte; 16],
            curve25519: [byte; 32],
            ed25519: [byte; 32],
        }
    }

    #[test]
    fn device_lists_leave_when_stale_refused_or_their_server_goes() {
        let cache = DeviceCache::default();
        let start = Instant::now();
        cache.put("a", 1, vec![device(1)], start);
        cache.put("a", 2, vec![device(2)], start);
        cache.put("b", 1, vec![device(3)], start);
        assert_eq!(cache.get("a", 1, start), Some(vec![device(1)]));

        // Refused: that list goes.
        cache.forget("a", 1);
        assert_eq!(cache.get("a", 1, start), None);
        assert_eq!(cache.get("a", 2, start), Some(vec![device(2)]));

        // Ten minutes on, the rest are stale, and the next insert drops them.
        let later = start + DEVICE_LIST_TTL;
        assert_eq!(cache.get("a", 2, later), None);
        cache.put("a", 3, vec![device(4)], later);
        assert_eq!(cache.lock().len(), 1);

        // Removing a server drops its lists only.
        cache.put("b", 1, vec![device(3)], later);
        cache.forget_server("a");
        assert_eq!(cache.get("a", 3, later), None);
        assert_eq!(cache.get("b", 1, later), Some(vec![device(3)]));
    }
}
