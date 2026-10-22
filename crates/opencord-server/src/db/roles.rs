use sqlx::SqliteConnection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleRow {
    pub id: i64,
    pub name: String,
    pub color: i64,
    pub position: i64,
    pub permissions: i64,
    pub hoist: bool,
    pub mentionable: bool,
    pub max_upload_bytes: Option<i64>,
}

pub async fn list(conn: &mut SqliteConnection) -> Result<Vec<RoleRow>, sqlx::Error> {
    sqlx::query_as!(
        RoleRow,
        r#"SELECT id, name, color, position, permissions,
                  hoist AS "hoist: bool", mentionable AS "mentionable: bool",
                  max_upload_bytes
           FROM roles"#
    )
    .fetch_all(conn)
    .await
}

pub async fn insert(conn: &mut SqliteConnection, role: &RoleRow) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO roles (id, name, color, position, permissions, hoist, mentionable, max_upload_bytes)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        role.id,
        role.name,
        role.color,
        role.position,
        role.permissions,
        role.hoist,
        role.mentionable,
        role.max_upload_bytes
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn update(conn: &mut SqliteConnection, role: &RoleRow) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE roles SET name = $1, color = $2, position = $3, permissions = $4,
                          hoist = $5, mentionable = $6, max_upload_bytes = $7
         WHERE id = $8",
        role.name,
        role.color,
        role.position,
        role.permissions,
        role.hoist,
        role.mentionable,
        role.max_upload_bytes,
        role.id
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn delete(conn: &mut SqliteConnection, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM roles WHERE id = $1", id)
        .execute(conn)
        .await?;
    Ok(())
}
