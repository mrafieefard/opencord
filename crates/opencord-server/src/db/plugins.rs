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
