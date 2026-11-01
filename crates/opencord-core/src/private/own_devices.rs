//! This person's other devices with chat keys of their own, and giving
//! one of them this device's chat key after both showed the same words
//! (design 2026-11-01 §7).

use opencord_common::private::{DEVICE_ID_LEN, KEY_LEN};
use opencord_common::words;
use opencord_proto::v1 as proto;
use proto::request::Kind as Request;
use proto::response::Result as Response;
use sqlx::SqliteConnection;

use super::PrivateError;
use super::inbox::{self, Insert};
use super::outbox::{self, KEYS_CHANGED, SENT, TEXT, VerifiedDevice};
use super::seal::Sealer;
use super::service::{Open, PrivateService, now_ms, warn_core};
use crate::api::types::{CoreError, OwnDevice, OwnDeviceState, PrivateEvent};

/// The label a waiting share is sealed with; the row is the sender's id.
const SHARE_LABEL: &str = "own_devices.share";
// `own_devices.state`.
const OTHER: i64 = 0;
const NOT_MINE: i64 = 1;

/// Notes this person's devices a server listed: one with another chat key
/// is kept, one with this device's is forgotten (it was approved).
pub(super) async fn observe(
    service: &PrivateService,
    open: &Open,
    mine: &[VerifiedDevice],
) -> Result<(), PrivateError> {
    if mine.is_empty() {
        return Ok(());
    }
    let chat_key = open.chat_key().lock().await.public();
    let now = now_ms();
    let mut changed = false;
    let mut tx = open.store().pool().begin().await?;
    for device in mine {
        if device.chat_key == chat_key {
            changed |= sqlx::query("DELETE FROM own_devices WHERE device_id = $1")
                .bind(device.device_id.as_slice())
                .execute(&mut *tx)
                .await?
                .rows_affected()
                > 0;
            continue;
        }
        let known: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT chat_key FROM own_devices WHERE device_id = $1")
                .bind(device.device_id.as_slice())
                .fetch_optional(&mut *tx)
                .await?;
        changed |= known.as_deref() != Some(device.chat_key.as_slice());
        upsert(&mut tx, device, now).await?;
    }
    tx.commit().await?;
    if changed {
        emit(service, open).await?;
    }
    Ok(())
}

async fn upsert(
    conn: &mut SqliteConnection,
    device: &VerifiedDevice,
    now: i64,
) -> Result<(), PrivateError> {
    sqlx::query(
        "INSERT INTO own_devices (device_id, ed25519, chat_key, state, seen_at) \
         VALUES ($1, $2, $3, $4, $5) \
         ON CONFLICT (device_id) DO UPDATE SET ed25519 = excluded.ed25519, \
         chat_key = excluded.chat_key, seen_at = excluded.seen_at",
    )
    .bind(device.device_id.as_slice())
    .bind(device.ed25519.as_slice())
    .bind(device.chat_key.as_slice())
    .bind(OTHER)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The devices to show, and whether this device has chats.
pub(super) async fn list(open: &Open) -> Result<(Vec<OwnDevice>, bool), PrivateError> {
    let rows: Vec<(Vec<u8>, bool)> = sqlx::query_as(
        "SELECT device_id, share IS NOT NULL FROM own_devices WHERE state = $1 \
         ORDER BY seen_at, device_id",
    )
    .bind(OTHER)
    .fetch_all(open.store().pool())
    .await?;
    let devices = rows
        .into_iter()
        .map(|(device_id, offering)| OwnDevice {
            device_id: hex::encode(device_id),
            state: if offering {
                OwnDeviceState::Offering
            } else {
                OwnDeviceState::Other
            },
        })
        .collect();
    let has_chats = sqlx::query_scalar::<_, i64>("SELECT 1 FROM messages WHERE kind = $1 LIMIT 1")
        .bind(TEXT)
        .fetch_optional(open.store().pool())
        .await?
        .is_some();
    Ok((devices, has_chats))
}

pub(super) async fn emit(service: &PrivateService, open: &Open) -> Result<(), PrivateError> {
    let (devices, has_chats) = list(open).await?;
    service.emit(PrivateEvent::OwnDevicesChanged { devices, has_chats });
    Ok(())
}

/// Asks every connected server for this person's devices, then lists them.
pub async fn own_devices(service: &PrivateService) -> Result<(Vec<OwnDevice>, bool), CoreError> {
    let open = service.opened()?;
    for key in service.members().private_servers() {
        if let Err(error) = refresh_on(service, &open, &key).await {
            warn_core("listing this person's devices failed", &key, &error);
        }
    }
    Ok(list(&open).await?)
}

/// Asks `key` for this person's devices and notes them.
pub(super) async fn refresh_on(
    service: &PrivateService,
    open: &Open,
    key: &str,
) -> Result<(), CoreError> {
    let self_id = service
        .members()
        .self_id(key)
        .ok_or(CoreError::NotConnected)?;
    let response = service
        .request(
            key,
            Request::FetchDevices(proto::FetchDevices {
                user_ids: vec![self_id],
            }),
        )
        .await?;
    let Response::DeviceLists(lists) = response else {
        return Err(crate::client::unexpected(&response));
    };
    let this_device = open.device().lock().await.id();
    let mine = outbox::verified(key, outbox::listed(&lists, self_id), Some(this_device));
    outbox::remember(open, &open.identity(), &mine).await?;
    observe(service, open, &mine).await?;
    Ok(())
}

/// The 16 words this device and `device_id` both show (§9.2).
pub async fn pairing_words(
    service: &PrivateService,
    device_id: &str,
) -> Result<Vec<String>, CoreError> {
    let open = service.opened()?;
    let id = device_id_of(device_id)?;
    let theirs: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT ed25519 FROM own_devices WHERE device_id = $1")
            .bind(id.as_slice())
            .fetch_optional(open.store().pool())
            .await
            .map_err(PrivateError::from)?;
    let theirs = theirs
        .and_then(|key| <[u8; KEY_LEN]>::try_from(key.as_slice()).ok())
        .ok_or_else(unknown_device)?;
    let mine = open.device().lock().await.ed25519();
    Ok(words::pairing_words(&mine, &theirs)
        .iter()
        .map(|word| (*word).to_owned())
        .collect())
}

/// They match, on the device giving its chat key: it goes to `device_id`
/// through the first connected server that carries it.
pub async fn approve(service: &PrivateService, device_id: &str) -> Result<(), CoreError> {
    let open = service.opened()?;
    let id = device_id_of(device_id)?;
    let state: Option<i64> =
        sqlx::query_scalar("SELECT state FROM own_devices WHERE device_id = $1")
            .bind(id.as_slice())
            .fetch_optional(open.store().pool())
            .await
            .map_err(PrivateError::from)?;
    if state != Some(OTHER) {
        return Err(unknown_device());
    }
    let secret = open.chat_key().lock().await.secret();
    let mut failed = CoreError::NotConnected;
    for key in service.members().private_servers() {
        match outbox::share_chat_key(service, &open, &key, id, secret).await {
            Ok(()) => return Ok(()),
            Err(error) => {
                warn_core("giving a device this chat key failed", &key, &error);
                failed = error;
            }
        }
    }
    Err(failed)
}

/// A `ChatKeyShare` from `sender`, another of this person's devices, kept
/// sealed until They match. Inside the caller's transaction; `false` when
/// that device is "Not mine".
pub(super) async fn receive_share(
    conn: &mut SqliteConnection,
    sealer: &Sealer,
    sender: &VerifiedDevice,
    secret: &[u8],
) -> Result<bool, PrivateError> {
    let state: Option<i64> =
        sqlx::query_scalar("SELECT state FROM own_devices WHERE device_id = $1")
            .bind(sender.device_id.as_slice())
            .fetch_optional(&mut *conn)
            .await?;
    if state == Some(NOT_MINE) {
        return Ok(false);
    }
    upsert(conn, sender, now_ms()).await?;
    sqlx::query("UPDATE own_devices SET share = $1 WHERE device_id = $2")
        .bind(sealer.seal(SHARE_LABEL, &sender.device_id, secret))
        .bind(sender.device_id.as_slice())
        .execute(&mut *conn)
        .await?;
    Ok(true)
}

/// They match or They don't match, on the device a share came to. A match
/// takes the shared chat key: this device registers again with it, and
/// every chat says the keys changed.
pub async fn accept(
    service: &PrivateService,
    device_id: &str,
    matched: bool,
) -> Result<(), CoreError> {
    let open = service.opened()?;
    let id = device_id_of(device_id)?;
    let share: Option<Option<Vec<u8>>> =
        sqlx::query_scalar("SELECT share FROM own_devices WHERE device_id = $1 AND state = $2")
            .bind(id.as_slice())
            .bind(OTHER)
            .fetch_optional(open.store().pool())
            .await
            .map_err(PrivateError::from)?;
    let sealed = share.flatten().ok_or_else(unknown_device)?;
    if !matched {
        sqlx::query("UPDATE own_devices SET share = NULL WHERE device_id = $1")
            .bind(id.as_slice())
            .execute(open.store().pool())
            .await
            .map_err(PrivateError::from)?;
        emit(service, &open).await?;
        return Ok(());
    }
    let sealer = open.store().sealer();
    let secret = sealer.open(SHARE_LABEL, &id, &sealed)?;
    let secret = <[u8; KEY_LEN]>::try_from(secret.as_slice())
        .map_err(|_| PrivateError::Invalid("a shared chat key isn't 32 bytes".to_owned()))?;
    let added = {
        let mut chat_key = open.chat_key().lock().await;
        let mut tx = open
            .store()
            .pool()
            .begin()
            .await
            .map_err(PrivateError::from)?;
        chat_key.replace(&mut tx, sealer, secret).await?;
        sqlx::query("DELETE FROM own_devices WHERE device_id = $1")
            .bind(id.as_slice())
            .execute(&mut *tx)
            .await
            .map_err(PrivateError::from)?;
        let added = keys_changed_everywhere(&mut tx, sealer).await?;
        tx.commit().await.map_err(PrivateError::from)?;
        added
    };
    service.device_cache().clear();
    for key in open.registered_servers() {
        service.start_registration(&key);
    }
    for message_id in added {
        match outbox::view(service, &open, message_id).await {
            Ok(message) => service.emit(PrivateEvent::MessageAdded(message)),
            Err(error) => warn_core("showing a keys changed line failed", "", &error),
        }
    }
    emit(service, &open).await?;
    Ok(())
}

/// "Your keys changed." in every conversation with messages.
async fn keys_changed_everywhere(
    conn: &mut SqliteConnection,
    sealer: &Sealer,
) -> Result<Vec<[u8; 16]>, PrivateError> {
    let peers: Vec<Vec<u8>> = sqlx::query_scalar("SELECT DISTINCT peer FROM messages")
        .fetch_all(&mut *conn)
        .await?;
    let now = now_ms();
    let mut added = Vec::new();
    for peer in peers {
        let Ok(peer) = <[u8; KEY_LEN]>::try_from(peer.as_slice()) else {
            continue;
        };
        let id = inbox::random_id()?;
        let insert = Insert {
            id,
            peer,
            outgoing: true,
            kind: KEYS_CHANGED,
            body: None,
            server_id: None,
            sent_at: now,
            state: SENT,
        };
        if inbox::insert(conn, sealer, insert).await? {
            added.push(id);
        }
    }
    Ok(added)
}

/// "Not mine": the device is hidden here and never sent anything.
pub async fn reject(service: &PrivateService, device_id: &str) -> Result<(), CoreError> {
    let open = service.opened()?;
    let id = device_id_of(device_id)?;
    let updated =
        sqlx::query("UPDATE own_devices SET state = $1, share = NULL WHERE device_id = $2")
            .bind(NOT_MINE)
            .bind(id.as_slice())
            .execute(open.store().pool())
            .await
            .map_err(PrivateError::from)?
            .rows_affected();
    if updated == 0 {
        return Err(unknown_device());
    }
    emit(service, &open).await?;
    Ok(())
}

fn device_id_of(hex_id: &str) -> Result<[u8; DEVICE_ID_LEN], CoreError> {
    hex::decode(hex_id)
        .ok()
        .and_then(|bytes| <[u8; DEVICE_ID_LEN]>::try_from(bytes.as_slice()).ok())
        .ok_or_else(unknown_device)
}

fn unknown_device() -> CoreError {
    CoreError::InvalidInput {
        message: "That device isn't one of yours.".to_owned(),
    }
}
