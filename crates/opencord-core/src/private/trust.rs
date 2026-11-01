//! Whose chat key this device trusts for each person (design 2026-11-01
//! §8): one pinned key per contact. Their devices signed by it get
//! messages; the rest are held until compared or accepted.

use std::collections::BTreeMap;

use opencord_common::private::{DEVICE_ID_LEN, KEY_LEN};
use opencord_common::words;
use sqlx::SqliteConnection;

use super::PrivateError;
use super::inbox::{self, Insert};
use super::outbox::{self, KEYS_CHANGED, NEW_DEVICE, RECEIVED, VerifiedDevice};
use super::seal::Sealer;
use super::service::{Open, PrivateService, now_ms, parse_hex};
use crate::api::types::{CoreError, PrivateEvent, PrivateVerification};

/// What this device holds about one contact's chat key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactKeys {
    pub pinned: [u8; KEY_LEN],
    pub candidate: Option<[u8; KEY_LEN]>,
    /// The key pinned before: its devices are held, never offered.
    replaced: Option<[u8; KEY_LEN]>,
    /// (mine, theirs) when They match was tapped.
    verified: Option<([u8; KEY_LEN], [u8; KEY_LEN])>,
    mismatch: bool,
    pub reachable: bool,
}

impl ContactKeys {
    /// Verified only while both chat keys are the ones compared.
    pub fn verification(&self, mine: &[u8; KEY_LEN]) -> PrivateVerification {
        if self.mismatch {
            PrivateVerification::Mismatch
        } else if self.verified == Some((*mine, self.pinned)) {
            PrivateVerification::Verified
        } else {
            PrivateVerification::Unverified
        }
    }
}

type Row = (
    Vec<u8>,
    Option<Vec<u8>>,
    Option<Vec<u8>>,
    Option<Vec<u8>>,
    Option<Vec<u8>>,
    bool,
    bool,
);

pub async fn load(
    conn: &mut SqliteConnection,
    peer: &[u8; KEY_LEN],
) -> Result<Option<ContactKeys>, PrivateError> {
    let row: Option<Row> = sqlx::query_as(
        "SELECT pinned, candidate, replaced, verified_mine, verified_theirs, mismatch, reachable \
         FROM contact_keys WHERE public_key = $1",
    )
    .bind(peer.as_slice())
    .fetch_optional(&mut *conn)
    .await?;
    let Some((pinned, candidate, replaced, mine, theirs, mismatch, reachable)) = row else {
        return Ok(None);
    };
    let key = |bytes: Vec<u8>| {
        <[u8; KEY_LEN]>::try_from(bytes.as_slice())
            .map_err(|_| PrivateError::Storage("a stored chat key isn't 32 bytes".to_owned()))
    };
    Ok(Some(ContactKeys {
        pinned: key(pinned)?,
        candidate: candidate.map(key).transpose()?,
        replaced: replaced.map(key).transpose()?,
        verified: match (mine, theirs) {
            (Some(mine), Some(theirs)) => Some((key(mine)?, key(theirs)?)),
            _ => None,
        },
        mismatch,
        reachable,
    }))
}

async fn save(
    conn: &mut SqliteConnection,
    peer: &[u8; KEY_LEN],
    keys: &ContactKeys,
) -> Result<(), PrivateError> {
    let (mine, theirs) = keys.verified.unzip();
    sqlx::query(
        "INSERT INTO contact_keys \
         (public_key, pinned, candidate, replaced, verified_mine, verified_theirs, mismatch, \
         reachable) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         ON CONFLICT (public_key) DO UPDATE SET pinned = excluded.pinned, \
         candidate = excluded.candidate, replaced = excluded.replaced, verified_mine = excluded.verified_mine, \
         verified_theirs = excluded.verified_theirs, mismatch = excluded.mismatch, \
         reachable = excluded.reachable",
    )
    .bind(peer.as_slice())
    .bind(keys.pinned.as_slice())
    .bind(keys.candidate.as_ref().map(|key| key.to_vec()))
    .bind(keys.replaced.as_ref().map(|key| key.to_vec()))
    .bind(mine.map(|key| key.to_vec()))
    .bind(theirs.map(|key| key.to_vec()))
    .bind(keys.mismatch)
    .bind(keys.reachable)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// A server listed `peer`'s devices: pins on first contact, notes another
/// key and whether any trusted device is left, and a quiet divider for each
/// new device under the pinned key (`fresh`: ids this device didn't know).
/// Answers the sorted devices.
pub(super) async fn observe(
    service: &PrivateService,
    open: &Open,
    peer: &[u8; KEY_LEN],
    listed: &[VerifiedDevice],
    fresh: &[[u8; DEVICE_ID_LEN]],
) -> Result<Classified, PrivateError> {
    let sealer = open.store().sealer();
    let mut tx = open.store().pool().begin_with("BEGIN IMMEDIATE").await?;
    let before = load(&mut tx, peer).await?;
    let classified = classify(before.as_ref().map(|keys| keys.pinned), listed);
    let Some(pinned) = before.as_ref().map(|keys| keys.pinned).or(classified.pin) else {
        tx.commit().await?;
        return Ok(classified);
    };
    let replaced = before.as_ref().and_then(|keys| keys.replaced);
    let offered = most_used(
        classified
            .held
            .iter()
            .filter(|device| Some(device.chat_key) != replaced),
    );
    let after = ContactKeys {
        pinned,
        candidate: offered,
        reachable: !classified.trusted.is_empty(),
        ..before.clone().unwrap_or(ContactKeys {
            pinned,
            candidate: None,
            replaced: None,
            verified: None,
            mismatch: false,
            reachable: true,
        })
    };
    let mut added = Vec::new();
    if before.is_some() {
        let new_devices = classified
            .trusted
            .iter()
            .filter(|device| fresh.contains(&device.device_id))
            .count();
        for _ in 0..new_devices {
            if let Some(id) = divider(&mut tx, sealer, peer, NEW_DEVICE).await? {
                added.push(id);
            }
        }
    }
    let changed = before.as_ref() != Some(&after);
    if changed {
        save(&mut tx, peer, &after).await?;
    }
    tx.commit().await?;
    for id in added {
        show(service, open, id).await;
    }
    // What the contact shows: whether new keys wait, and whether anyone
    // can be reached.
    let shown = |keys: Option<&ContactKeys>| {
        keys.map_or((false, true), |keys| {
            (keys.candidate.is_some(), keys.reachable)
        })
    };
    if shown(before.as_ref()) != shown(Some(&after)) {
        emit_contact(service, open, peer).await;
    }
    Ok(classified)
}

/// Whether a message from `device` of `peer` may be read. First contact
/// pins its chat key; another key is noted as the candidate.
pub(super) async fn accepts(
    service: &PrivateService,
    open: &Open,
    peer: &[u8; KEY_LEN],
    device: &VerifiedDevice,
) -> Result<bool, PrivateError> {
    let mut tx = open.store().pool().begin_with("BEGIN IMMEDIATE").await?;
    let before = load(&mut tx, peer).await?;
    let (accepted, after) = match &before {
        None => (
            true,
            ContactKeys {
                pinned: device.chat_key,
                candidate: None,
                replaced: None,
                verified: None,
                mismatch: false,
                reachable: true,
            },
        ),
        Some(keys) if keys.pinned == device.chat_key => (true, keys.clone()),
        Some(keys) if keys.replaced == Some(device.chat_key) => (false, keys.clone()),
        Some(keys) => (
            false,
            ContactKeys {
                candidate: Some(device.chat_key),
                ..keys.clone()
            },
        ),
    };
    let changed = before.as_ref() != Some(&after);
    if changed {
        save(&mut tx, peer, &after).await?;
    }
    tx.commit().await?;
    if changed && before.is_some() {
        emit_contact(service, open, peer).await;
    }
    Ok(accepted)
}

/// The 16 words for the chat with `peer`: this person's chat key and theirs,
/// or with `new_keys`, the other key their devices offer (§9.1).
pub async fn chat_words(
    service: &PrivateService,
    peer: &str,
    new_keys: bool,
) -> Result<Vec<String>, CoreError> {
    let open = service.opened()?;
    let peer = parse_hex::<KEY_LEN>(peer, "That isn't someone's key.")?;
    let keys = stored(&open, &peer).await?;
    let theirs = if new_keys {
        keys.candidate.ok_or_else(no_new_keys)?
    } else {
        keys.pinned
    };
    let mine = open.chat_key().lock().await.public();
    Ok(words::chat_words(&mine, &theirs)
        .iter()
        .map(|word| (*word).to_owned())
        .collect())
}

/// They match or They don't match. A match on the new keys pins them.
pub async fn verify(
    service: &PrivateService,
    peer: &str,
    new_keys: bool,
    matched: bool,
) -> Result<(), CoreError> {
    let open = service.opened()?;
    let peer = parse_hex::<KEY_LEN>(peer, "That isn't someone's key.")?;
    let mine = open.chat_key().lock().await.public();
    let mut keys = stored(&open, &peer).await?;
    let pin_new = matched && new_keys;
    if pin_new {
        keys.replaced = Some(keys.pinned);
        keys.pinned = keys.candidate.ok_or_else(no_new_keys)?;
        keys.candidate = None;
    }
    if matched {
        keys.verified = Some((mine, keys.pinned));
        keys.mismatch = false;
    } else {
        keys.mismatch = true;
    }
    finish(service, &open, &peer, &keys, pin_new).await
}

/// Use new keys anyway: the other key is pinned, unverified.
pub async fn accept_new_keys(service: &PrivateService, peer: &str) -> Result<(), CoreError> {
    let open = service.opened()?;
    let peer = parse_hex::<KEY_LEN>(peer, "That isn't someone's key.")?;
    let mut keys = stored(&open, &peer).await?;
    keys.replaced = Some(keys.pinned);
    keys.pinned = keys.candidate.ok_or_else(no_new_keys)?;
    keys.candidate = None;
    keys.verified = None;
    keys.mismatch = false;
    finish(service, &open, &peer, &keys, true).await
}

/// Saves `keys`; a newly pinned key gets "*Name*'s keys changed." and the
/// device lists are fetched again.
async fn finish(
    service: &PrivateService,
    open: &Open,
    peer: &[u8; KEY_LEN],
    keys: &ContactKeys,
    pinned_new: bool,
) -> Result<(), CoreError> {
    let sealer = open.store().sealer();
    let mut tx = open
        .store()
        .pool()
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(PrivateError::from)?;
    save(&mut tx, peer, keys).await?;
    let added = if pinned_new {
        divider(&mut tx, sealer, peer, KEYS_CHANGED).await?
    } else {
        None
    };
    tx.commit().await.map_err(PrivateError::from)?;
    if pinned_new {
        service.device_cache().clear();
    }
    if let Some(id) = added {
        show(service, open, id).await;
    }
    emit_contact(service, open, peer).await;
    Ok(())
}

async fn stored(open: &Open, peer: &[u8; KEY_LEN]) -> Result<ContactKeys, CoreError> {
    let mut conn = open
        .store()
        .pool()
        .acquire()
        .await
        .map_err(PrivateError::from)?;
    load(&mut conn, peer)
        .await?
        .ok_or_else(|| CoreError::InvalidInput {
            message: "You haven't exchanged keys with them yet.".to_owned(),
        })
}

/// A line from their side in the chat, if it has any messages.
async fn divider(
    conn: &mut SqliteConnection,
    sealer: &Sealer,
    peer: &[u8; KEY_LEN],
    kind: i64,
) -> Result<Option<[u8; 16]>, PrivateError> {
    let has_any = sqlx::query_scalar::<_, i64>("SELECT 1 FROM messages WHERE peer = $1 LIMIT 1")
        .bind(peer.as_slice())
        .fetch_optional(&mut *conn)
        .await?
        .is_some();
    if !has_any {
        return Ok(None);
    }
    let id = inbox::random_id()?;
    let inserted = inbox::insert(
        conn,
        sealer,
        Insert {
            id,
            peer: *peer,
            outgoing: false,
            kind,
            body: None,
            server_id: None,
            sent_at: now_ms(),
            state: RECEIVED,
        },
    )
    .await?;
    Ok(inserted.then_some(id))
}

async fn show(service: &PrivateService, open: &Open, id: [u8; 16]) {
    match outbox::view(service, open, id).await {
        Ok(message) => service.emit(PrivateEvent::MessageAdded(message)),
        Err(error) => tracing::warn!(%error, "showing a keys line failed"),
    }
}

async fn emit_contact(service: &PrivateService, open: &Open, peer: &[u8; KEY_LEN]) {
    if let Err(error) = service.emit_contact_of(open, peer).await {
        tracing::warn!(%error, "showing a changed contact failed");
    }
}

fn no_new_keys() -> CoreError {
    CoreError::InvalidInput {
        message: "Their keys haven't changed.".to_owned(),
    }
}

/// One person's listed devices, sorted by the chat key that signed them.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Classified {
    /// The key to pin now: first contact only.
    pub pin: Option<[u8; KEY_LEN]>,
    /// Signed by the pinned key: they get messages.
    pub trusted: Vec<VerifiedDevice>,
    /// Signed by another key: they get nothing.
    pub held: Vec<VerifiedDevice>,
    /// The other key on the most held devices (ties: the lowest), offered
    /// in the chat's banner.
    pub candidate: Option<[u8; KEY_LEN]>,
}

/// Sorts `listed` by `pinned`; with nothing pinned yet, the key on the most
/// devices (ties: the lowest) is pinned (design 2026-11-01 §8.1).
pub fn classify(pinned: Option<[u8; KEY_LEN]>, listed: &[VerifiedDevice]) -> Classified {
    let pin = match pinned {
        Some(_) => None,
        None => most_used(listed.iter()),
    };
    let Some(trusted_key) = pinned.or(pin) else {
        return Classified::default();
    };
    let (trusted, held): (Vec<VerifiedDevice>, Vec<VerifiedDevice>) = listed
        .iter()
        .cloned()
        .partition(|device| device.chat_key == trusted_key);
    let candidate = most_used(held.iter());
    Classified {
        pin,
        trusted,
        held,
        candidate,
    }
}

fn most_used<'a>(devices: impl Iterator<Item = &'a VerifiedDevice>) -> Option<[u8; KEY_LEN]> {
    let mut counts: BTreeMap<[u8; KEY_LEN], usize> = BTreeMap::new();
    for device in devices {
        *counts.entry(device.chat_key).or_default() += 1;
    }
    // Ascending keys, so the first of the most used is the lowest.
    let most = counts.values().copied().max()?;
    counts
        .into_iter()
        .find(|(_, count)| *count == most)
        .map(|(key, _)| key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: u8, chat_key: u8) -> VerifiedDevice {
        VerifiedDevice {
            device_id: [id; 16],
            curve25519: [id; 32],
            ed25519: [id; 32],
            chat_key: [chat_key; 32],
        }
    }

    #[test]
    fn first_contact_pins_the_key_on_most_devices() {
        let listed = [device(1, 9), device(2, 7), device(3, 7)];

        let classified = classify(None, &listed);

        assert_eq!(classified.pin, Some([7; 32]));
        assert_eq!(classified.trusted, vec![device(2, 7), device(3, 7)]);
        assert_eq!(classified.held, vec![device(1, 9)]);
        assert_eq!(classified.candidate, Some([9; 32]));
    }

    #[test]
    fn ties_pin_the_lowest_key() {
        let listed = [device(1, 9), device(2, 4)];

        let classified = classify(None, &listed);

        assert_eq!(classified.pin, Some([4; 32]));
        assert_eq!(classified.trusted, vec![device(2, 4)]);
    }

    #[test]
    fn devices_with_another_key_are_held_and_offered() {
        let listed = [device(1, 5), device(2, 6), device(3, 6), device(4, 8)];

        let classified = classify(Some([5; 32]), &listed);

        assert_eq!(classified.pin, None);
        assert_eq!(classified.trusted, vec![device(1, 5)]);
        assert_eq!(classified.held.len(), 3);
        assert_eq!(classified.candidate, Some([6; 32]));
    }

    #[test]
    fn a_device_with_the_pinned_key_is_trusted() {
        let classified = classify(Some([5; 32]), &[device(1, 5), device(2, 5)]);

        assert_eq!(classified.trusted.len(), 2);
        assert!(classified.held.is_empty());
        assert_eq!(classified.candidate, None);
    }

    #[test]
    fn nobody_listed_pins_nothing() {
        let classified = classify(None, &[]);

        assert_eq!(classified.pin, None);
        assert!(classified.trusted.is_empty());
        assert_eq!(classified.candidate, None);
    }
}
