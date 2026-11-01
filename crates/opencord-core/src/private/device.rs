//! This device's Olm account (design 2026-10-22 §10.2): its keys, vouched for
//! by the person's chat key (design 2026-11-01 §6.2), and the prekeys others
//! start sessions with.

use opencord_common::private::{DEVICE_ID_LEN, FALLBACK_ROTATION_MS, prekey_payload};
use opencord_proto::v1 as proto;
use sqlx::SqliteConnection;
use vodozemac::Curve25519PublicKey;
use vodozemac::olm::{Account, AccountPickle};

use super::PrivateError;
use super::chat_key::ChatKey;
use super::store::PrivateStore;

/// This device's account and id.
///
/// vodozemac only reports the fallback key while it's unpublished, so this
/// account's keys are never marked published: each batch of one-time keys is
/// taken from what generating it returns. Never call
/// `Account::mark_keys_as_published`.
pub struct Device {
    id: [u8; DEVICE_ID_LEN],
    account: Account,
    fallback: Curve25519PublicKey,
    fallback_created_at: i64,
}

impl Device {
    /// Loads this database's device, or creates and stores one.
    pub async fn load_or_create(store: &PrivateStore, now_ms: i64) -> Result<Device, PrivateError> {
        if let Some(device) = Self::load(store).await? {
            return Ok(device);
        }
        let device = Self::create(now_ms);
        let inserted = sqlx::query(
            "INSERT INTO device (id, device_id, account, fallback_created_at, created_at) \
             VALUES (1, $1, $2, $3, $4) ON CONFLICT (id) DO NOTHING",
        )
        .bind(device.id.as_slice())
        .bind(device.pickle(store.pickle_key()))
        .bind(device.fallback_created_at)
        .bind(now_ms)
        .execute(store.pool())
        .await?
        .rows_affected()
            == 1;
        if inserted {
            return Ok(device);
        }
        // Another caller stored its device first: that one is this device.
        Self::load(store)
            .await?
            .ok_or_else(|| PrivateError::Storage("the device row disappeared".to_owned()))
    }

    fn create(now_ms: i64) -> Self {
        let mut id = [0; DEVICE_ID_LEN];
        // The OS random source failing leaves nothing secure to fall back to.
        getrandom::fill(&mut id).expect("the operating system's random source is unavailable");
        let mut account = Account::new();
        account.generate_fallback_key();
        let fallback =
            unpublished_fallback(&account).expect("a fallback key just generated is unpublished");
        Self {
            id,
            account,
            fallback,
            fallback_created_at: now_ms,
        }
    }

    async fn load(store: &PrivateStore) -> Result<Option<Self>, PrivateError> {
        let row: Option<(Vec<u8>, String, i64)> = sqlx::query_as(
            "SELECT device_id, account, fallback_created_at FROM device WHERE id = 1",
        )
        .fetch_optional(store.pool())
        .await?;
        let Some((id, pickle, fallback_created_at)) = row else {
            return Ok(None);
        };
        let id = <[u8; DEVICE_ID_LEN]>::try_from(id.as_slice())
            .map_err(|_| PrivateError::Invalid("the stored device id isn't 16 bytes".to_owned()))?;
        let pickle =
            AccountPickle::from_encrypted(&pickle, store.pickle_key()).map_err(|error| {
                PrivateError::Crypto(format!("the device's account doesn't open: {error}"))
            })?;
        let account = Account::from_pickle(pickle);
        let fallback = unpublished_fallback(&account)
            .ok_or_else(|| PrivateError::Crypto("the device has no fallback key".to_owned()))?;
        Ok(Some(Self {
            id,
            account,
            fallback,
            fallback_created_at,
        }))
    }

    pub fn id(&self) -> [u8; 16] {
        self.id
    }

    pub fn curve25519(&self) -> [u8; 32] {
        self.account.curve25519_key().to_bytes()
    }

    pub fn ed25519(&self) -> [u8; 32] {
        *self.account.ed25519_key().as_bytes()
    }

    /// The device's keys with the chat key's signature over them.
    pub fn keys(&self, chat_key: &ChatKey) -> proto::DeviceKeys {
        let curve25519 = self.curve25519();
        let ed25519 = self.ed25519();
        proto::DeviceKeys {
            device_id: self.id.to_vec(),
            curve25519: curve25519.to_vec(),
            ed25519: ed25519.to_vec(),
            signature: chat_key
                .sign_device(&self.id, &curve25519, &ed25519)
                .to_vec(),
            chat_key: chat_key.public().to_vec(),
        }
    }

    pub fn fallback_prekey(&self) -> proto::Prekey {
        self.prekey(&self.fallback, true)
    }

    /// `count` new one-time keys, each signed by the device.
    pub fn new_one_time_keys(&mut self, count: usize) -> Vec<proto::Prekey> {
        let created = self.account.generate_one_time_keys(count).created;
        created.iter().map(|key| self.prekey(key, false)).collect()
    }

    /// Makes a new fallback key 30 days after the current one. vodozemac
    /// keeps the previous one too, so sessions started with it still open
    /// until the next rotation.
    pub fn rotate_fallback_if_due(&mut self, now_ms: i64) -> bool {
        if now_ms.saturating_sub(self.fallback_created_at) < FALLBACK_ROTATION_MS {
            return false;
        }
        self.account.generate_fallback_key();
        self.fallback = unpublished_fallback(&self.account)
            .expect("a fallback key just generated is unpublished");
        self.fallback_created_at = now_ms;
        true
    }

    fn prekey(&self, key: &Curve25519PublicKey, fallback: bool) -> proto::Prekey {
        let signature = self
            .account
            .sign(prekey_payload(&self.id, key.as_bytes(), fallback));
        proto::Prekey {
            key: key.to_vec(),
            signature: signature.to_bytes().to_vec(),
        }
    }

    fn pickle(&self, pickle_key: &[u8; 32]) -> String {
        self.account.pickle().encrypt(pickle_key)
    }

    pub fn account_mut(&mut self) -> &mut Account {
        &mut self.account
    }

    pub(super) fn account(&self) -> &Account {
        &self.account
    }

    /// A copy to change, which replaces this one once it's saved.
    pub(super) fn duplicate(&self) -> Device {
        Device {
            id: self.id,
            account: Account::from_pickle(self.account.pickle()),
            fallback: self.fallback,
            fallback_created_at: self.fallback_created_at,
        }
    }

    /// Writes the encrypted account inside the caller's transaction.
    pub async fn save(
        &self,
        conn: &mut SqliteConnection,
        pickle_key: &[u8; 32],
    ) -> Result<(), PrivateError> {
        let updated = sqlx::query(
            "UPDATE device SET account = $1, fallback_created_at = $2 \
             WHERE id = 1 AND device_id = $3",
        )
        .bind(self.pickle(pickle_key))
        .bind(self.fallback_created_at)
        .bind(self.id.as_slice())
        .execute(&mut *conn)
        .await?
        .rows_affected();
        if updated != 1 {
            return Err(PrivateError::Storage(
                "this device is no longer the database's device".to_owned(),
            ));
        }
        Ok(())
    }
}

/// The current fallback key, which vodozemac reports only while it's
/// unpublished.
fn unpublished_fallback(account: &Account) -> Option<Curve25519PublicKey> {
    account.fallback_key().into_values().next()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use opencord_common::private::{verify_device_v2, verify_prekey};

    use crate::identity::Identity;

    use super::*;

    const MASTER: [u8; 32] = [9; 32];

    async fn open(dir: &std::path::Path, identity: &Identity) -> PrivateStore {
        PrivateStore::open(dir, &identity.public_key(), &MASTER, 0)
            .await
            .unwrap()
            .store
    }

    fn verifies_as(device: &Device, prekey: &proto::Prekey, fallback: bool) -> bool {
        verify_prekey(
            &device.ed25519(),
            &device.id(),
            &prekey.key,
            fallback,
            &prekey.signature,
        )
    }

    #[tokio::test]
    async fn device_keys_carry_a_valid_chat_key_signature() {
        let dir = tempfile::tempdir().unwrap();
        let identity = Identity::generate();
        let store = open(dir.path(), &identity).await;
        let device = Device::load_or_create(&store, 0).await.unwrap();
        let chat_key = ChatKey::load_or_create(&store, 0).await.unwrap();

        let keys = device.keys(&chat_key);

        assert_eq!(keys.device_id, device.id());
        assert_eq!(keys.curve25519, device.curve25519());
        assert_eq!(keys.ed25519, device.ed25519());
        assert_eq!(keys.chat_key, chat_key.public());
        assert!(verify_device_v2(
            &chat_key.public(),
            &keys.device_id,
            &keys.curve25519,
            &keys.ed25519,
            &keys.signature
        ));
        assert!(!verify_device_v2(
            &identity.public_key(),
            &keys.device_id,
            &keys.curve25519,
            &keys.ed25519,
            &keys.signature
        ));
        store.close().await;
    }

    #[tokio::test]
    async fn one_time_keys_are_signed_and_never_repeat() {
        let dir = tempfile::tempdir().unwrap();
        let identity = Identity::generate();
        let store = open(dir.path(), &identity).await;
        let mut device = Device::load_or_create(&store, 0).await.unwrap();

        let first = device.new_one_time_keys(20);
        let mut conn = store.pool().acquire().await.unwrap();
        device.save(&mut conn, store.pickle_key()).await.unwrap();
        drop(conn);
        let mut device = Device::load_or_create(&store, 0).await.unwrap();
        let second = device.new_one_time_keys(20);
        let fallback = device.fallback_prekey();

        assert_eq!(first.len(), 20);
        assert_eq!(second.len(), 20);
        let keys: HashSet<&[u8]> = first
            .iter()
            .chain(&second)
            .map(|prekey| prekey.key.as_slice())
            .collect();
        assert_eq!(keys.len(), 40);
        assert!(!keys.contains(fallback.key.as_slice()));
        for prekey in first.iter().chain(&second) {
            assert!(verifies_as(&device, prekey, false));
            assert!(!verifies_as(&device, prekey, true));
        }
        assert!(verifies_as(&device, &fallback, true));
        assert!(!verifies_as(&device, &fallback, false));
        store.close().await;
    }

    #[tokio::test]
    async fn the_fallback_key_rotates_after_thirty_days() {
        let dir = tempfile::tempdir().unwrap();
        let identity = Identity::generate();
        let store = open(dir.path(), &identity).await;
        let mut device = Device::load_or_create(&store, 1_000).await.unwrap();
        let first = device.fallback_prekey();

        assert!(!device.rotate_fallback_if_due(1_000 + FALLBACK_ROTATION_MS - 1));
        assert_eq!(device.fallback_prekey().key, first.key);
        assert!(device.rotate_fallback_if_due(1_000 + FALLBACK_ROTATION_MS));
        let second = device.fallback_prekey();
        assert_ne!(second.key, first.key);
        assert!(verifies_as(&device, &second, true));
        // The next rotation counts from this one.
        assert!(!device.rotate_fallback_if_due(1_000 + 2 * FALLBACK_ROTATION_MS - 1));

        let mut conn = store.pool().acquire().await.unwrap();
        device.save(&mut conn, store.pickle_key()).await.unwrap();
        drop(conn);
        let mut reloaded = Device::load_or_create(&store, 0).await.unwrap();
        assert_eq!(reloaded.fallback_prekey().key, second.key);
        assert!(!reloaded.rotate_fallback_if_due(1_000 + 2 * FALLBACK_ROTATION_MS - 1));
        assert!(reloaded.rotate_fallback_if_due(1_000 + 2 * FALLBACK_ROTATION_MS));
        store.close().await;
    }

    #[tokio::test]
    async fn the_account_survives_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let identity = Identity::generate();
        let store = open(dir.path(), &identity).await;
        let device = Device::load_or_create(&store, 0).await.unwrap();
        let (id, curve25519, ed25519) = (device.id(), device.curve25519(), device.ed25519());
        let fallback = device.fallback_prekey();
        drop(device);
        store.close().await;

        let store = open(dir.path(), &identity).await;
        let device = Device::load_or_create(&store, 5_000).await.unwrap();

        assert_eq!(device.id(), id);
        assert_eq!(device.curve25519(), curve25519);
        assert_eq!(device.ed25519(), ed25519);
        assert_eq!(device.fallback_prekey().key, fallback.key);
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device")
            .fetch_one(store.pool())
            .await
            .unwrap();
        assert_eq!(rows, 1);
        store.close().await;
    }

    #[tokio::test]
    async fn saving_a_replaced_device_fails() {
        let dir = tempfile::tempdir().unwrap();
        let identity = Identity::generate();
        let store = open(dir.path(), &identity).await;
        let device = Device::load_or_create(&store, 0).await.unwrap();
        sqlx::query("UPDATE device SET device_id = $1")
            .bind([0u8; 16].as_slice())
            .execute(store.pool())
            .await
            .unwrap();

        let mut conn = store.pool().acquire().await.unwrap();
        let saved = device.save(&mut conn, store.pickle_key()).await;

        assert!(matches!(saved, Err(PrivateError::Storage(_))));
        drop(conn);
        store.close().await;
    }
}
