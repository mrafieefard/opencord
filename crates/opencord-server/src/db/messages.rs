use sqlx::SqliteConnection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageRow {
    pub id: i64,
    pub channel_id: i64,
    pub author_id: i64,
    pub content: String,
    pub edited_at: Option<i64>,
    pub deleted: bool,
}

pub async fn insert(
    conn: &mut SqliteConnection,
    id: i64,
    channel_id: i64,
    author_id: i64,
    content: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO messages (id, channel_id, author_id, content) VALUES ($1, $2, $3, $4)",
        id,
        channel_id,
        author_id,
        content
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// A message that has not been deleted.
pub async fn find(conn: &mut SqliteConnection, id: i64) -> Result<Option<MessageRow>, sqlx::Error> {
    sqlx::query_as!(
        MessageRow,
        r#"SELECT id, channel_id, author_id, content, edited_at, deleted AS "deleted: bool"
           FROM messages WHERE id = $1 AND deleted = FALSE"#,
        id
    )
    .fetch_optional(conn)
    .await
}

pub async fn update_content(
    conn: &mut SqliteConnection,
    id: i64,
    content: &str,
    edited_at: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE messages SET content = $1, edited_at = $2 WHERE id = $3",
        content,
        edited_at,
        id
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn mark_deleted(conn: &mut SqliteConnection, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query!("UPDATE messages SET deleted = TRUE WHERE id = $1", id)
        .execute(conn)
        .await?;
    Ok(())
}

/// Newest first, only ids below `before`.
pub async fn list(
    conn: &mut SqliteConnection,
    channel_id: i64,
    before: i64,
    limit: i64,
) -> Result<Vec<MessageRow>, sqlx::Error> {
    sqlx::query_as!(
        MessageRow,
        r#"SELECT id, channel_id, author_id, content, edited_at, deleted AS "deleted: bool"
           FROM messages
           WHERE channel_id = $1 AND id < $2 AND deleted = FALSE
           ORDER BY id DESC
           LIMIT $3"#,
        channel_id,
        before,
        limit
    )
    .fetch_all(conn)
    .await
}
