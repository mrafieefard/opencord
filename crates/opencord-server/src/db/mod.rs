//! SQLite storage. Queries are checked at compile time against the
//! migrations (see `.sqlx/` for the offline data) and avoid SQLite-only
//! syntax so another backend can be added later.

use std::path::Path;
use std::time::Duration;

use opencord_common::permissions::Permissions;
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};

pub mod attachments;
pub mod bans;
pub mod certificates;
pub mod channels;
pub mod domains;
pub mod invites;
pub mod lavalink;
pub mod local_songs;
pub mod members;
pub mod messages;
pub mod meta;
pub mod music;
pub mod private;
pub mod roles;
pub mod sounds;
pub mod storage;
pub mod users;

pub const DATABASE_FILE: &str = "opencord.db";

/// Opens (creating if needed) the database and applies pending migrations.
pub async fn connect(path: &Path) -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

/// Starts a transaction that reads and then writes. `BEGIN IMMEDIATE` takes
/// the write lock now, waiting the busy timeout for it. A plain transaction
/// takes it at its first write, and in WAL mode one that has read by then
/// fails at once with `SQLITE_BUSY_SNAPSHOT` if another connection committed
/// in between. A transaction that only writes doesn't need this (Working
/// Rule 20, D-log of `DeleteMessage`).
pub async fn begin_read_write(
    pool: &SqlitePool,
) -> Result<sqlx::Transaction<'static, sqlx::Sqlite>, sqlx::Error> {
    pool.begin_with("BEGIN IMMEDIATE").await
}

/// Permission sets are stored as their bits reinterpreted as `i64`.
pub fn permissions_to_db(permissions: Permissions) -> i64 {
    i64::from_ne_bytes(permissions.bits().to_ne_bytes())
}

pub fn permissions_from_db(value: i64) -> Permissions {
    Permissions::from_bits_truncate(u64::from_ne_bytes(value.to_ne_bytes()))
}

pub async fn holds_sealed_data(pool: &SqlitePool) -> Result<bool, sqlx::Error> {
    let has_rows: bool = sqlx::query_scalar!(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM domain_certificates
            UNION ALL
            SELECT 1 FROM acme_accounts
            UNION ALL
            SELECT 1 FROM dns_providers
            UNION ALL
            SELECT 1 FROM lavalink_nodes
            UNION ALL
            SELECT 1 FROM attachments
            UNION ALL
            SELECT 1 FROM storage_backends WHERE secret_enc IS NOT NULL
            UNION ALL
            SELECT 1 FROM server_meta WHERE key = 'tunnel.token'
        ) AS "has_rows: bool"
        "#
    )
    .fetch_one(pool)
    .await?;
    Ok(has_rows)
}

#[cfg(test)]
pub(crate) async fn test_pool() -> (SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let pool = connect(&dir.path().join(DATABASE_FILE)).await.unwrap();
    (pool, dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn connect_enables_wal_and_foreign_keys() {
        let (pool, _dir) = test_pool().await;

        let journal: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&pool)
            .await
            .unwrap();
        let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(journal, "wal");
        assert_eq!(foreign_keys, 1);
    }

    /// identify's race (it read the user, the address detection committed,
    /// and its insert failed with SQLITE_BUSY_SNAPSHOT): the transaction now
    /// waits for the other writer instead.
    #[tokio::test]
    async fn a_read_write_transaction_survives_a_commit_in_between() {
        let (pool, _dir) = test_pool().await;
        let mut tx = begin_read_write(&pool).await.unwrap();
        let _: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        let other = {
            let pool = pool.clone();
            tokio::spawn(async move {
                sqlx::query("INSERT INTO lavalink_nodes (id, name, host, port, secure, password_enc, created_at) VALUES (1, 'n', 'h', 1, FALSE, X'00', 0)")
                    .execute(&pool)
                    .await
            })
        };
        // Long enough for the other connection to commit, if it can.
        tokio::time::sleep(Duration::from_millis(200)).await;

        sqlx::query("INSERT INTO lavalink_nodes (id, name, host, port, secure, password_enc, created_at) VALUES (2, 'n', 'h', 1, FALSE, X'00', 0)")
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        other.await.unwrap().unwrap();
    }

    #[test]
    fn administrator_bit_survives_the_round_trip() {
        let permissions = Permissions::ADMINISTRATOR | Permissions::VIEW_CHANNEL;

        assert_eq!(
            permissions_from_db(permissions_to_db(permissions)),
            permissions
        );
    }

    #[tokio::test]
    async fn holds_sealed_data_sees_lavalink_and_storage_secrets() {
        let (pool, _dir) = test_pool().await;
        assert!(!holds_sealed_data(&pool).await.unwrap());

        sqlx::query(
            "INSERT INTO lavalink_nodes (id, name, host, port, secure, password_enc, created_at)
             VALUES (1, 'node', 'localhost', 2333, FALSE, X'00', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        assert!(holds_sealed_data(&pool).await.unwrap());
    }

    #[tokio::test]
    async fn chat_keys_drop_private_devices_and_keep_friends() {
        let dir = tempfile::tempdir().unwrap();
        let options = SqliteConnectOptions::new()
            .filename(dir.path().join(DATABASE_FILE))
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePool::connect_with(options).await.unwrap();
        let full_migrator = sqlx::migrate!("./migrations");
        let before = sqlx::migrate::Migrator {
            migrations: std::borrow::Cow::Owned(
                full_migrator
                    .migrations
                    .iter()
                    .filter(|m| m.version <= 12)
                    .cloned()
                    .collect(),
            ),
            ignore_missing: full_migrator.ignore_missing,
            locking: full_migrator.locking,
            create_schemas: full_migrator.create_schemas.clone(),
            no_tx: full_migrator.no_tx,
            table_name: full_migrator.table_name.clone(),
        };
        before.run(&pool).await.unwrap();
        for statement in [
            "INSERT INTO users (id, public_key, display_name, created_at) \
             VALUES (1, X'01', 'A', 0), (2, X'02', 'B', 0)",
            "INSERT INTO private_devices (user_id, device_id, curve25519, ed25519, signature, \
             fallback_key, fallback_signature, created_at, last_seen_at) \
             VALUES (1, X'0A', X'00', X'00', X'00', X'00', X'00', 0, 0)",
            "INSERT INTO private_one_time_keys (user_id, device_id, key, signature) \
             VALUES (1, X'0A', X'01', X'00')",
            "INSERT INTO private_pending (user_id, device_id, event, created_at) \
             VALUES (1, X'0A', X'00', 0)",
            "INSERT INTO private_recent_sends (user_id, device_id, send_id, created_at) \
             VALUES (1, X'0A', X'01', 0)",
            "INSERT INTO private_friends (owner_id, friend_id) VALUES (1, 2)",
            "INSERT INTO private_friend_requests (from_id, to_id, created_at) VALUES (2, 1, 0)",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }

        full_migrator.run(&pool).await.unwrap();

        for (table, rows) in [
            ("private_devices", 0),
            ("private_one_time_keys", 0),
            ("private_pending", 0),
            ("private_recent_sends", 0),
            ("private_friends", 1),
            ("private_friend_requests", 1),
        ] {
            let count: i64 =
                sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(count, rows, "{table}");
        }
        let chat_key: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT chat_key FROM private_devices LIMIT 1")
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert_eq!(chat_key, None);
    }

    #[tokio::test]
    async fn migration_gives_everyone_attach_files_voice_and_50_mb() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join(DATABASE_FILE);
        let options = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePool::connect_with(options).await.unwrap();

        let full_migrator = sqlx::migrate!("./migrations");
        let v7_migrations: Vec<_> = full_migrator
            .migrations
            .iter()
            .filter(|m| m.version <= 7)
            .cloned()
            .collect();
        let v7_migrator = sqlx::migrate::Migrator {
            migrations: std::borrow::Cow::Owned(v7_migrations),
            ignore_missing: full_migrator.ignore_missing,
            locking: full_migrator.locking,
            create_schemas: full_migrator.create_schemas.clone(),
            no_tx: full_migrator.no_tx,
            table_name: full_migrator.table_name.clone(),
        };
        v7_migrator.run(&pool).await.unwrap();

        sqlx::query("INSERT INTO server_meta (key, value) VALUES ('everyone_role_id', '1')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO roles (id, name, color, position, permissions, hoist, mentionable)
             VALUES (1, '@everyone', 0, 0, 3, FALSE, FALSE)",
        )
        .execute(&pool)
        .await
        .unwrap();

        full_migrator.run(&pool).await.unwrap();

        let (permissions, max_upload_bytes): (i64, Option<i64>) =
            sqlx::query_as("SELECT permissions, max_upload_bytes FROM roles WHERE id = 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(permissions, 3 | 1024 | 16384);
        assert_eq!(max_upload_bytes, Some(52428800));

        sqlx::query(
            "INSERT INTO users (id, public_key, display_name, created_at)
             VALUES (100, X'00', 'User', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO channels (id, kind, name, position, bitrate, user_limit, text_in_voice, created_at)
             VALUES (200, 'text', 'general', 0, 64000, 0, TRUE, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO attachments (id, uploader_id, channel_id, position, state, kind, filename, size_bytes, storage_id, key_enc, has_thumbnail, created_at, updated_at)
             VALUES (10, 100, 200, 0, 'ready', 'image', 'photo.png', 1000, 1, X'01', TRUE, 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query("DELETE FROM attachments WHERE id = 10")
            .execute(&pool)
            .await
            .unwrap();

        let deletions: Vec<(i64, String)> =
            sqlx::query_as("SELECT storage_id, object_key FROM blob_deletions ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            deletions,
            vec![(1, "a/10".to_string()), (1, "t/10".to_string())]
        );

        sqlx::query(
            "INSERT INTO attachments (id, uploader_id, channel_id, position, state, kind, filename, size_bytes, storage_id, key_enc, has_thumbnail, created_at, updated_at)
             VALUES (11, 100, 200, 0, 'ready', 'file', 'doc.txt', 500, 1, X'01', FALSE, 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query("DELETE FROM channels WHERE id = 200")
            .execute(&pool)
            .await
            .unwrap();

        let deletions: Vec<(i64, String)> =
            sqlx::query_as("SELECT storage_id, object_key FROM blob_deletions ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            deletions,
            vec![
                (1, "a/10".to_string()),
                (1, "t/10".to_string()),
                (1, "a/11".to_string())
            ]
        );
    }

    #[tokio::test]
    async fn a_song_outlives_the_channel_it_was_uploaded_in() {
        let dir = tempfile::tempdir().unwrap();
        let pool = connect(&dir.path().join("test.db")).await.unwrap();
        sqlx::query(
            "INSERT INTO users (id, public_key, display_name, created_at)
             VALUES (100, X'00', 'User', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO channels (id, kind, name, position, bitrate, user_limit, text_in_voice, created_at)
             VALUES (200, 'voice', 'General', 0, 64000, 0, TRUE, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        for (id, state, kind) in [(10, "music", "music"), (11, "uploading", "file")] {
            sqlx::query(
                "INSERT INTO attachments (id, uploader_id, channel_id, position, state, kind, filename, size_bytes, storage_id, key_enc, created_at, updated_at)
                 VALUES ($1, 100, 200, 0, $2, $3, 'a', 10, 1, X'01', 0, 0)",
            )
            .bind(id)
            .bind(state)
            .bind(kind)
            .execute(&pool)
            .await
            .unwrap();
        }

        sqlx::query("DELETE FROM channels WHERE id = 200")
            .execute(&pool)
            .await
            .unwrap();

        let left: Vec<(i64,)> = sqlx::query_as("SELECT id FROM attachments")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(left, vec![(10,)]);
    }
}
