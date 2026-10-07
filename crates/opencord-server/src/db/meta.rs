//! Server-wide settings stored as key/value pairs in `server_meta`.

use std::collections::HashMap;

use opencord_common::auth::{SERVER_ID_LEN, ServerId};
use sqlx::SqliteConnection;

const SERVER_ID: &str = "server_id";
const NAME: &str = "name";
const DESCRIPTION: &str = "description";
const OWNER_USER_ID: &str = "owner_user_id";
const OPEN_JOIN: &str = "open_join";
const EVERYONE_ROLE_ID: &str = "everyone_role_id";
const CLAIM_TOKEN_HASH: &str = "claim_token_hash";
const DEFAULT_INVITE: &str = "default_invite";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerMeta {
    pub server_id: ServerId,
    pub name: String,
    pub description: String,
    pub owner_id: Option<i64>,
    pub open_join: bool,
    pub everyone_role_id: i64,
    /// SHA-256 of the pending owner claim token.
    pub claim_token_hash: Option<[u8; 32]>,
    /// Invite printed at startup for the owner to share.
    pub default_invite: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum MetaError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("server_meta.{key} is missing or invalid")]
    Corrupt { key: &'static str },
}

impl ServerMeta {
    /// `None` before the first start has initialized the server.
    pub async fn load(conn: &mut SqliteConnection) -> Result<Option<Self>, MetaError> {
        let rows = sqlx::query!("SELECT key, value FROM server_meta")
            .fetch_all(&mut *conn)
            .await?;
        let values: HashMap<String, String> =
            rows.into_iter().map(|row| (row.key, row.value)).collect();
        if !values.contains_key(SERVER_ID) {
            return Ok(None);
        }
        let required =
            |key: &'static str| values.get(key).cloned().ok_or(MetaError::Corrupt { key });
        let parse_id = |key: &'static str, value: &str| {
            value.parse::<i64>().map_err(|_| MetaError::Corrupt { key })
        };
        let mut server_id = [0; SERVER_ID_LEN];
        hex::decode_to_slice(required(SERVER_ID)?, &mut server_id)
            .map_err(|_| MetaError::Corrupt { key: SERVER_ID })?;
        let claim_token_hash = values
            .get(CLAIM_TOKEN_HASH)
            .map(|value| {
                let mut hash = [0; 32];
                hex::decode_to_slice(value, &mut hash)
                    .map(|()| hash)
                    .map_err(|_| MetaError::Corrupt {
                        key: CLAIM_TOKEN_HASH,
                    })
            })
            .transpose()?;
        Ok(Some(Self {
            server_id,
            name: required(NAME)?,
            description: values.get(DESCRIPTION).cloned().unwrap_or_default(),
            owner_id: values
                .get(OWNER_USER_ID)
                .map(|value| parse_id(OWNER_USER_ID, value))
                .transpose()?,
            open_join: values.get(OPEN_JOIN).is_some_and(|value| value == "true"),
            everyone_role_id: parse_id(EVERYONE_ROLE_ID, &required(EVERYONE_ROLE_ID)?)?,
            claim_token_hash,
            default_invite: values.get(DEFAULT_INVITE).cloned(),
        }))
    }

    pub async fn save(&self, conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
        let entries = [
            (SERVER_ID, Some(hex::encode(self.server_id))),
            (NAME, Some(self.name.clone())),
            (DESCRIPTION, Some(self.description.clone())),
            (OWNER_USER_ID, self.owner_id.map(|id| id.to_string())),
            (OPEN_JOIN, Some(self.open_join.to_string())),
            (EVERYONE_ROLE_ID, Some(self.everyone_role_id.to_string())),
            (CLAIM_TOKEN_HASH, self.claim_token_hash.map(hex::encode)),
            (DEFAULT_INVITE, self.default_invite.clone()),
        ];
        for (key, value) in entries {
            put(conn, key, value.as_deref()).await?;
        }
        Ok(())
    }
}

/// Stores `value` under `key`, or removes the key for `None`.
pub async fn put(
    conn: &mut SqliteConnection,
    key: &str,
    value: Option<&str>,
) -> Result<(), sqlx::Error> {
    match value {
        Some(value) => {
            sqlx::query!(
                "INSERT INTO server_meta (key, value) VALUES ($1, $2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                key,
                value
            )
            .execute(&mut *conn)
            .await?;
        }
        None => {
            sqlx::query!("DELETE FROM server_meta WHERE key = $1", key)
                .execute(&mut *conn)
                .await?;
        }
    }
    Ok(())
}

/// Every key starting with `prefix`, with its value.
pub async fn load_prefixed(
    conn: &mut SqliteConnection,
    prefix: &str,
) -> Result<HashMap<String, String>, sqlx::Error> {
    let pattern = format!("{prefix}%");
    let rows = sqlx::query!(
        "SELECT key, value FROM server_meta WHERE key LIKE $1",
        pattern
    )
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().map(|row| (row.key, row.value)).collect())
}
