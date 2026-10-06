use sqlx::SqliteConnection;

use crate::random;

pub const CODE_LEN: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InviteRow {
    pub code: String,
    pub created_by: Option<i64>,
    pub created_at: i64,
    pub max_uses: Option<i64>,
    pub uses: i64,
    pub expires_at: Option<i64>,
}

impl InviteRow {
    pub fn is_usable(&self, now_ms: i64) -> bool {
        let unexpired = self.expires_at.is_none_or(|expires_at| now_ms < expires_at);
        let uses_left = self.max_uses.is_none_or(|max_uses| self.uses < max_uses);
        unexpired && uses_left
    }
}

/// Creates an invite with a fresh code. 0 or `None` means unlimited uses or
/// no expiry.
pub async fn create(
    conn: &mut SqliteConnection,
    created_by: Option<i64>,
    max_uses: Option<u32>,
    expires_in_s: Option<u32>,
    now_ms: i64,
) -> Result<InviteRow, sqlx::Error> {
    let invite = InviteRow {
        code: random::code(CODE_LEN),
        created_by,
        created_at: now_ms,
        max_uses: max_uses.filter(|uses| *uses > 0).map(i64::from),
        uses: 0,
        expires_at: expires_in_s
            .filter(|seconds| *seconds > 0)
            .map(|seconds| now_ms + i64::from(seconds) * 1_000),
    };
    insert(conn, &invite).await?;
    Ok(invite)
}

pub async fn insert(conn: &mut SqliteConnection, invite: &InviteRow) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO invites (code, created_by, created_at, max_uses, uses, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6)",
        invite.code,
        invite.created_by,
        invite.created_at,
        invite.max_uses,
        invite.uses,
        invite.expires_at
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn find(
    conn: &mut SqliteConnection,
    code: &str,
) -> Result<Option<InviteRow>, sqlx::Error> {
    sqlx::query_as!(
        InviteRow,
        "SELECT code, created_by, created_at, max_uses, uses, expires_at
         FROM invites WHERE code = $1",
        code
    )
    .fetch_optional(conn)
    .await
}

pub async fn list(conn: &mut SqliteConnection) -> Result<Vec<InviteRow>, sqlx::Error> {
    sqlx::query_as!(
        InviteRow,
        "SELECT code, created_by, created_at, max_uses, uses, expires_at
         FROM invites ORDER BY created_at"
    )
    .fetch_all(conn)
    .await
}

pub async fn increment_uses(conn: &mut SqliteConnection, code: &str) -> Result<(), sqlx::Error> {
    sqlx::query!("UPDATE invites SET uses = uses + 1 WHERE code = $1", code)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn delete(conn: &mut SqliteConnection, code: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!("DELETE FROM invites WHERE code = $1", code)
        .execute(conn)
        .await?;
    Ok(result.rows_affected() > 0)
}
