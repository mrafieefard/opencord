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
