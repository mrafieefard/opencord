use sqlx::{Connection, SqliteConnection};

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct StorageRow {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub endpoint: Option<String>,
    pub region: Option<String>,
    pub bucket: Option<String>,
    pub key_prefix: String,
    pub path_style: bool,
    pub allow_http: bool,
    pub access_key_id: Option<String>,
    pub secret_enc: Option<Vec<u8>>,
    pub active: bool,
    pub last_error: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct BlobRow {
    pub id: i64,
    pub storage_id: i64,
    pub object_key: String,
    pub attempts: i64,
}

pub async fn list(conn: &mut SqliteConnection) -> Result<Vec<StorageRow>, sqlx::Error> {
    sqlx::query_as!(
        StorageRow,
        r#"SELECT id, kind, name, endpoint, region, bucket,
                  key_prefix, path_style AS "path_style: bool",
                  allow_http AS "allow_http: bool", access_key_id,
                  secret_enc, active AS "active: bool", last_error,
                  created_at
           FROM storage_backends ORDER BY id"#
    )
    .fetch_all(conn)
    .await
}

pub async fn insert(conn: &mut SqliteConnection, row: &StorageRow) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO storage_backends (
            id, kind, name, endpoint, region, bucket, key_prefix,
            path_style, allow_http, access_key_id, secret_enc,
            active, last_error, created_at
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        row.id,
        row.kind,
        row.name,
        row.endpoint,
        row.region,
        row.bucket,
        row.key_prefix,
        row.path_style,
        row.allow_http,
        row.access_key_id,
        row.secret_enc,
        row.active,
        row.last_error,
        row.created_at
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn update(conn: &mut SqliteConnection, row: &StorageRow) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE storage_backends
         SET kind = $2, name = $3, endpoint = $4, region = $5,
             bucket = $6, key_prefix = $7, path_style = $8,
             allow_http = $9, access_key_id = $10, secret_enc = $11,
             active = $12, last_error = $13
         WHERE id = $1",
        row.id,
        row.kind,
        row.name,
        row.endpoint,
        row.region,
        row.bucket,
        row.key_prefix,
        row.path_style,
        row.allow_http,
        row.access_key_id,
        row.secret_enc,
        row.active,
        row.last_error
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn set_active(conn: &mut SqliteConnection, id: i64) -> Result<(), sqlx::Error> {
    let mut tx = conn.begin().await?;
    sqlx::query!("UPDATE storage_backends SET active = FALSE WHERE active")
        .execute(&mut *tx)
        .await?;
    sqlx::query!(
        "UPDATE storage_backends SET active = TRUE WHERE id = $1",
        id
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn delete(conn: &mut SqliteConnection, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM storage_backends WHERE id = $1", id)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn set_last_error(
    conn: &mut SqliteConnection,
    id: i64,
    error: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE storage_backends SET last_error = $1 WHERE id = $2",
        error,
        id
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn queue_blob(
    conn: &mut SqliteConnection,
    storage_id: i64,
    key: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO blob_deletions (storage_id, object_key) VALUES ($1, $2)",
        storage_id,
        key
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn due_blobs(
    conn: &mut SqliteConnection,
    now: i64,
    limit: i64,
) -> Result<Vec<BlobRow>, sqlx::Error> {
    sqlx::query_as!(
        BlobRow,
        "SELECT id, storage_id, object_key, attempts
         FROM blob_deletions
         WHERE next_attempt_at <= $1
         ORDER BY id
         LIMIT $2",
        now,
        limit
    )
    .fetch_all(conn)
    .await
}

pub async fn blob_done(conn: &mut SqliteConnection, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM blob_deletions WHERE id = $1", id)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn blob_retry(
    conn: &mut SqliteConnection,
    id: i64,
    attempts: i64,
    next_at: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE blob_deletions SET attempts = $1, next_attempt_at = $2 WHERE id = $3",
        attempts,
        next_at,
        id
    )
    .execute(conn)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_pool;

    #[tokio::test]
    async fn one_backend_is_active() {
        let (pool, _dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        let backends = list(&mut conn).await.unwrap();
        assert_eq!(backends.len(), 1);
        assert!(backends[0].active);

        let row2 = StorageRow {
            id: 2,
            kind: "local".to_string(),
            name: "Second".to_string(),
            endpoint: None,
            region: None,
            bucket: None,
            key_prefix: "".to_string(),
            path_style: false,
            allow_http: false,
            access_key_id: None,
            secret_enc: None,
            active: false,
            last_error: None,
            created_at: 0,
        };
        insert(&mut conn, &row2).await.unwrap();

        set_active(&mut conn, 2).await.unwrap();

        let updated = list(&mut conn).await.unwrap();
        assert_eq!(updated.len(), 2);
        assert!(!updated[0].active);
        assert!(updated[1].active);
    }
}
