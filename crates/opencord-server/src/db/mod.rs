//! SQLite storage. Queries are checked at compile time against the
//! migrations (see `.sqlx/` for the offline data) and avoid SQLite-only
//! syntax so another backend can be added later.

use std::path::Path;
use std::time::Duration;

use opencord_common::permissions::Permissions;
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};

pub mod bans;
pub mod channels;
pub mod invites;
pub mod members;
pub mod meta;
pub mod roles;
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

/// Permission sets are stored as their bits reinterpreted as `i64`.
pub fn permissions_to_db(permissions: Permissions) -> i64 {
    i64::from_ne_bytes(permissions.bits().to_ne_bytes())
}

pub fn permissions_from_db(value: i64) -> Permissions {
    Permissions::from_bits_truncate(u64::from_ne_bytes(value.to_ne_bytes()))
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

    #[test]
    fn administrator_bit_survives_the_round_trip() {
        let permissions = Permissions::ADMINISTRATOR | Permissions::VIEW_CHANNEL;

        assert_eq!(
            permissions_from_db(permissions_to_db(permissions)),
            permissions
        );
    }
}
