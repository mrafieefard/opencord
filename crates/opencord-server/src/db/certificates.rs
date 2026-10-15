//! Certificates, ACME accounts and DNS providers (Phase 3 plan §7, §10).
//! Secrets in these rows are sealed; see [`crate::secrets`].

use sqlx::SqliteConnection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificateRow {
    pub domain_id: i64,
    pub chain_pem: String,
    pub key_enc: Vec<u8>,
    pub issuer: String,
    pub not_before: i64,
    pub not_after: i64,
    pub is_test: bool,
    pub uploaded: bool,
    pub created_at: i64,
}

pub async fn list(conn: &mut SqliteConnection) -> Result<Vec<CertificateRow>, sqlx::Error> {
    sqlx::query_as!(
        CertificateRow,
        "SELECT domain_id, chain_pem, key_enc, issuer, not_before, not_after, is_test, uploaded,
                created_at
         FROM domain_certificates"
    )
    .fetch_all(conn)
    .await
}

/// Saves a domain's certificate, replacing the one before.
pub async fn put(conn: &mut SqliteConnection, row: &CertificateRow) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO domain_certificates (domain_id, chain_pem, key_enc, issuer, not_before,
                                          not_after, is_test, uploaded, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         ON CONFLICT (domain_id) DO UPDATE SET
             chain_pem = excluded.chain_pem, key_enc = excluded.key_enc,
             issuer = excluded.issuer, not_before = excluded.not_before,
             not_after = excluded.not_after, is_test = excluded.is_test,
             uploaded = excluded.uploaded, created_at = excluded.created_at",
        row.domain_id,
        row.chain_pem,
        row.key_enc,
        row.issuer,
        row.not_before,
        row.not_after,
        row.is_test,
        row.uploaded,
        row.created_at
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn delete(conn: &mut SqliteConnection, domain_id: i64) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM domain_certificates WHERE domain_id = $1",
        domain_id
    )
    .execute(conn)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountRow {
    pub directory: String,
    pub credentials_enc: Vec<u8>,
    pub email: Option<String>,
    pub created_at: i64,
}

pub async fn account(
    conn: &mut SqliteConnection,
    directory: &str,
) -> Result<Option<AccountRow>, sqlx::Error> {
    sqlx::query_as!(
        AccountRow,
        "SELECT directory, credentials_enc, email, created_at FROM acme_accounts
         WHERE directory = $1",
        directory
    )
    .fetch_optional(conn)
    .await
}

pub async fn accounts(conn: &mut SqliteConnection) -> Result<Vec<AccountRow>, sqlx::Error> {
    sqlx::query_as!(
        AccountRow,
        "SELECT directory, credentials_enc, email, created_at FROM acme_accounts"
    )
    .fetch_all(conn)
    .await
}

pub async fn put_account(conn: &mut SqliteConnection, row: &AccountRow) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO acme_accounts (directory, credentials_enc, email, created_at)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (directory) DO UPDATE SET
             credentials_enc = excluded.credentials_enc, email = excluded.email",
        row.directory,
        row.credentials_enc,
        row.email,
        row.created_at
    )
    .execute(conn)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRow {
    pub id: i64,
    pub kind: String,
    pub token_enc: Vec<u8>,
    pub created_at: i64,
}

pub async fn providers(conn: &mut SqliteConnection) -> Result<Vec<ProviderRow>, sqlx::Error> {
    sqlx::query_as!(
        ProviderRow,
        "SELECT id, kind, token_enc, created_at FROM dns_providers ORDER BY id"
    )
    .fetch_all(conn)
    .await
}

/// Saves the token for a kind of provider, replacing the one before.
pub async fn put_provider(
    conn: &mut SqliteConnection,
    row: &ProviderRow,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO dns_providers (id, kind, token_enc, created_at) VALUES ($1, $2, $3, $4)
         ON CONFLICT (kind) DO UPDATE SET token_enc = excluded.token_enc",
        row.id,
        row.kind,
        row.token_enc,
        row.created_at
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn delete_provider(conn: &mut SqliteConnection, kind: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!("DELETE FROM dns_providers WHERE kind = $1", kind)
        .execute(conn)
        .await?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::domains::{self, DomainRow};
    use crate::db::test_pool;

    #[tokio::test]
    async fn a_certificate_replaces_the_last_and_goes_with_its_domain() {
        let (pool, _dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        domains::insert(
            &mut conn,
            &DomainRow {
                id: 1,
                hostname: "chat.example.com".into(),
                kind: "own".into(),
                public_port: 7710,
                status: "verified".into(),
                active: true,
                is_primary: true,
                problem_since: None,
                deactivated_at: None,
                last_checked_at: None,
                last_result: None,
                created_at: 0,
            },
        )
        .await
        .unwrap();
        let mut row = CertificateRow {
            domain_id: 1,
            chain_pem: "first".into(),
            key_enc: vec![1],
            issuer: "Test".into(),
            not_before: 1,
            not_after: 2,
            is_test: false,
            uploaded: false,
            created_at: 1,
        };
        put(&mut conn, &row).await.unwrap();
        row.chain_pem = "second".into();
        put(&mut conn, &row).await.unwrap();

        assert_eq!(list(&mut conn).await.unwrap(), [row]);
        domains::delete(&mut conn, 1).await.unwrap();
        assert!(list(&mut conn).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn accounts_and_providers_keep_one_per_kind() {
        let (pool, _dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let account_row = AccountRow {
            directory: "https://acme".into(),
            credentials_enc: vec![1],
            email: None,
            created_at: 1,
        };
        put_account(&mut conn, &account_row).await.unwrap();
        assert_eq!(
            account(&mut conn, "https://acme").await.unwrap(),
            Some(account_row)
        );
        put_provider(
            &mut conn,
            &ProviderRow {
                id: 1,
                kind: "cloudflare".into(),
                token_enc: vec![1],
                created_at: 1,
            },
        )
        .await
        .unwrap();
        put_provider(
            &mut conn,
            &ProviderRow {
                id: 2,
                kind: "cloudflare".into(),
                token_enc: vec![2],
                created_at: 2,
            },
        )
        .await
        .unwrap();
        let listed = providers(&mut conn).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].token_enc, [2]);
        assert!(delete_provider(&mut conn, "cloudflare").await.unwrap());
    }
}
