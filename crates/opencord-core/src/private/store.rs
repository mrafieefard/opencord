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
    /// The database was from before chat keys, and its chats were removed
    /// (design 2026-11-01 §10).
    pub keys_reset: bool,
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

        let ((pool, keys_reset), started_fresh) = match connect_checked(&path, &sealer).await {
            Ok(opened) => (opened, false),
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
            keys_reset,
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
/// again when anything fails, so the files can be moved. Also says whether
/// this opening removed chats from before chat keys.
async fn connect_checked(path: &Path, sealer: &Sealer) -> Result<(SqlitePool, bool), PrivateError> {
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
        Ok(keys_reset) => Ok((pool, keys_reset)),
        Err(error) => {
            pool.close().await;
            Err(error)
        }
    }
}

async fn migrate_and_check(pool: &SqlitePool, sealer: &Sealer) -> Result<bool, PrivateError> {
    let keys_reset = from_before_chat_keys(pool).await?;
    // A newer build may have migrated further: switching to an older
    // track installs an older build (docs/updater.md), which must still
    // open the database rather than set it aside. Migrations only add.
    let mut migrator = sqlx::migrate!("./migrations");
    migrator.set_ignore_missing(true);
    migrator
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
    Ok(keys_reset)
}

/// Whether the database has the first migration but not the chat keys one
/// (0002), which removes its chats.
async fn from_before_chat_keys(pool: &SqlitePool) -> Result<bool, PrivateError> {
    let tracked: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations'",
    )
    .fetch_optional(pool)
    .await?;
    if tracked.is_none() {
        return Ok(false);
    }
    let versions: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE version IN (1, 2)")
            .fetch_all(pool)
            .await?;
    Ok(versions == [1])
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
    async fn a_database_from_before_chat_keys_is_reset_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = database_path(dir.path(), &ALICE);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(options).await.unwrap();
        // Only the first migration, as a build from before chat keys left it.
        let full = sqlx::migrate!("./migrations");
        let first = sqlx::migrate::Migrator {
            migrations: std::borrow::Cow::Owned(
                full.migrations
                    .iter()
                    .filter(|m| m.version <= 1)
                    .cloned()
                    .collect(),
            ),
            ignore_missing: full.ignore_missing,
            locking: full.locking,
            create_schemas: full.create_schemas.clone(),
            no_tx: full.no_tx,
            table_name: full.table_name.clone(),
        };
        first.run(&pool).await.unwrap();
        for statement in [
            "INSERT INTO device (id, device_id, account, fallback_created_at, created_at) \
             VALUES (1, X'01', 'pickle', 0, 0)",
            "INSERT INTO contacts (public_key, friend_state, conversation, relay_server_id, \
             last_activity_at, last_read_at, muted, updated_at) \
             VALUES (X'B0', 3, 2, 'ab', 5, 5, 1, 0)",
            "INSERT INTO messages (id, peer, outgoing, kind, sent_at, state) \
             VALUES (X'01', X'B0', 1, 0, 5, 2)",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        pool.close().await;

        let opened = PrivateStore::open(dir.path(), &ALICE, &[1; 32], 1_000)
            .await
            .unwrap();

        assert!(opened.keys_reset);
        assert!(!opened.started_fresh);
        let pool = opened.store.pool();
        for table in ["device", "messages", "sessions", "peer_devices"] {
            let count: i64 =
                sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
                    .fetch_one(pool)
                    .await
                    .unwrap();
            assert_eq!(count, 0, "{table}");
        }
        let contact: (i64, i64, Option<String>, Option<i64>, i64) = sqlx::query_as(
            "SELECT friend_state, conversation, relay_server_id, last_activity_at, muted \
             FROM contacts",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(contact, (3, 0, None, None, 1));
        opened.store.close().await;

        let again = PrivateStore::open(dir.path(), &ALICE, &[1; 32], 2_000)
            .await
            .unwrap();
        assert!(!again.keys_reset);
        again.store.close().await;
    }

    #[tokio::test]
    async fn a_new_database_needs_no_reset_notice() {
        let dir = tempfile::tempdir().unwrap();
        let opened = PrivateStore::open(dir.path(), &ALICE, &[1; 32], 1_000)
            .await
            .unwrap();
        assert!(!opened.keys_reset);
        opened.store.close().await;
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
    async fn a_database_a_newer_build_migrated_still_opens() {
        let dir = tempfile::tempdir().unwrap();
        let opened = PrivateStore::open(dir.path(), &ALICE, &[1; 32], 1_000)
            .await
            .unwrap();
        opened
            .store
            .set_setting("privacy", "friends")
            .await
            .unwrap();
        // What a later build's migration leaves behind.
        sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) \
             VALUES (9999, 'from a newer build', TRUE, X'00', 0)",
        )
        .execute(&opened.store.pool)
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
                "chat_key",
                "contacts",
                "device",
                "messages",
                "own_devices",
                "peer_devices",
                "received_events",
                "sessions",
                "settings"
            ]
        );
        opened.store.close().await;
    }
}
