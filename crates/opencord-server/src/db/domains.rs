//! Domains and their check history (Phase 3 plan §10).

use sqlx::SqliteConnection;

/// Checks kept per domain (plan §6.4).
pub const KEEP_CHECKS: i64 = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainRow {
    pub id: i64,
    pub hostname: String,
    pub kind: String,
    pub public_port: i64,
    pub status: String,
    pub active: bool,
    pub is_primary: bool,
    pub problem_since: Option<i64>,
    pub deactivated_at: Option<i64>,
    pub last_checked_at: Option<i64>,
    pub last_result: Option<String>,
    pub created_at: i64,
}

pub async fn list(conn: &mut SqliteConnection) -> Result<Vec<DomainRow>, sqlx::Error> {
    sqlx::query_as!(
        DomainRow,
        "SELECT id, hostname, kind, public_port, status, active, is_primary, problem_since,
                deactivated_at, last_checked_at, last_result, created_at
         FROM domains ORDER BY id"
    )
    .fetch_all(conn)
    .await
}

pub async fn insert(conn: &mut SqliteConnection, row: &DomainRow) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO domains (id, hostname, kind, public_port, status, active, is_primary,
                              problem_since, deactivated_at, last_checked_at, last_result,
                              created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        row.id,
        row.hostname,
        row.kind,
        row.public_port,
        row.status,
        row.active,
        row.is_primary,
        row.problem_since,
        row.deactivated_at,
        row.last_checked_at,
        row.last_result,
        row.created_at
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Saves everything about a domain but its name, kind and creation.
pub async fn save(conn: &mut SqliteConnection, row: &DomainRow) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE domains SET public_port = $2, status = $3, active = $4, is_primary = $5,
                problem_since = $6, deactivated_at = $7, last_checked_at = $8,
                last_result = $9
         WHERE id = $1",
        row.id,
        row.public_port,
        row.status,
        row.active,
        row.is_primary,
        row.problem_since,
        row.deactivated_at,
        row.last_checked_at,
        row.last_result
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn delete(conn: &mut SqliteConnection, id: i64) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!("DELETE FROM domains WHERE id = $1", id)
        .execute(conn)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Records a check, keeping the newest [`KEEP_CHECKS`].
pub async fn add_check(
    conn: &mut SqliteConnection,
    domain_id: i64,
    checked_at: i64,
    result: &str,
    details: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO domain_checks (domain_id, checked_at, result, details)
         VALUES ($1, $2, $3, $4)",
        domain_id,
        checked_at,
        result,
        details
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "DELETE FROM domain_checks WHERE domain_id = $1 AND id NOT IN (
             SELECT id FROM domain_checks WHERE domain_id = $1
             ORDER BY checked_at DESC, id DESC LIMIT $2
         )",
        domain_id,
        KEEP_CHECKS
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// A domain's checks, newest first, as stored.
pub async fn checks(
    conn: &mut SqliteConnection,
    domain_id: i64,
) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query!(
        "SELECT details FROM domain_checks WHERE domain_id = $1
         ORDER BY checked_at DESC, id DESC",
        domain_id
    )
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().map(|row| row.details).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_pool;

    fn row(id: i64, hostname: &str) -> DomainRow {
        DomainRow {
            id,
            hostname: hostname.to_owned(),
            kind: "own".to_owned(),
            public_port: 7710,
            status: "pending".to_owned(),
            active: false,
            is_primary: false,
            problem_since: None,
            deactivated_at: None,
            last_checked_at: None,
            last_result: None,
            created_at: 1,
        }
    }

    #[tokio::test]
    async fn domains_save_and_load_back() {
        let (pool, _dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        insert(&mut conn, &row(1, "chat.example.com"))
            .await
            .unwrap();
        let mut changed = row(1, "chat.example.com");
        changed.status = "verified".to_owned();
        changed.active = true;
        changed.is_primary = true;
        changed.last_result = Some("{}".to_owned());

        save(&mut conn, &changed).await.unwrap();

        assert_eq!(list(&mut conn).await.unwrap(), [changed]);
        assert!(
            insert(&mut conn, &row(2, "chat.example.com"))
                .await
                .is_err()
        );
        assert!(delete(&mut conn, 1).await.unwrap());
        assert!(list(&mut conn).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn only_the_newest_checks_are_kept_and_go_with_their_domain() {
        let (pool, _dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        insert(&mut conn, &row(1, "a.example.com")).await.unwrap();
        for at in 0..15 {
            add_check(&mut conn, 1, at, "verified", &format!("{at}"))
                .await
                .unwrap();
        }

        let kept = checks(&mut conn, 1).await.unwrap();

        assert_eq!(kept.len(), 10);
        assert_eq!(kept[0], "14");
        assert_eq!(kept[9], "5");
        delete(&mut conn, 1).await.unwrap();
        assert!(checks(&mut conn, 1).await.unwrap().is_empty());
    }
}
