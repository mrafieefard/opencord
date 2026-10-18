use sqlx::SqliteConnection;

const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 24 * HOUR_MS;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct AttachmentRow {
    pub id: i64,
    pub uploader_id: i64,
    pub channel_id: i64,
    pub message_id: Option<i64>,
    pub position: i64,
    pub state: String,
    pub kind: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub received_bytes: i64,
    pub storage_id: i64,
    pub key_enc: Vec<u8>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_ms: Option<i64>,
    pub animated: bool,
    pub waveform: Option<Vec<u8>>,
    pub has_thumbnail: bool,
    pub thumbnail_width: Option<i64>,
    pub thumbnail_height: Option<i64>,
    pub error_code: Option<i64>,
    pub error_message: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadyFields {
    pub kind: String,
    pub content_type: String,
    pub filename: String,
    pub storage_id: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_ms: Option<i64>,
    pub animated: bool,
    pub waveform: Option<Vec<u8>>,
    pub has_thumbnail: bool,
    pub thumbnail_width: Option<i64>,
    pub thumbnail_height: Option<i64>,
}

pub async fn insert_upload(
    conn: &mut SqliteConnection,
    row: &AttachmentRow,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO attachments (
            id, uploader_id, channel_id, message_id, position, state, kind,
            filename, content_type, size_bytes, received_bytes, storage_id,
            key_enc, width, height, duration_ms, animated, waveform,
            has_thumbnail, thumbnail_width, thumbnail_height, error_code,
            error_message, created_at, updated_at
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                   $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25)",
        row.id,
        row.uploader_id,
        row.channel_id,
        row.message_id,
        row.position,
        row.state,
        row.kind,
        row.filename,
        row.content_type,
        row.size_bytes,
        row.received_bytes,
        row.storage_id,
        row.key_enc,
        row.width,
        row.height,
        row.duration_ms,
        row.animated,
        row.waveform,
        row.has_thumbnail,
        row.thumbnail_width,
        row.thumbnail_height,
        row.error_code,
        row.error_message,
        row.created_at,
        row.updated_at
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn find(
    conn: &mut SqliteConnection,
    id: i64,
) -> Result<Option<AttachmentRow>, sqlx::Error> {
    sqlx::query_as!(
        AttachmentRow,
        r#"SELECT id, uploader_id, channel_id, message_id, position, state, kind,
                  filename, content_type, size_bytes, received_bytes, storage_id,
                  key_enc, width, height, duration_ms, animated AS "animated: bool",
                  waveform, has_thumbnail AS "has_thumbnail: bool", thumbnail_width,
                  thumbnail_height, error_code, error_message, created_at, updated_at
           FROM attachments WHERE id = $1"#,
        id
    )
    .fetch_optional(conn)
    .await
}

pub async fn set_received(
    conn: &mut SqliteConnection,
    id: i64,
    received: i64,
    now: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE attachments SET received_bytes = $2, updated_at = $3 WHERE id = $1",
        id,
        received,
        now
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn set_state(
    conn: &mut SqliteConnection,
    id: i64,
    state: &str,
    now: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE attachments SET state = $2, updated_at = $3 WHERE id = $1",
        id,
        state,
        now
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Marks a processing row ready. False when the row is gone or no longer
/// processing (cancelled while its job ran).
pub async fn set_ready(
    conn: &mut SqliteConnection,
    id: i64,
    ready: &ReadyFields,
    now: i64,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "UPDATE attachments
         SET state = 'ready',
             kind = $2,
             content_type = $3,
             filename = $4,
             storage_id = $5,
             width = $6,
             height = $7,
             duration_ms = $8,
             animated = $9,
             waveform = $10,
             has_thumbnail = $11,
             thumbnail_width = $12,
             thumbnail_height = $13,
             updated_at = $14
         WHERE id = $1 AND state = 'processing'",
        id,
        ready.kind,
        ready.content_type,
        ready.filename,
        ready.storage_id,
        ready.width,
        ready.height,
        ready.duration_ms,
        ready.animated,
        ready.waveform,
        ready.has_thumbnail,
        ready.thumbnail_width,
        ready.thumbnail_height,
        now
    )
    .execute(conn)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn set_failed(
    conn: &mut SqliteConnection,
    id: i64,
    code: i32,
    message: &str,
    now: i64,
) -> Result<(), sqlx::Error> {
    let error_code = i64::from(code);
    sqlx::query!(
        "UPDATE attachments
         SET state = 'failed',
             error_code = $2,
             error_message = $3,
             updated_at = $4
         WHERE id = $1",
        id,
        error_code,
        message,
        now
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn mark_sent(
    conn: &mut SqliteConnection,
    ids: &[i64],
    message_id: i64,
) -> Result<(), sqlx::Error> {
    for (pos, id) in ids.iter().enumerate() {
        let position = pos as i64;
        sqlx::query!(
            "UPDATE attachments
             SET message_id = $1, position = $2, state = 'sent'
             WHERE id = $3",
            message_id,
            position,
            id
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

pub async fn for_messages(
    conn: &mut SqliteConnection,
    message_ids: &[i64],
) -> Result<Vec<AttachmentRow>, sqlx::Error> {
    if message_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut rows = Vec::new();
    for chunk in message_ids.chunks(100) {
        let mut builder = sqlx::QueryBuilder::new(
            "SELECT id, uploader_id, channel_id, message_id, position, state, kind, \
                    filename, content_type, size_bytes, received_bytes, storage_id, \
                    key_enc, width, height, duration_ms, animated, waveform, \
                    has_thumbnail, thumbnail_width, thumbnail_height, error_code, \
                    error_message, created_at, updated_at \
             FROM attachments WHERE state = 'sent' AND message_id IN (",
        );
        let mut separated = builder.separated(", ");
        for id in chunk {
            separated.push_bind(id);
        }
        separated.push_unseparated(") ORDER BY message_id, position");
        // Dynamic query builder query for IN batching (unchecked sqlx query).
        let chunk_rows: Vec<AttachmentRow> = builder.build_query_as().fetch_all(&mut *conn).await?;
        rows.extend(chunk_rows);
    }
    rows.sort_by_key(|r| (r.message_id, r.position));
    Ok(rows)
}

pub async fn delete(conn: &mut SqliteConnection, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM attachments WHERE id = $1", id)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn delete_for_message(
    conn: &mut SqliteConnection,
    message_id: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM attachments WHERE message_id = $1", message_id)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn count_unfinished(
    conn: &mut SqliteConnection,
    uploader_id: i64,
) -> Result<i64, sqlx::Error> {
    let count: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM attachments
         WHERE uploader_id = $1 AND state IN ('uploading', 'processing')",
        uploader_id
    )
    .fetch_one(conn)
    .await?;
    Ok(count)
}

pub async fn expired(conn: &mut SqliteConnection, now: i64) -> Result<Vec<i64>, sqlx::Error> {
    let day_ago = now - DAY_MS;
    let hour_ago = now - HOUR_MS;
    let ids = sqlx::query_scalar!(
        "SELECT id FROM attachments
         WHERE (state = 'uploading' AND updated_at < $1)
            OR (state = 'ready' AND updated_at < $1)
            OR (state = 'failed' AND updated_at < $2)",
        day_ago,
        hour_ago
    )
    .fetch_all(conn)
    .await?;
    Ok(ids)
}

pub async fn in_state(
    conn: &mut SqliteConnection,
    state: &str,
) -> Result<Vec<AttachmentRow>, sqlx::Error> {
    sqlx::query_as!(
        AttachmentRow,
        r#"SELECT id, uploader_id, channel_id, message_id, position, state, kind,
                  filename, content_type, size_bytes, received_bytes, storage_id,
                  key_enc, width, height, duration_ms, animated AS "animated: bool",
                  waveform, has_thumbnail AS "has_thumbnail: bool", thumbnail_width,
                  thumbnail_height, error_code, error_message, created_at, updated_at
           FROM attachments WHERE state = $1"#,
        state
    )
    .fetch_all(conn)
    .await
}

pub async fn usage(conn: &mut SqliteConnection) -> Result<Vec<(i64, i64, i64)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT storage_id, COUNT(*) AS "count: i64", COALESCE(SUM(size_bytes), 0) AS "bytes: i64"
           FROM attachments
           WHERE state IN ('ready', 'sent')
           GROUP BY storage_id"#
    )
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.storage_id, r.count, r.bytes))
        .collect())
}

pub async fn not_on(
    conn: &mut SqliteConnection,
    storage_id: i64,
    after_id: i64,
    limit: i64,
) -> Result<Vec<AttachmentRow>, sqlx::Error> {
    sqlx::query_as!(
        AttachmentRow,
        r#"SELECT id, uploader_id, channel_id, message_id, position, state, kind,
                  filename, content_type, size_bytes, received_bytes, storage_id,
                  key_enc, width, height, duration_ms, animated AS "animated: bool",
                  waveform, has_thumbnail AS "has_thumbnail: bool", thumbnail_width,
                  thumbnail_height, error_code, error_message, created_at, updated_at
           FROM attachments
           WHERE storage_id != $1 AND id > $2 AND state IN ('ready', 'sent')
           ORDER BY id
           LIMIT $3"#,
        storage_id,
        after_id,
        limit
    )
    .fetch_all(conn)
    .await
}

pub async fn count_not_on(
    conn: &mut SqliteConnection,
    storage_id: i64,
) -> Result<(i64, i64), sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT COUNT(*) AS "count: i64", COALESCE(SUM(size_bytes), 0) AS "bytes: i64"
           FROM attachments
           WHERE storage_id != $1 AND state IN ('ready', 'sent')"#,
        storage_id
    )
    .fetch_one(conn)
    .await?;
    Ok((row.count, row.bytes))
}

pub async fn set_storage_if(
    conn: &mut SqliteConnection,
    id: i64,
    from: i64,
    to: i64,
) -> Result<bool, sqlx::Error> {
    let res = sqlx::query!(
        "UPDATE attachments SET storage_id = $1 WHERE id = $2 AND storage_id = $3",
        to,
        id,
        from
    )
    .execute(conn)
    .await?;
    Ok(res.rows_affected() == 1)
}

pub async fn count_on(conn: &mut SqliteConnection, storage_id: i64) -> Result<i64, sqlx::Error> {
    let count: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM attachments WHERE storage_id = $1",
        storage_id
    )
    .fetch_one(conn)
    .await?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_pool;

    #[tokio::test]
    async fn upload_rows_move_through_their_states() {
        let (pool, _dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        sqlx::query(
            "INSERT INTO users (id, public_key, display_name, created_at) VALUES (1, X'01', 'U1', 0)",
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO channels (id, kind, name, position, bitrate, user_limit, text_in_voice, created_at)
             VALUES (10, 'text', 'general', 0, 64000, 0, TRUE, 0)",
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO messages (id, channel_id, author_id, content) VALUES (100, 10, 1, 'hi')",
        )
        .execute(&mut *conn)
        .await
        .unwrap();

        let row1 = AttachmentRow {
            id: 501,
            uploader_id: 1,
            channel_id: 10,
            message_id: None,
            position: 0,
            state: "uploading".to_string(),
            kind: "file".to_string(),
            filename: "file1.txt".to_string(),
            content_type: "text/plain".to_string(),
            size_bytes: 1000,
            received_bytes: 0,
            storage_id: 1,
            key_enc: vec![1, 2, 3],
            width: None,
            height: None,
            duration_ms: None,
            animated: false,
            waveform: None,
            has_thumbnail: false,
            thumbnail_width: None,
            thumbnail_height: None,
            error_code: None,
            error_message: None,
            created_at: 10,
            updated_at: 10,
        };
        let row2 = AttachmentRow {
            id: 502,
            filename: "file2.txt".to_string(),
            ..row1.clone()
        };

        insert_upload(&mut conn, &row1).await.unwrap();
        insert_upload(&mut conn, &row2).await.unwrap();

        assert_eq!(count_unfinished(&mut conn, 1).await.unwrap(), 2);

        set_received(&mut conn, 501, 500, 20).await.unwrap();
        let found = find(&mut conn, 501).await.unwrap().unwrap();
        assert_eq!(found.received_bytes, 500);
        assert_eq!(found.updated_at, 20);

        set_state(&mut conn, 501, "processing", 30).await.unwrap();
        let processing = in_state(&mut conn, "processing").await.unwrap();
        assert_eq!(processing.len(), 1);
        assert_eq!(processing[0].id, 501);

        let ready = ReadyFields {
            kind: "image".to_string(),
            content_type: "image/png".to_string(),
            filename: "photo.png".to_string(),
            storage_id: 1,
            width: Some(800),
            height: Some(600),
            duration_ms: None,
            animated: false,
            waveform: None,
            has_thumbnail: true,
            thumbnail_width: Some(200),
            thumbnail_height: Some(150),
        };
        assert!(set_ready(&mut conn, 501, &ready, 40).await.unwrap());

        let ready_fields2 = ReadyFields {
            kind: "file".to_string(),
            content_type: "text/plain".to_string(),
            filename: "doc.txt".to_string(),
            storage_id: 1,
            width: None,
            height: None,
            duration_ms: None,
            animated: false,
            waveform: None,
            has_thumbnail: false,
            thumbnail_width: None,
            thumbnail_height: None,
        };
        // Only a processing row becomes ready.
        assert!(!set_ready(&mut conn, 502, &ready_fields2, 35).await.unwrap());
        set_state(&mut conn, 502, "processing", 35).await.unwrap();
        assert!(set_ready(&mut conn, 502, &ready_fields2, 40).await.unwrap());

        // Mark sent with specific order: 502 at position 0, 501 at position 1
        mark_sent(&mut conn, &[502, 501], 100).await.unwrap();

        let sent = for_messages(&mut conn, &[100]).await.unwrap();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0].id, 502);
        assert_eq!(sent[0].position, 0);
        assert_eq!(sent[1].id, 501);
        assert_eq!(sent[1].position, 1);
        assert_eq!(sent[0].state, "sent");
        assert_eq!(sent[1].state, "sent");
    }

    #[tokio::test]
    async fn expired_picks_each_state_by_its_age() {
        let (pool, _dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        sqlx::query(
            "INSERT INTO users (id, public_key, display_name, created_at) VALUES (1, X'01', 'U1', 0)",
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO channels (id, kind, name, position, bitrate, user_limit, text_in_voice, created_at)
             VALUES (10, 'text', 'general', 0, 64000, 0, TRUE, 0)",
        )
        .execute(&mut *conn)
        .await
        .unwrap();

        let now = 100 * DAY_MS;

        let make_row = |id: i64, state: &str, updated_at: i64| AttachmentRow {
            id,
            uploader_id: 1,
            channel_id: 10,
            message_id: None,
            position: 0,
            state: state.to_string(),
            kind: "file".to_string(),
            filename: "file.txt".to_string(),
            content_type: "text/plain".to_string(),
            size_bytes: 100,
            received_bytes: 0,
            storage_id: 1,
            key_enc: vec![1],
            width: None,
            height: None,
            duration_ms: None,
            animated: false,
            waveform: None,
            has_thumbnail: false,
            thumbnail_width: None,
            thumbnail_height: None,
            error_code: None,
            error_message: None,
            created_at: updated_at,
            updated_at,
        };

        // 1. Uploading older than 24h -> expired
        insert_upload(&mut conn, &make_row(1, "uploading", now - DAY_MS - 1000))
            .await
            .unwrap();
        // 2. Uploading fresh -> kept
        insert_upload(&mut conn, &make_row(2, "uploading", now - 1000))
            .await
            .unwrap();
        // 3. Ready older than 24h -> expired
        insert_upload(&mut conn, &make_row(3, "ready", now - DAY_MS - 1000))
            .await
            .unwrap();
        // 4. Ready fresh -> kept
        insert_upload(&mut conn, &make_row(4, "ready", now - 1000))
            .await
            .unwrap();
        // 5. Failed older than 1h -> expired
        insert_upload(&mut conn, &make_row(5, "failed", now - HOUR_MS - 1000))
            .await
            .unwrap();
        // 6. Failed fresh -> kept
        insert_upload(&mut conn, &make_row(6, "failed", now - 1000))
            .await
            .unwrap();

        let mut expired_ids = expired(&mut conn, now).await.unwrap();
        expired_ids.sort();
        assert_eq!(expired_ids, vec![1, 3, 5]);
    }
}
