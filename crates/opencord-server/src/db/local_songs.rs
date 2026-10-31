//! Uploaded songs and who owns them (design 2026-10-24).

use sqlx::SqliteConnection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SongRow {
    pub id: i64,
    pub sha256: Vec<u8>,
    pub size_bytes: i64,
    pub filename: String,
    pub title: String,
    pub artist: String,
    pub duration_ms: i64,
    pub uploader_id: i64,
    pub created_at: i64,
}

pub async fn insert(conn: &mut SqliteConnection, row: &SongRow) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO music_local_songs
             (id, sha256, size_bytes, filename, title, artist, duration_ms, uploader_id, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        row.id,
        row.sha256,
        row.size_bytes,
        row.filename,
        row.title,
        row.artist,
        row.duration_ms,
        row.uploader_id,
        row.created_at
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn find(conn: &mut SqliteConnection, id: i64) -> Result<Option<SongRow>, sqlx::Error> {
    sqlx::query_as!(
        SongRow,
        "SELECT id, sha256, size_bytes, filename, title, artist, duration_ms, uploader_id,
                created_at
         FROM music_local_songs WHERE id = $1",
        id
    )
    .fetch_optional(conn)
    .await
}

pub async fn find_by_hash(
    conn: &mut SqliteConnection,
    sha256: &[u8],
) -> Result<Option<SongRow>, sqlx::Error> {
    sqlx::query_as!(
        SongRow,
        "SELECT id, sha256, size_bytes, filename, title, artist, duration_ms, uploader_id,
                created_at
         FROM music_local_songs WHERE sha256 = $1",
        sha256
    )
    .fetch_optional(conn)
    .await
}

/// Songs whose title, artist or filename contains `pattern` (a lowercase
/// `LIKE` pattern, `\` escaping), owned by `owner` or, with `None`, all;
/// newest first.
pub async fn matching(
    conn: &mut SqliteConnection,
    pattern: &str,
    owner: Option<i64>,
    limit: i64,
) -> Result<Vec<SongRow>, sqlx::Error> {
    match owner {
        Some(user_id) => {
            sqlx::query_as!(
                SongRow,
                r"SELECT s.id, s.sha256, s.size_bytes, s.filename, s.title, s.artist,
                         s.duration_ms, s.uploader_id, s.created_at
                  FROM music_local_songs s
                  JOIN music_local_song_owners o ON o.song_id = s.id
                  WHERE o.user_id = $1
                    AND LOWER(s.title || ' ' || s.artist || ' ' || s.filename) LIKE $2 ESCAPE '\'
                  ORDER BY s.id DESC
                  LIMIT $3",
                user_id,
                pattern,
                limit
            )
            .fetch_all(conn)
            .await
        }
        None => {
            sqlx::query_as!(
                SongRow,
                r"SELECT id, sha256, size_bytes, filename, title, artist, duration_ms,
                         uploader_id, created_at
                  FROM music_local_songs
                  WHERE LOWER(title || ' ' || artist || ' ' || filename) LIKE $1 ESCAPE '\'
                  ORDER BY id DESC
                  LIMIT $2",
                pattern,
                limit
            )
            .fetch_all(conn)
            .await
        }
    }
}

/// A page of every song, newest first, older than `before_id`.
pub async fn page(
    conn: &mut SqliteConnection,
    pattern: &str,
    before_id: i64,
    limit: i64,
) -> Result<Vec<SongRow>, sqlx::Error> {
    sqlx::query_as!(
        SongRow,
        r"SELECT id, sha256, size_bytes, filename, title, artist, duration_ms, uploader_id,
                 created_at
          FROM music_local_songs
          WHERE id < $1
            AND LOWER(title || ' ' || artist || ' ' || filename) LIKE $2 ESCAPE '\'
          ORDER BY id DESC
          LIMIT $3",
        before_id,
        pattern,
        limit
    )
    .fetch_all(conn)
    .await
}

/// Makes `user_id` an owner; already one is fine.
pub async fn add_owner(
    conn: &mut SqliteConnection,
    song_id: i64,
    user_id: i64,
    now: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO music_local_song_owners (song_id, user_id, added_at) VALUES ($1, $2, $3)
         ON CONFLICT (song_id, user_id) DO NOTHING",
        song_id,
        user_id,
        now
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn is_owner(
    conn: &mut SqliteConnection,
    song_id: i64,
    user_id: i64,
) -> Result<bool, sqlx::Error> {
    let found = sqlx::query_scalar!(
        "SELECT 1 AS one FROM music_local_song_owners WHERE song_id = $1 AND user_id = $2",
        song_id,
        user_id
    )
    .fetch_optional(conn)
    .await?;
    Ok(found.is_some())
}

/// Of `song_ids`, those `user_id` owns.
pub async fn owned_of(
    conn: &mut SqliteConnection,
    user_id: i64,
    song_ids: &[i64],
) -> Result<Vec<i64>, sqlx::Error> {
    let mut owned = Vec::new();
    for song_id in song_ids {
        if is_owner(conn, *song_id, user_id).await? {
            owned.push(*song_id);
        }
    }
    Ok(owned)
}

/// Drops `user_id` from the owners; returns how many owners are left.
pub async fn remove_owner(
    conn: &mut SqliteConnection,
    song_id: i64,
    user_id: i64,
) -> Result<i64, sqlx::Error> {
    sqlx::query!(
        "DELETE FROM music_local_song_owners WHERE song_id = $1 AND user_id = $2",
        song_id,
        user_id
    )
    .execute(&mut *conn)
    .await?;
    let left: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM music_local_song_owners WHERE song_id = $1",
        song_id
    )
    .fetch_one(conn)
    .await?;
    Ok(left)
}
