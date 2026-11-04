//! This person's chat key (design 2026-11-01 §6.1): it signs their devices,
//! and stays on them. It has nothing to do with the identity key.

use opencord_common::private::{DEVICE_ID_LEN, KEY_LEN, SIGNATURE_LEN, SigningKey, sign_device_v2};
use sqlx::SqliteConnection;

use super::PrivateError;
use super::seal::Sealer;
use super::store::PrivateStore;

/// The label the secret is sealed with; the row is `[1]`.
const SECRET_LABEL: &str = "chat_key.secret";
const ROW: &[u8] = &[1];

pub struct ChatKey {
    key: SigningKey,
}

impl ChatKey {
    /// Loads this database's chat key, or makes and stores one.
    pub async fn load_or_create(
        store: &PrivateStore,
        now_ms: i64,
    ) -> Result<ChatKey, PrivateError> {
        if let Some(key) = Self::load(store).await? {
            return Ok(key);
        }
        let mut secret = [0; KEY_LEN];
        // The OS random source failing leaves nothing secure to fall back to.
        getrandom::fill(&mut secret).expect("the operating system's random source is unavailable");
        let key = Self::from_secret(secret);
        sqlx::query(
            "INSERT INTO chat_key (id, public, secret, created_at) VALUES (1, $1, $2, $3) \
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(key.public().as_slice())
        .bind(store.sealer().seal(SECRET_LABEL, ROW, &secret))
        .bind(now_ms)
        .execute(store.pool())
        .await?;
        // Another caller may have stored its key first: that one is the key.
        Self::load(store)
            .await?
            .ok_or_else(|| PrivateError::Storage("the chat key row disappeared".to_owned()))
    }

    async fn load(store: &PrivateStore) -> Result<Option<ChatKey>, PrivateError> {
        let sealed: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT secret FROM chat_key WHERE id = 1")
                .fetch_optional(store.pool())
                .await?;
        let Some(sealed) = sealed else {
            return Ok(None);
        };
        let secret = store.sealer().open(SECRET_LABEL, ROW, &sealed)?;
        let secret = <[u8; KEY_LEN]>::try_from(secret.as_slice())
            .map_err(|_| PrivateError::Invalid("the stored chat key isn't 32 bytes".to_owned()))?;
        Ok(Some(Self::from_secret(secret)))
    }

    fn from_secret(secret: [u8; KEY_LEN]) -> Self {
        Self {
            key: SigningKey::from_bytes(&secret),
        }
    }

    pub fn public(&self) -> [u8; KEY_LEN] {
        self.key.verifying_key().to_bytes()
    }

    /// The private half, only for giving it to another of this person's
    /// devices (§7.2).
    pub fn secret(&self) -> [u8; KEY_LEN] {
        self.key.to_bytes()
    }

    pub fn sign_device(
        &self,
        device_id: &[u8; DEVICE_ID_LEN],
        curve25519: &[u8; KEY_LEN],
        ed25519: &[u8; KEY_LEN],
    ) -> [u8; SIGNATURE_LEN] {
        sign_device_v2(&self.key, device_id, curve25519, ed25519)
    }

    /// Takes the chat key another of this person's devices gave, inside the
    /// caller's transaction.
    pub async fn replace(
        &mut self,
        conn: &mut SqliteConnection,
        sealer: &Sealer,
        secret: [u8; KEY_LEN],
    ) -> Result<(), PrivateError> {
        let key = Self::from_secret(secret);
        let updated = sqlx::query("UPDATE chat_key SET public = $1, secret = $2 WHERE id = 1")
            .bind(key.public().as_slice())
            .bind(sealer.seal(SECRET_LABEL, ROW, &secret))
            .execute(&mut *conn)
            .await?
            .rows_affected();
        if updated != 1 {
            return Err(PrivateError::Storage(
                "there's no chat key to replace".to_owned(),
            ));
        }
        *self = key;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::private::store::PrivateStore;

    async fn store(dir: &std::path::Path) -> PrivateStore {
        PrivateStore::open(dir, &[0xa1; 32], &[1; 32], 0)
            .await
            .unwrap()
            .store
    }

    #[tokio::test]
    async fn a_chat_key_is_made_once_and_kept_sealed() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path()).await;

        let first = ChatKey::load_or_create(&store, 1).await.unwrap();
        let again = ChatKey::load_or_create(&store, 2).await.unwrap();

        assert_eq!(first.public(), again.public());
        assert_eq!(first.secret(), again.secret());
        let stored: Vec<u8> = sqlx::query_scalar("SELECT secret FROM chat_key")
            .fetch_one(store.pool())
            .await
            .unwrap();
        assert!(!stored.windows(32).any(|window| window == first.secret()));
        store.close().await;
    }

    #[tokio::test]
    async fn replacing_the_chat_key_changes_its_signatures() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path()).await;
        let mut key = ChatKey::load_or_create(&store, 1).await.unwrap();
        let (id, curve, ed) = ([1; 16], [2; 32], [3; 32]);
        let before = key.public();

        let mut conn = store.pool().acquire().await.unwrap();
        key.replace(&mut conn, store.sealer(), [9; 32])
            .await
            .unwrap();
        drop(conn);

        assert_ne!(key.public(), before);
        let signature = key.sign_device(&id, &curve, &ed);
        assert!(opencord_common::private::verify_device_v2(
            &key.public(),
            &id,
            &curve,
            &ed,
            &signature
        ));
        let reloaded = ChatKey::load_or_create(&store, 2).await.unwrap();
        assert_eq!(reloaded.public(), key.public());
        store.close().await;
    }
}
