//! Music sessions and members blocked from music (Phase 3 plan §14.1,
//! §16.1).

use sqlx::SqliteConnection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRow {
    pub channel_id: i64,
    pub state_json: String,
    pub updated_at: i64,
}

pub async fn sessions(conn: &mut SqliteConnection) -> Result<Vec<SessionRow>, sqlx::Error> {
    sqlx::query_as!(
        SessionRow,
        "SELECT channel_id, state_json, updated_at FROM music_sessions"
    )
    .fetch_all(conn)
    .await
}

pub async fn put_session(conn: &mut SqliteConnection, row: &SessionRow) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO music_sessions (channel_id, state_json, updated_at) VALUES ($1, $2, $3)
         ON CONFLICT (channel_id) DO UPDATE SET
             state_json = excluded.state_json, updated_at = excluded.updated_at",
        row.channel_id,
        row.state_json,
        row.updated_at
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn delete_session(
    conn: &mut SqliteConnection,
    channel_id: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM music_sessions WHERE channel_id = $1",
        channel_id
    )
    .execute(conn)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockRow {
    pub user_id: i64,
    pub reason: String,
    pub blocked_by: i64,
    pub created_at: i64,
}

pub async fn blocks(conn: &mut SqliteConnection) -> Result<Vec<BlockRow>, sqlx::Error> {
    sqlx::query_as!(
        BlockRow,
        "SELECT user_id, reason, blocked_by, created_at FROM music_blocks ORDER BY created_at"
    )
    .fetch_all(conn)
    .await
}

pub async fn put_block(conn: &mut SqliteConnection, row: &BlockRow) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO music_blocks (user_id, reason, blocked_by, created_at) VALUES ($1, $2, $3, $4)
         ON CONFLICT (user_id) DO UPDATE SET reason = excluded.reason,
             blocked_by = excluded.blocked_by, created_at = excluded.created_at",
        row.user_id,
        row.reason,
        row.blocked_by,
        row.created_at
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn delete_block(conn: &mut SqliteConnection, user_id: i64) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM music_blocks WHERE user_id = $1", user_id)
        .execute(conn)
        .await?;
    Ok(())
}
