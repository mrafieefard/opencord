//! Soundboard sounds, and the sounds from other servers played here.

use sqlx::SqliteConnection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundRow {
    pub id: i64,
    pub name: String,
    pub emoji: Option<String>,
    pub volume: i64,
    pub sha256: Vec<u8>,
    pub size_bytes: i64,
    pub duration_ms: i64,
    pub channel_count: i64,
    pub uploader_id: Option<i64>,
    pub created_at: i64,
}

pub async fn list(conn: &mut SqliteConnection) -> Result<Vec<SoundRow>, sqlx::Error> {
    sqlx::query_as!(
        SoundRow,
        "SELECT id, name, emoji, volume, sha256, size_bytes, duration_ms, channel_count,
                uploader_id, created_at
         FROM soundboard_sounds ORDER BY id"
    )
    .fetch_all(conn)
    .await
}

pub async fn insert(conn: &mut SqliteConnection, sound: &SoundRow) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO soundboard_sounds (id, name, emoji, volume, sha256, size_bytes,
                                        duration_ms, channel_count, uploader_id, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        sound.id,
        sound.name,
        sound.emoji,
        sound.volume,
        sound.sha256,
        sound.size_bytes,
        sound.duration_ms,
        sound.channel_count,
        sound.uploader_id,
        sound.created_at
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn update(
    conn: &mut SqliteConnection,
    id: i64,
    name: &str,
    emoji: Option<&str>,
    volume: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE soundboard_sounds SET name = $1, emoji = $2, volume = $3 WHERE id = $4",
        name,
        emoji,
        volume,
        id
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn delete(conn: &mut SqliteConnection, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM soundboard_sounds WHERE id = $1", id)
        .execute(conn)
        .await?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalRow {
    pub sha256: Vec<u8>,
    pub size_bytes: i64,
    pub duration_ms: i64,
    pub uploaded_by: Option<i64>,
    pub last_used_at: i64,
}

pub async fn external(
    conn: &mut SqliteConnection,
    sha256: &[u8],
) -> Result<Option<ExternalRow>, sqlx::Error> {
    sqlx::query_as!(
        ExternalRow,
        "SELECT sha256, size_bytes, duration_ms, uploaded_by, last_used_at
         FROM external_sounds WHERE sha256 = $1",
        sha256
    )
    .fetch_optional(conn)
    .await
}

/// Least recently used first.
pub async fn list_external(conn: &mut SqliteConnection) -> Result<Vec<ExternalRow>, sqlx::Error> {
    sqlx::query_as!(
        ExternalRow,
        "SELECT sha256, size_bytes, duration_ms, uploaded_by, last_used_at
         FROM external_sounds ORDER BY last_used_at, sha256"
    )
    .fetch_all(conn)
    .await
}

/// Stores the sound, or marks it used if it is already here.
pub async fn put_external(
    conn: &mut SqliteConnection,
    row: &ExternalRow,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO external_sounds (sha256, size_bytes, duration_ms, uploaded_by, last_used_at)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (sha256) DO UPDATE SET last_used_at = excluded.last_used_at",
        row.sha256,
        row.size_bytes,
        row.duration_ms,
        row.uploaded_by,
        row.last_used_at
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn touch_external(
    conn: &mut SqliteConnection,
    sha256: &[u8],
    now: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE external_sounds SET last_used_at = $1 WHERE sha256 = $2",
        now,
        sha256
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn delete_external(
    conn: &mut SqliteConnection,
    sha256: &[u8],
) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM external_sounds WHERE sha256 = $1", sha256)
        .execute(conn)
        .await?;
    Ok(())
}
