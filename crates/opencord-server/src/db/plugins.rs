//! Plugins' rows (design 2026-11-07).

use sqlx::SqliteConnection;

use super::users::UserRow;

/// A plugin user, with its managed role while the plugin is installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginUserRow {
    pub user_id: i64,
    pub plugin_id: String,
    pub display_name: String,
    pub avatar_hash: Option<String>,
    /// `None` once uninstalled.
    pub role_id: Option<i64>,
}

/// Every plugin user, installed or not. Its plugin is installed while a
/// role carries its id: `identity::ensure` writes the two together and
/// `identity::retire` removes the role with the plugin's row.
pub async fn plugin_users(conn: &mut SqliteConnection) -> Result<Vec<PluginUserRow>, sqlx::Error> {
    sqlx::query_as!(
        PluginUserRow,
        r#"SELECT u.id AS "user_id!", u.plugin_id AS "plugin_id!", u.display_name,
                  u.avatar_hash, r.id AS "role_id?"
           FROM users u LEFT JOIN roles r ON r.plugin_id = u.plugin_id
           WHERE u.plugin_id IS NOT NULL"#
    )
    .fetch_all(conn)
    .await
}

/// A user for the plugin with this id, which nobody can sign in as.
pub async fn insert_plugin_user(
    conn: &mut SqliteConnection,
    user: &UserRow,
    plugin_id: &str,
    created_at: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO users (id, public_key, display_name, avatar_hash, created_at, plugin_id)
         VALUES ($1, $2, $3, $4, $5, $6)",
        user.id,
        user.public_key,
        user.display_name,
        user.avatar_hash,
        created_at,
        plugin_id
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// A plugin user's name and picture; never touches other users.
pub async fn set_plugin_user_name(
    conn: &mut SqliteConnection,
    user_id: i64,
    name: &str,
    avatar_hash: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE users SET display_name = $1, avatar_hash = $2
         WHERE id = $3 AND plugin_id IS NOT NULL",
        name,
        avatar_hash,
        user_id
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Removes the plugin's row, with its permissions and config.
pub async fn delete_plugin(
    conn: &mut SqliteConnection,
    plugin_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM plugins WHERE id = $1", plugin_id)
        .execute(conn)
        .await?;
    Ok(())
}

/// An installed plugin's row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginRow {
    pub id: String,
    pub user_id: i64,
    pub role_id: i64,
    pub version: String,
    pub folder: String,
    pub manifest: String,
    pub signature: String,
    pub public_key: Option<Vec<u8>>,
    pub github_check: String,
    pub running: bool,
    pub crashed_reason: Option<String>,
    pub installed_at: i64,
    pub updated_at: i64,
}

pub async fn plugin(
    conn: &mut SqliteConnection,
    plugin_id: &str,
) -> Result<Option<PluginRow>, sqlx::Error> {
    sqlx::query_as!(
        PluginRow,
        r#"SELECT id AS "id!", user_id, role_id, version, folder, manifest, signature,
                  public_key, github_check, running AS "running: bool", crashed_reason,
                  installed_at, updated_at
           FROM plugins WHERE id = $1"#,
        plugin_id
    )
    .fetch_optional(conn)
    .await
}

pub async fn plugins(conn: &mut SqliteConnection) -> Result<Vec<PluginRow>, sqlx::Error> {
    sqlx::query_as!(
        PluginRow,
        r#"SELECT id AS "id!", user_id, role_id, version, folder, manifest, signature,
                  public_key, github_check, running AS "running: bool", crashed_reason,
                  installed_at, updated_at
           FROM plugins ORDER BY id"#
    )
    .fetch_all(conn)
    .await
}

/// The names of the permissions granted to the plugin.
pub async fn granted(
    conn: &mut SqliteConnection,
    plugin_id: &str,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT name FROM plugin_permissions WHERE plugin_id = $1 ORDER BY name",
        plugin_id
    )
    .fetch_all(conn)
    .await
}

/// A config value: UTF-8 text, or sealed bytes for a secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigValueRow {
    pub key: String,
    pub value: Vec<u8>,
    pub sealed: bool,
}

pub async fn config_values(
    conn: &mut SqliteConnection,
    plugin_id: &str,
) -> Result<Vec<ConfigValueRow>, sqlx::Error> {
    sqlx::query_as!(
        ConfigValueRow,
        r#"SELECT key, value, sealed AS "sealed: bool"
           FROM plugin_config WHERE plugin_id = $1 ORDER BY key"#,
        plugin_id
    )
    .fetch_all(conn)
    .await
}

/// What the plugin's files take, in bytes.
pub async fn files_bytes(conn: &mut SqliteConnection, plugin_id: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT COALESCE(SUM(size), 0) AS "bytes!: i64" FROM plugin_files WHERE plugin_id = $1"#,
        plugin_id
    )
    .fetch_one(conn)
    .await
}

pub async fn count_plugins(conn: &mut SqliteConnection) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT COUNT(*) AS "count!: i64" FROM plugins"#)
        .fetch_one(conn)
        .await
}

/// Installs the plugin's row; `crashed_reason` starts clear.
pub async fn insert_plugin(
    conn: &mut SqliteConnection,
    row: &PluginRow,
    license_accepted_by: Option<i64>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO plugins (id, user_id, role_id, version, folder, manifest, signature,
                              public_key, github_check, running, license_accepted_by,
                              installed_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        row.id,
        row.user_id,
        row.role_id,
        row.version,
        row.folder,
        row.manifest,
        row.signature,
        row.public_key,
        row.github_check,
        row.running,
        license_accepted_by,
        row.installed_at,
        row.updated_at
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Replaces what an update (or putting the old version back) changes.
pub async fn update_plugin(
    conn: &mut SqliteConnection,
    row: &PluginRow,
    license_accepted_by: Option<i64>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE plugins SET version = $1, folder = $2, manifest = $3, signature = $4,
                            public_key = $5, github_check = $6, running = $7,
                            crashed_reason = $8, license_accepted_by = $9, updated_at = $10
         WHERE id = $11",
        row.version,
        row.folder,
        row.manifest,
        row.signature,
        row.public_key,
        row.github_check,
        row.running,
        row.crashed_reason,
        license_accepted_by,
        row.updated_at,
        row.id
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn license_accepted_by(
    conn: &mut SqliteConnection,
    plugin_id: &str,
) -> Result<Option<i64>, sqlx::Error> {
    let accepted = sqlx::query_scalar!(
        "SELECT license_accepted_by FROM plugins WHERE id = $1",
        plugin_id
    )
    .fetch_optional(conn)
    .await?;
    Ok(accepted.flatten())
}

/// The admin's switch.
pub async fn set_running(
    conn: &mut SqliteConnection,
    plugin_id: &str,
    running: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE plugins SET running = $1 WHERE id = $2",
        running,
        plugin_id
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// A granted permission and who granted it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionRow {
    pub name: String,
    pub granted_by: i64,
    pub granted_at: i64,
}

pub async fn permission_rows(
    conn: &mut SqliteConnection,
    plugin_id: &str,
) -> Result<Vec<PermissionRow>, sqlx::Error> {
    sqlx::query_as!(
        PermissionRow,
        "SELECT name, granted_by, granted_at FROM plugin_permissions
         WHERE plugin_id = $1 ORDER BY name",
        plugin_id
    )
    .fetch_all(conn)
    .await
}

/// The plugin's granted permissions become exactly `rows`.
pub async fn replace_permissions(
    conn: &mut SqliteConnection,
    plugin_id: &str,
    rows: &[PermissionRow],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM plugin_permissions WHERE plugin_id = $1",
        plugin_id
    )
    .execute(&mut *conn)
    .await?;
    for row in rows {
        sqlx::query!(
            "INSERT INTO plugin_permissions (plugin_id, name, granted_by, granted_at)
             VALUES ($1, $2, $3, $4)",
            plugin_id,
            row.name,
            row.granted_by,
            row.granted_at
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// The plugin's settings become exactly `rows`.
pub async fn replace_config(
    conn: &mut SqliteConnection,
    plugin_id: &str,
    rows: &[ConfigValueRow],
) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM plugin_config WHERE plugin_id = $1", plugin_id)
        .execute(&mut *conn)
        .await?;
    for row in rows {
        sqlx::query!(
            "INSERT INTO plugin_config (plugin_id, key, value, sealed) VALUES ($1, $2, $3, $4)",
            plugin_id,
            row.key,
            row.value,
            row.sealed
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// What an uninstall that kept the data left: `Some(pin)` when there is
/// one, the pin being the key a reinstall must carry.
pub async fn leftover(
    conn: &mut SqliteConnection,
    plugin_id: &str,
) -> Result<Option<Option<Vec<u8>>>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT public_key FROM plugin_leftovers WHERE id = $1",
        plugin_id
    )
    .fetch_optional(conn)
    .await
}

pub async fn insert_leftover(
    conn: &mut SqliteConnection,
    plugin_id: &str,
    public_key: Option<&[u8]>,
    removed_at: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT OR REPLACE INTO plugin_leftovers (id, public_key, removed_at) VALUES ($1, $2, $3)",
        plugin_id,
        public_key,
        removed_at
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn delete_leftover(
    conn: &mut SqliteConnection,
    plugin_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM plugin_leftovers WHERE id = $1", plugin_id)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn delete_plugin_files(
    conn: &mut SqliteConnection,
    plugin_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM plugin_files WHERE plugin_id = $1", plugin_id)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn file_size(
    conn: &mut SqliteConnection,
    plugin_id: &str,
    path: &str,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT size AS "size!: i64" FROM plugin_files WHERE plugin_id = $1 AND path = $2"#,
        plugin_id,
        path
    )
    .fetch_optional(conn)
    .await
}

/// Whether the plugin has a file at `path`: `Some(public)`, or `None`.
pub async fn file_is_public(
    conn: &mut SqliteConnection,
    plugin_id: &str,
    path: &str,
) -> Result<Option<bool>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT public AS "public!: bool" FROM plugin_files WHERE plugin_id = $1 AND path = $2"#,
        plugin_id,
        path
    )
    .fetch_optional(conn)
    .await
}

pub async fn upsert_plugin_file(
    conn: &mut SqliteConnection,
    plugin_id: &str,
    path: &str,
    size: i64,
    public: bool,
    updated_at: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO plugin_files (plugin_id, path, size, public, updated_at)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (plugin_id, path) DO UPDATE SET
             size = excluded.size,
             public = excluded.public,
             updated_at = excluded.updated_at",
        plugin_id,
        path,
        size,
        public,
        updated_at
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn delete_plugin_file(
    conn: &mut SqliteConnection,
    plugin_id: &str,
    path: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM plugin_files WHERE plugin_id = $1 AND path = $2",
        plugin_id,
        path
    )
    .execute(conn)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PluginFileRow {
    pub path: String,
    pub size: i64,
    pub public: bool,
    pub updated_at: i64,
}

pub async fn list_plugin_files(
    conn: &mut SqliteConnection,
    plugin_id: &str,
    prefix: Option<&str>,
) -> Result<Vec<PluginFileRow>, sqlx::Error> {
    let rows = sqlx::query_as!(
        PluginFileRow,
        r#"SELECT path, size, public AS "public: bool", updated_at
           FROM plugin_files
           WHERE plugin_id = $1
           ORDER BY path"#,
        plugin_id
    )
    .fetch_all(conn)
    .await?;
    if let Some(prefix) = prefix {
        Ok(rows
            .into_iter()
            .filter(|r| r.path.starts_with(prefix))
            .collect())
    } else {
        Ok(rows)
    }
}

/// A dev token, without its secret's hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevTokenRow {
    pub id: i64,
    pub plugin_id: String,
    pub created_by: i64,
    pub created_at: i64,
    pub expires_at: i64,
    pub last_used_at: Option<i64>,
}

/// What checking a dev token needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevTokenHashRow {
    pub id: i64,
    pub token_hash: Vec<u8>,
    pub created_by: i64,
    pub expires_at: i64,
}

pub async fn insert_dev_token(
    conn: &mut SqliteConnection,
    row: &DevTokenRow,
    token_hash: &[u8],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO plugin_dev_tokens (id, plugin_id, token_hash, created_by, created_at,
                                        expires_at, last_used_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
        row.id,
        row.plugin_id,
        token_hash,
        row.created_by,
        row.created_at,
        row.expires_at,
        row.last_used_at
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Every dev token, expired ones too, oldest first.
pub async fn dev_tokens(conn: &mut SqliteConnection) -> Result<Vec<DevTokenRow>, sqlx::Error> {
    sqlx::query_as!(
        DevTokenRow,
        "SELECT id, plugin_id, created_by, created_at, expires_at, last_used_at
         FROM plugin_dev_tokens ORDER BY created_at, id"
    )
    .fetch_all(conn)
    .await
}

/// The dev tokens naming the plugin, with their hashes.
pub async fn dev_token_hashes(
    conn: &mut SqliteConnection,
    plugin_id: &str,
) -> Result<Vec<DevTokenHashRow>, sqlx::Error> {
    sqlx::query_as!(
        DevTokenHashRow,
        "SELECT id, token_hash, created_by, expires_at
         FROM plugin_dev_tokens WHERE plugin_id = $1",
        plugin_id
    )
    .fetch_all(conn)
    .await
}

/// Whether there was such a token.
pub async fn delete_dev_token(conn: &mut SqliteConnection, id: i64) -> Result<bool, sqlx::Error> {
    let deleted = sqlx::query!("DELETE FROM plugin_dev_tokens WHERE id = $1", id)
        .execute(conn)
        .await?;
    Ok(deleted.rows_affected() == 1)
}

pub async fn set_dev_token_used(
    conn: &mut SqliteConnection,
    id: i64,
    at: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE plugin_dev_tokens SET last_used_at = $1 WHERE id = $2",
        at,
        id
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Why the plugin was turned off after failing; `None` clears it.
pub async fn set_crashed_reason(
    conn: &mut SqliteConnection,
    plugin_id: &str,
    reason: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE plugins SET crashed_reason = $1 WHERE id = $2",
        reason,
        plugin_id
    )
    .execute(conn)
    .await?;
    Ok(())
}
