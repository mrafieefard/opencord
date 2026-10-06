use sqlx::SqliteConnection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BanRow {
    pub user_id: i64,
    pub public_key: Vec<u8>,
    pub display_name: String,
    pub reason: Option<String>,
    pub banned_by: Option<i64>,
    pub created_at: i64,
}

pub async fn is_banned(conn: &mut SqliteConnection, user_id: i64) -> Result<bool, sqlx::Error> {
    let found = sqlx::query_scalar!("SELECT user_id FROM bans WHERE user_id = $1", user_id)
        .fetch_optional(conn)
        .await?;
    Ok(found.is_some())
}

pub async fn insert(
    conn: &mut SqliteConnection,
    user_id: i64,
    reason: Option<&str>,
    banned_by: i64,
    created_at: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO bans (user_id, reason, banned_by, created_at) VALUES ($1, $2, $3, $4)
         ON CONFLICT (user_id) DO UPDATE SET reason = excluded.reason,
             banned_by = excluded.banned_by, created_at = excluded.created_at",
        user_id,
        reason,
        banned_by,
        created_at
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn delete(conn: &mut SqliteConnection, user_id: i64) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!("DELETE FROM bans WHERE user_id = $1", user_id)
        .execute(conn)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn list(conn: &mut SqliteConnection) -> Result<Vec<BanRow>, sqlx::Error> {
    sqlx::query_as!(
        BanRow,
        "SELECT b.user_id, u.public_key, u.display_name, b.reason, b.banned_by, b.created_at
         FROM bans b JOIN users u ON u.id = b.user_id ORDER BY b.created_at"
    )
    .fetch_all(conn)
    .await
}
