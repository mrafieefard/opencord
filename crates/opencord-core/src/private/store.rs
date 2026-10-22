//! The private messages database (design 2026-10-22 §11): one SQLite file
//! per identity under `<data_dir>/private/`, its secret columns sealed with
//! keys derived from the keychain's master key.

use std::path::Path;
use std::time::Duration;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};

use super::PrivateError;
use super::seal::{Sealer, derive_keys};

pub const PRIVATE_DIR: &str = "private";
const MAX_CONNECTIONS: u32 = 2;
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const KEY_CHECK_SETTING: &str = "key_check";
const KEY_CHECK_LABEL: &str = "settings.key_check";
const KEY_CHECK_PLAINTEXT: &[u8] = b"opencord-private-key-check";
/// The database file and SQLite's two companions, moved aside together.
const FILE_SUFFIXES: [&str; 3] = ["", "-wal", "-shm"];

pub struct PrivateStore {
    pool: SqlitePool,
    sealer: Sealer,
    pickle_key: [u8; 32],
}

pub struct Opened {
    pub store: PrivateStore,
    /// The old database couldn't be opened and was moved aside.
    pub started_fresh: bool,
}

impl PrivateStore {
    /// Opens `identity_public_key`'s database, creating it if needed. One
    /// that can't be read with `master_key`, or doesn't open or migrate, is
    /// moved aside and replaced by a new one.
    pub async fn open(
        data_dir: &Path,
        identity_public_key: &[u8; 32],
        master_key: &[u8; 32],
        now_ms: i64,
    ) -> Result<Opened, PrivateError> {
        let keys = derive_keys(master_key);
        let sealer = Sealer::new(&keys.seal);
        let dir = data_dir.join(PRIVATE_DIR);
        tokio::fs::create_dir_all(&dir).await?;
        let path = dir.join(format!("{}.db", hex::encode(identity_public_key)));

        let (pool, started_fresh) = match connect_checked(&path, &sealer).await {
            Ok(pool) => (pool, false),
            Err(error) => {
                tracing::warn!(
                    %error,
                    "the private messages database can't be opened; moving it aside and starting fresh"
                );
                move_aside(&path, now_ms).await?;
                (connect_checked(&path, &sealer).await?, true)
            }
        };
        Ok(Opened {
            store: Self {
                pool,
                sealer,
                pickle_key: keys.pickle,
            },
            started_fresh,
        })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub fn sealer(&self) -> &Sealer {
        &self.sealer
    }

    pub fn pickle_key(&self) -> &[u8; 32] {
        &self.pickle_key
    }

    pub async fn setting(&self, key: &str) -> Result<Option<String>, PrivateError> {
        Ok(setting(&self.pool, key).await?)
    }

    pub async fn set_setting(&self, key: &str, value: &str) -> Result<(), PrivateError> {
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES ($1, $2) \
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn close(self) {
        self.pool.close().await;
    }
}

/// Opens and migrates the database at `path`, then checks that `sealer`
/// opens its key check, or gives a new database one. The pool is closed
/// again when anything fails, so the files can be moved.
async fn connect_checked(path: &Path, sealer: &Sealer) -> Result<SqlitePool, PrivateError> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(BUSY_TIMEOUT);
    let pool = SqlitePoolOptions::new()
        .max_connections(MAX_CONNECTIONS)
        .connect_with(options)
        .await?;
    match migrate_and_check(&pool, sealer).await {
        Ok(()) => Ok(pool),
        Err(error) => {
            pool.close().await;
            Err(error)
        }
    }
}

async fn migrate_and_check(pool: &SqlitePool, sealer: &Sealer) -> Result<(), PrivateError> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .map_err(|error| PrivateError::Storage(error.to_string()))?;
    match setting(pool, KEY_CHECK_SETTING).await? {
        Some(check) => {
            let sealed = hex::decode(&check)
                .map_err(|_| PrivateError::Invalid("the key check isn't hex".to_owned()))?;
            sealer.open(KEY_CHECK_LABEL, b"", &sealed)?;
        }
        None => {
            let check = hex::encode(sealer.seal(KEY_CHECK_LABEL, b"", KEY_CHECK_PLAINTEXT));
            sqlx::query("INSERT INTO settings (key, value) VALUES ($1, $2)")
                .bind(KEY_CHECK_SETTING)
                .bind(check)
                .execute(pool)
                .await?;
        }
    }
    Ok(())
}

/// Renames the database and its WAL files, whichever exist, by appending
/// `.unreadable-<now_ms>`. They are kept, never deleted.
async fn move_aside(path: &Path, now_ms: i64) -> Result<(), PrivateError> {
    for suffix in FILE_SUFFIXES {
        let mut from = path.as_os_str().to_owned();
        from.push(suffix);
        let mut to = from.clone();
        to.push(format!(".unreadable-{now_ms}"));
        match tokio::fs::rename(&from, &to).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

async fn setting(pool: &SqlitePool, key: &str) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT value FROM settings WHERE key = $1")
        .bind(key)
        .fetch_optional(pool)
        .await
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    const ALICE: [u8; 32] = [0xa1; 32];
    const BOB: [u8; 32] = [0xb0; 32];

    fn database_path(dir: &Path, identity: &[u8; 32]) -> PathBuf {
        dir.join(PRIVATE_DIR)
            .join(format!("{}.db", hex::encode(identity)))
    }

    fn unreadable_files(dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir.join(PRIVATE_DIR))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".unreadable-"))
            .collect()
    }

    #[tokio::test]
    async fn a_database_opens_again_with_its_key() {
        let dir = tempfile::tempdir().unwrap();
        let opened = PrivateStore::open(dir.path(), &ALICE, &[1; 32], 1_000)
            .await
            .unwrap();
        assert!(!opened.started_fresh);
        opened
            .store
            .set_setting("privacy", "friends")
            .await
            .unwrap();
        opened.store.close().await;

        let opened = PrivateStore::open(dir.path(), &ALICE, &[1; 32], 2_000)
            .await
            .unwrap();

        assert!(!opened.started_fresh);
        assert_eq!(
            opened.store.setting("privacy").await.unwrap().as_deref(),
            Some("friends")
        );
        assert!(unreadable_files(dir.path()).is_empty());
        opened.store.close().await;
    }

    #[tokio::test]
    async fn a_wrong_key_moves_the_database_aside() {
        let dir = tempfile::tempdir().unwrap();
        let opened = PrivateStore::open(dir.path(), &ALICE, &[1; 32], 1_000)
            .await
            .unwrap();
        opened
            .store
            .set_setting("privacy", "friends")
            .await
            .unwrap();
        opened.store.close().await;

        let opened = PrivateStore::open(dir.path(), &ALICE, &[2; 32], 2_000)
            .await
            .unwrap();

        assert!(opened.started_fresh);
        assert_eq!(opened.store.setting("privacy").await.unwrap(), None);
        let moved = format!("{}.db.unreadable-2000", hex::encode(ALICE));
        assert!(unreadable_files(dir.path()).contains(&moved));
        // The new database checks against the new key.
        opened.store.close().await;
        let opened = PrivateStore::open(dir.path(), &ALICE, &[2; 32], 3_000)
            .await
            .unwrap();
        assert!(!opened.started_fresh);
        opened.store.close().await;
    }

    #[tokio::test]
    async fn a_damaged_file_is_moved_aside() {
        let dir = tempfile::tempdir().unwrap();
        let path = database_path(dir.path(), &ALICE);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, [0x5a; 4096]).unwrap();

        let opened = PrivateStore::open(dir.path(), &ALICE, &[1; 32], 5_000)
            .await
            .unwrap();

        assert!(opened.started_fresh);
        let moved = dir
            .path()
            .join(PRIVATE_DIR)
            .join(format!("{}.db.unreadable-5000", hex::encode(ALICE)));
        assert_eq!(std::fs::read(moved).unwrap(), [0x5a; 4096]);
        opened
            .store
            .set_setting("privacy", "everyone")
            .await
            .unwrap();
        assert_eq!(
            opened.store.setting("privacy").await.unwrap().as_deref(),
            Some("everyone")
        );
        opened.store.close().await;
    }

    #[tokio::test]
    async fn each_identity_has_its_own_database() {
        let dir = tempfile::tempdir().unwrap();
        let alice = PrivateStore::open(dir.path(), &ALICE, &[1; 32], 1_000)
            .await
            .unwrap();
        let bob = PrivateStore::open(dir.path(), &BOB, &[1; 32], 1_000)
            .await
            .unwrap();

        alice.store.set_setting("privacy", "friends").await.unwrap();

        assert_eq!(bob.store.setting("privacy").await.unwrap(), None);
        assert!(database_path(dir.path(), &ALICE).exists());
        assert!(database_path(dir.path(), &BOB).exists());
        alice.store.close().await;
        bob.store.close().await;
    }

    #[tokio::test]
    async fn migrations_create_every_table() {
        let dir = tempfile::tempdir().unwrap();
        let opened = PrivateStore::open(dir.path(), &ALICE, &[1; 32], 1_000)
            .await
            .unwrap();

        let mut tables: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type = 'table' \
             AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '_sqlx_%'",
        )
        .fetch_all(opened.store.pool())
        .await
        .unwrap();
        tables.sort();

        assert_eq!(
            tables,
            [
                "contacts",
                "device",
                "messages",
                "peer_devices",
                "received_events",
                "sessions",
                "settings"
            ]
        );
        opened.store.close().await;
    }
}
