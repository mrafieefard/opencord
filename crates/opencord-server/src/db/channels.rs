use sqlx::SqliteConnection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelRow {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub topic: Option<String>,
    pub parent_id: Option<i64>,
    pub position: i64,
    /// Voice channels only; stored for every kind.
    pub bitrate: i64,
    pub user_limit: i64,
    pub text_in_voice: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverwriteRow {
    pub channel_id: i64,
    pub target_kind: String,
    pub target_id: i64,
    pub allow: i64,
    pub deny: i64,
}

pub async fn list(conn: &mut SqliteConnection) -> Result<Vec<ChannelRow>, sqlx::Error> {
    sqlx::query_as!(
        ChannelRow,
        "SELECT id, kind, name, topic, parent_id, position, bitrate, user_limit, text_in_voice
         FROM channels"
    )
    .fetch_all(conn)
    .await
}

pub async fn list_overwrites(
    conn: &mut SqliteConnection,
) -> Result<Vec<OverwriteRow>, sqlx::Error> {
    sqlx::query_as!(
        OverwriteRow,
        "SELECT channel_id, target_kind, target_id, allow, deny FROM channel_overwrites"
    )
    .fetch_all(conn)
    .await
}

pub async fn insert(
    conn: &mut SqliteConnection,
    channel: &ChannelRow,
    created_at: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO channels (id, kind, name, topic, parent_id, position, created_at,
                               bitrate, user_limit, text_in_voice)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        channel.id,
        channel.kind,
        channel.name,
        channel.topic,
        channel.parent_id,
        channel.position,
        created_at,
        channel.bitrate,
        channel.user_limit,
        channel.text_in_voice
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn update(conn: &mut SqliteConnection, channel: &ChannelRow) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE channels SET name = $1, topic = $2, parent_id = $3, position = $4,
                             bitrate = $5, user_limit = $6, text_in_voice = $7
         WHERE id = $8",
        channel.name,
        channel.topic,
        channel.parent_id,
        channel.position,
        channel.bitrate,
        channel.user_limit,
        channel.text_in_voice,
        channel.id
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn delete(conn: &mut SqliteConnection, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM channels WHERE id = $1", id)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn upsert_overwrite(
    conn: &mut SqliteConnection,
    overwrite: &OverwriteRow,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO channel_overwrites (channel_id, target_kind, target_id, allow, deny)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (channel_id, target_kind, target_id)
         DO UPDATE SET allow = excluded.allow, deny = excluded.deny",
        overwrite.channel_id,
        overwrite.target_kind,
        overwrite.target_id,
        overwrite.allow,
        overwrite.deny
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn delete_overwrite(
    conn: &mut SqliteConnection,
    channel_id: i64,
    target_kind: &str,
    target_id: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM channel_overwrites
         WHERE channel_id = $1 AND target_kind = $2 AND target_id = $3",
        channel_id,
        target_kind,
        target_id
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Removes every overwrite that targets a role or member.
pub async fn delete_overwrites_for_target(
    conn: &mut SqliteConnection,
    target_kind: &str,
    target_id: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM channel_overwrites WHERE target_kind = $1 AND target_id = $2",
        target_kind,
        target_id
    )
    .execute(conn)
    .await?;
    Ok(())
}
