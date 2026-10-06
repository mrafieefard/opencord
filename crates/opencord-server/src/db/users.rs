use sqlx::SqliteConnection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserRow {
    pub id: i64,
    pub public_key: Vec<u8>,
    pub display_name: String,
}

pub async fn find_by_public_key(
    conn: &mut SqliteConnection,
    public_key: &[u8],
) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as!(
        UserRow,
        "SELECT id, public_key, display_name FROM users WHERE public_key = $1",
        public_key
    )
    .fetch_optional(conn)
    .await
}

pub async fn find(conn: &mut SqliteConnection, id: i64) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as!(
        UserRow,
        "SELECT id, public_key, display_name FROM users WHERE id = $1",
        id
    )
    .fetch_optional(conn)
    .await
}

pub async fn insert(
    conn: &mut SqliteConnection,
    user: &UserRow,
    created_at: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO users (id, public_key, display_name, created_at) VALUES ($1, $2, $3, $4)",
        user.id,
        user.public_key,
        user.display_name,
        created_at
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn set_display_name(
    conn: &mut SqliteConnection,
    id: i64,
    display_name: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE users SET display_name = $1 WHERE id = $2",
        display_name,
        id
    )
    .execute(conn)
    .await?;
    Ok(())
}
