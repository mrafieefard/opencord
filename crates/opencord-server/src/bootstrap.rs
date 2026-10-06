//! First-start setup and the owner claim token.

use opencord_common::auth::SERVER_ID_LEN;
use opencord_common::channel::ChannelKind;
use opencord_common::permissions::Permissions;
use opencord_common::snowflake::SnowflakeGenerator;
use opencord_common::validation;
use sha2::{Digest, Sha256};
use sqlx::{SqliteConnection, SqlitePool};

use crate::db::channels::{self, ChannelRow};
use crate::db::invites;
use crate::db::meta::{MetaError, ServerMeta};
use crate::db::permissions_to_db;
use crate::db::roles::{self, RoleRow};
use crate::random;

pub const EVERYONE_ROLE_NAME: &str = "@everyone";

#[derive(Debug)]
pub struct Bootstrap {
    pub meta: ServerMeta,
    /// Fresh claim token, issued on every start while the server has no owner.
    pub claim_token: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum BootstrapError {
    #[error(transparent)]
    Meta(#[from] MetaError),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("the server has not been started yet; run it once first")]
    NotInitialized,
}

/// Creates the server on first start (id, @everyone, default channels) and
/// issues a new claim token while there is no owner.
pub async fn initialize(
    pool: &SqlitePool,
    server_name: &str,
    ids: &SnowflakeGenerator,
    now_ms: i64,
) -> Result<Bootstrap, BootstrapError> {
    let mut tx = pool.begin().await?;
    let meta = match ServerMeta::load(&mut tx).await? {
        Some(meta) => meta,
        None => create_server(&mut tx, server_name, ids, now_ms).await?,
    };
    let (meta, claim_token) = if meta.owner_id.is_none() {
        let token = new_claim_token();
        let meta = ServerMeta {
            claim_token_hash: Some(hash_claim_token(&token)),
            ..meta
        };
        (meta, Some(token))
    } else {
        (meta, None)
    };
    meta.save(&mut tx).await?;
    tx.commit().await?;
    Ok(Bootstrap { meta, claim_token })
}

/// Issues a new claim token. Whoever uses it becomes the owner, replacing
/// the current one, so operators can recover a server whose owner lost
/// their identity.
pub async fn reset_claim_token(pool: &SqlitePool) -> Result<String, BootstrapError> {
    let mut tx = pool.begin().await?;
    let meta = ServerMeta::load(&mut tx)
        .await?
        .ok_or(BootstrapError::NotInitialized)?;
    let token = new_claim_token();
    ServerMeta {
        claim_token_hash: Some(hash_claim_token(&token)),
        ..meta
    }
    .save(&mut tx)
    .await?;
    tx.commit().await?;
    Ok(token)
}

/// Code of a permanent, unlimited invite for the owner to share, created on
/// demand. `None` while the server has no owner.
pub async fn startup_invite(
    pool: &SqlitePool,
    now_ms: i64,
) -> Result<Option<String>, BootstrapError> {
    let mut tx = pool.begin().await?;
    let meta = ServerMeta::load(&mut tx)
        .await?
        .ok_or(BootstrapError::NotInitialized)?;
    if meta.owner_id.is_none() {
        return Ok(None);
    }
    if let Some(code) = &meta.default_invite {
        let existing = invites::find(&mut tx, code).await?;
        if existing.is_some_and(|invite| invite.is_usable(now_ms)) {
            return Ok(Some(code.clone()));
        }
    }
    let invite = invites::create(&mut tx, None, None, None, now_ms).await?;
    ServerMeta {
        default_invite: Some(invite.code.clone()),
        ..meta
    }
    .save(&mut tx)
    .await?;
    tx.commit().await?;
    Ok(Some(invite.code))
}

pub fn hash_claim_token(token: &str) -> [u8; 32] {
    Sha256::digest(token.trim().as_bytes()).into()
}

fn new_claim_token() -> String {
    hex::encode(random::bytes::<16>())
}

async fn create_server(
    conn: &mut SqliteConnection,
    server_name: &str,
    ids: &SnowflakeGenerator,
    now_ms: i64,
) -> Result<ServerMeta, BootstrapError> {
    let everyone_role_id = ids.next_id();
    roles::insert(
        conn,
        &RoleRow {
            id: everyone_role_id,
            name: EVERYONE_ROLE_NAME.to_owned(),
            color: 0,
            position: 0,
            permissions: permissions_to_db(Permissions::DEFAULT_EVERYONE),
            hoist: false,
            mentionable: false,
        },
    )
    .await?;
    let default_channels = [
        (ChannelKind::Text, "general"),
        (ChannelKind::Voice, "General"),
    ];
    for (position, (kind, name)) in (0..).zip(default_channels) {
        let channel = ChannelRow {
            id: ids.next_id(),
            kind: kind.as_str().to_owned(),
            name: name.to_owned(),
            topic: None,
            parent_id: None,
            position,
        };
        channels::insert(conn, &channel, now_ms).await?;
    }
    Ok(ServerMeta {
        server_id: random::bytes::<SERVER_ID_LEN>(),
        name: validation::server_name(server_name).unwrap_or_else(|_| "Opencord".to_owned()),
        description: String::new(),
        owner_id: None,
        open_join: false,
        everyone_role_id,
        claim_token_hash: None,
        default_invite: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_pool;

    const NOW: i64 = 1_800_000_000_000;

    fn ids() -> SnowflakeGenerator {
        SnowflakeGenerator::new(0).unwrap()
    }

    async fn set_owner(pool: &SqlitePool, owner_id: i64) {
        let mut conn = pool.acquire().await.unwrap();
        let meta = ServerMeta::load(&mut conn).await.unwrap().unwrap();
        ServerMeta {
            owner_id: Some(owner_id),
            claim_token_hash: None,
            ..meta
        }
        .save(&mut conn)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn first_start_creates_the_default_server() {
        let (pool, _dir) = test_pool().await;

        let bootstrap = initialize(&pool, "Test Server", &ids(), NOW).await.unwrap();

        let mut conn = pool.acquire().await.unwrap();
        let roles = roles::list(&mut conn).await.unwrap();
        let mut channels = channels::list(&mut conn).await.unwrap();
        channels.sort_by_key(|channel| channel.position);
        let token = bootstrap.claim_token.expect("a claim token while unowned");

        assert_eq!(bootstrap.meta.name, "Test Server");
        assert_eq!(bootstrap.meta.owner_id, None);
        assert_eq!(
            bootstrap.meta.claim_token_hash,
            Some(hash_claim_token(&token))
        );
        assert_eq!(roles.len(), 1);
        assert_eq!(roles[0].id, bootstrap.meta.everyone_role_id);
        assert_eq!(roles[0].name, EVERYONE_ROLE_NAME);
        assert_eq!(roles[0].position, 0);
        assert_eq!(
            roles[0].permissions,
            permissions_to_db(Permissions::DEFAULT_EVERYONE)
        );
        let summary: Vec<(&str, &str)> = channels
            .iter()
            .map(|channel| (channel.kind.as_str(), channel.name.as_str()))
            .collect();
        assert_eq!(summary, [("text", "general"), ("voice", "General")]);
    }

    #[tokio::test]
    async fn later_starts_keep_the_server_and_rotate_the_claim_token() {
        let (pool, _dir) = test_pool().await;
        let first = initialize(&pool, "Test", &ids(), NOW).await.unwrap();

        let second = initialize(&pool, "Ignored", &ids(), NOW).await.unwrap();

        assert_eq!(second.meta.server_id, first.meta.server_id);
        assert_eq!(second.meta.name, "Test");
        assert_ne!(second.claim_token, first.claim_token);
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(roles::list(&mut conn).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn no_claim_token_once_the_server_has_an_owner() {
        let (pool, _dir) = test_pool().await;
        initialize(&pool, "Test", &ids(), NOW).await.unwrap();
        set_owner(&pool, 42).await;

        let bootstrap = initialize(&pool, "Test", &ids(), NOW).await.unwrap();

        assert_eq!(bootstrap.claim_token, None);
        assert_eq!(bootstrap.meta.claim_token_hash, None);
    }

    #[tokio::test]
    async fn reset_issues_a_token_even_with_an_owner() {
        let (pool, _dir) = test_pool().await;
        initialize(&pool, "Test", &ids(), NOW).await.unwrap();
        set_owner(&pool, 42).await;

        let token = reset_claim_token(&pool).await.unwrap();

        let mut conn = pool.acquire().await.unwrap();
        let meta = ServerMeta::load(&mut conn).await.unwrap().unwrap();
        assert_eq!(meta.claim_token_hash, Some(hash_claim_token(&token)));
        assert_eq!(meta.owner_id, Some(42));
    }

    #[tokio::test]
    async fn reset_needs_an_initialized_server() {
        let (pool, _dir) = test_pool().await;

        assert!(matches!(
            reset_claim_token(&pool).await,
            Err(BootstrapError::NotInitialized)
        ));
    }

    #[tokio::test]
    async fn startup_invite_waits_for_an_owner_then_is_reused() {
        let (pool, _dir) = test_pool().await;
        initialize(&pool, "Test", &ids(), NOW).await.unwrap();

        let unowned = startup_invite(&pool, NOW).await.unwrap();
        set_owner(&pool, 42).await;
        let first = startup_invite(&pool, NOW).await.unwrap().unwrap();
        let second = startup_invite(&pool, NOW + 1).await.unwrap().unwrap();

        assert_eq!(unowned, None);
        assert_eq!(first, second);
        let mut conn = pool.acquire().await.unwrap();
        let invite = invites::find(&mut conn, &first).await.unwrap().unwrap();
        assert_eq!((invite.max_uses, invite.expires_at), (None, None));
    }

    #[tokio::test]
    async fn startup_invite_is_replaced_once_revoked() {
        let (pool, _dir) = test_pool().await;
        initialize(&pool, "Test", &ids(), NOW).await.unwrap();
        set_owner(&pool, 42).await;
        let first = startup_invite(&pool, NOW).await.unwrap().unwrap();
        invites::delete(&mut pool.acquire().await.unwrap(), &first)
            .await
            .unwrap();

        let second = startup_invite(&pool, NOW).await.unwrap().unwrap();

        assert_ne!(first, second);
    }

    #[test]
    fn claim_token_hash_ignores_surrounding_whitespace() {
        assert_eq!(hash_claim_token(" abc \n"), hash_claim_token("abc"));
    }
}
