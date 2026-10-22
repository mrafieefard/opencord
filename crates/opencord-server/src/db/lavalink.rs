//! Lavalink nodes (Phase 3 plan §12.1). Passwords are sealed; see
//! [`crate::secrets`].

use sqlx::SqliteConnection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeRow {
    pub id: i64,
    pub name: String,
    pub host: String,
    pub port: i64,
    pub secure: bool,
    pub password_enc: Vec<u8>,
    pub priority: String,
    pub max_players: i64,
    pub strict_address: bool,
    pub enabled: bool,
    pub draining: bool,
    pub resume_session: Option<String>,
    pub created_at: i64,
}

pub async fn list(conn: &mut SqliteConnection) -> Result<Vec<NodeRow>, sqlx::Error> {
    sqlx::query_as!(
        NodeRow,
        "SELECT id, name, host, port, secure, password_enc, priority, max_players,
                strict_address, enabled, draining, resume_session, created_at
         FROM lavalink_nodes ORDER BY id"
    )
    .fetch_all(conn)
    .await
}

/// Saves a node, inserting it the first time.
pub async fn put(conn: &mut SqliteConnection, row: &NodeRow) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO lavalink_nodes (id, name, host, port, secure, password_enc, priority,
                                     max_players, strict_address, enabled, draining,
                                     resume_session, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
         ON CONFLICT (id) DO UPDATE SET
             name = excluded.name, host = excluded.host, port = excluded.port,
             secure = excluded.secure, password_enc = excluded.password_enc,
             priority = excluded.priority, max_players = excluded.max_players,
             strict_address = excluded.strict_address, enabled = excluded.enabled,
             draining = excluded.draining, resume_session = excluded.resume_session",
        row.id,
        row.name,
        row.host,
        row.port,
        row.secure,
        row.password_enc,
        row.priority,
        row.max_players,
        row.strict_address,
        row.enabled,
        row.draining,
        row.resume_session,
        row.created_at
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// The session to resume after a restart, or none.
pub async fn set_resume_session(
    conn: &mut SqliteConnection,
    id: i64,
    session: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE lavalink_nodes SET resume_session = $1 WHERE id = $2",
        session,
        id
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn delete(conn: &mut SqliteConnection, id: i64) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!("DELETE FROM lavalink_nodes WHERE id = $1", id)
        .execute(conn)
        .await?;
    Ok(done.rows_affected() > 0)
}
