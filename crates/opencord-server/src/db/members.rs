use sqlx::SqliteConnection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberRow {
    pub user_id: i64,
    pub public_key: Vec<u8>,
    pub display_name: String,
    pub nickname: Option<String>,
    pub joined_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoleAssignment {
    pub user_id: i64,
    pub role_id: i64,
}

pub async fn list(conn: &mut SqliteConnection) -> Result<Vec<MemberRow>, sqlx::Error> {
    sqlx::query_as!(
        MemberRow,
        "SELECT m.user_id, u.public_key, u.display_name, m.nickname, m.joined_at
         FROM members m JOIN users u ON u.id = m.user_id"
    )
    .fetch_all(conn)
    .await
}

pub async fn list_role_assignments(
    conn: &mut SqliteConnection,
) -> Result<Vec<RoleAssignment>, sqlx::Error> {
    sqlx::query_as!(RoleAssignment, "SELECT user_id, role_id FROM member_roles")
        .fetch_all(conn)
        .await
}

pub async fn insert(
    conn: &mut SqliteConnection,
    user_id: i64,
    joined_at: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO members (user_id, joined_at) VALUES ($1, $2)",
        user_id,
        joined_at
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn delete(conn: &mut SqliteConnection, user_id: i64) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM members WHERE user_id = $1", user_id)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn set_nickname(
    conn: &mut SqliteConnection,
    user_id: i64,
    nickname: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE members SET nickname = $1 WHERE user_id = $2",
        nickname,
        user_id
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn add_role(
    conn: &mut SqliteConnection,
    user_id: i64,
    role_id: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO member_roles (user_id, role_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        user_id,
        role_id
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn remove_role(
    conn: &mut SqliteConnection,
    user_id: i64,
    role_id: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM member_roles WHERE user_id = $1 AND role_id = $2",
        user_id,
        role_id
    )
    .execute(conn)
    .await?;
    Ok(())
}
