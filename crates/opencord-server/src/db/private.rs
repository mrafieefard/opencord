//! Private messages: devices, their keys, privacy settings and blocks, and
//! the events waiting for each device.

use sqlx::SqliteConnection;

use crate::private::PrivateCounts;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivacyRow {
    pub user_id: i64,
    pub allow_friend_requests: bool,
    pub allow_message_requests: bool,
}

pub async fn privacy_settings(conn: &mut SqliteConnection) -> Result<Vec<PrivacyRow>, sqlx::Error> {
    sqlx::query_as!(
        PrivacyRow,
        r#"SELECT user_id, allow_friend_requests AS "allow_friend_requests: bool",
                  allow_message_requests AS "allow_message_requests: bool"
           FROM private_privacy"#
    )
    .fetch_all(conn)
    .await
}

/// Every block, as (owner, blocked).
pub async fn blocks(conn: &mut SqliteConnection) -> Result<Vec<(i64, i64)>, sqlx::Error> {
    let rows = sqlx::query!("SELECT owner_id, blocked_id FROM private_blocks")
        .fetch_all(conn)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.owner_id, row.blocked_id))
        .collect())
}

/// Replaces the member's settings and block list. Run it in a transaction.
pub async fn save_privacy(
    conn: &mut SqliteConnection,
    privacy: &PrivacyRow,
    blocked: &[i64],
    now_ms: i64,
) -> Result<(), sqlx::Error> {
    let user_id = privacy.user_id;
    sqlx::query!(
        "INSERT INTO private_privacy (user_id, allow_friend_requests, allow_message_requests,
                                      updated_at)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (user_id) DO UPDATE SET
             allow_friend_requests = excluded.allow_friend_requests,
             allow_message_requests = excluded.allow_message_requests,
             updated_at = excluded.updated_at",
        user_id,
        privacy.allow_friend_requests,
        privacy.allow_message_requests,
        now_ms
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!("DELETE FROM private_blocks WHERE owner_id = $1", user_id)
        .execute(&mut *conn)
        .await?;
    for blocked_id in blocked {
        sqlx::query!(
            "INSERT INTO private_blocks (owner_id, blocked_id) VALUES ($1, $2)",
            user_id,
            blocked_id
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceRow {
    pub device_id: Vec<u8>,
    pub curve25519: Vec<u8>,
    pub ed25519: Vec<u8>,
    pub signature: Vec<u8>,
    pub fallback_key: Vec<u8>,
    pub fallback_signature: Vec<u8>,
}

pub async fn device(
    conn: &mut SqliteConnection,
    user_id: i64,
    device_id: &[u8],
) -> Result<Option<DeviceRow>, sqlx::Error> {
    sqlx::query_as!(
        DeviceRow,
        "SELECT device_id, curve25519, ed25519, signature, fallback_key, fallback_signature
         FROM private_devices WHERE user_id = $1 AND device_id = $2",
        user_id,
        device_id
    )
    .fetch_optional(conn)
    .await
}

/// The member's devices, oldest first.
pub async fn devices(
    conn: &mut SqliteConnection,
    user_id: i64,
) -> Result<Vec<DeviceRow>, sqlx::Error> {
    sqlx::query_as!(
        DeviceRow,
        "SELECT device_id, curve25519, ed25519, signature, fallback_key, fallback_signature
         FROM private_devices WHERE user_id = $1 ORDER BY created_at, rowid",
        user_id
    )
    .fetch_all(conn)
    .await
}

/// Deletes the device's one-time keys if it is registered with other keys
/// than these: they were signed by its old key. One statement, so it
/// compares with the row as it is now.
pub async fn delete_one_time_keys_if_replaced(
    conn: &mut SqliteConnection,
    user_id: i64,
    device: &DeviceRow,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM private_one_time_keys
         WHERE user_id = $1 AND device_id = $2
           AND EXISTS (SELECT 1 FROM private_devices
                       WHERE user_id = $1 AND device_id = $2
                         AND (curve25519 <> $3 OR ed25519 <> $4))",
        user_id,
        device.device_id,
        device.curve25519,
        device.ed25519
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Adds the device or replaces its keys; `created_at` stays.
pub async fn upsert_device(
    conn: &mut SqliteConnection,
    user_id: i64,
    device: &DeviceRow,
    now_ms: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO private_devices (user_id, device_id, curve25519, ed25519, signature,
                                      fallback_key, fallback_signature, created_at, last_seen_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)
         ON CONFLICT (user_id, device_id) DO UPDATE SET
             curve25519 = excluded.curve25519,
             ed25519 = excluded.ed25519,
             signature = excluded.signature,
             fallback_key = excluded.fallback_key,
             fallback_signature = excluded.fallback_signature,
             last_seen_at = excluded.last_seen_at",
        user_id,
        device.device_id,
        device.curve25519,
        device.ed25519,
        device.signature,
        device.fallback_key,
        device.fallback_signature,
        now_ms
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Deletes the member's devices beyond the `keep_others` seen most
/// recently, never `keep`. Returns the ids deleted.
pub async fn delete_devices_beyond(
    conn: &mut SqliteConnection,
    user_id: i64,
    keep: &[u8],
    keep_others: i64,
) -> Result<Vec<Vec<u8>>, sqlx::Error> {
    let rows = sqlx::query!(
        "DELETE FROM private_devices
         WHERE user_id = $1 AND device_id IN (
             SELECT device_id FROM private_devices
             WHERE user_id = $1 AND device_id <> $2
             ORDER BY last_seen_at DESC, rowid DESC
             LIMIT -1 OFFSET $3)
         RETURNING device_id",
        user_id,
        keep,
        keep_others
    )
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().map(|row| row.device_id).collect())
}

pub async fn delete_device(
    conn: &mut SqliteConnection,
    user_id: i64,
    device_id: &[u8],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM private_devices WHERE user_id = $1 AND device_id = $2",
        user_id,
        device_id
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Marks the device as seen now; `false` when it is gone.
pub async fn touch_device(
    conn: &mut SqliteConnection,
    user_id: i64,
    device_id: &[u8],
    now_ms: i64,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "UPDATE private_devices SET last_seen_at = $1 WHERE user_id = $2 AND device_id = $3",
        now_ms,
        user_id,
        device_id
    )
    .execute(conn)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn set_fallback_key(
    conn: &mut SqliteConnection,
    user_id: i64,
    device_id: &[u8],
    key: &[u8],
    signature: &[u8],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE private_devices SET fallback_key = $1, fallback_signature = $2
         WHERE user_id = $3 AND device_id = $4",
        key,
        signature,
        user_id,
        device_id
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// A key already stored is left as it is.
pub async fn insert_one_time_key(
    conn: &mut SqliteConnection,
    user_id: i64,
    device_id: &[u8],
    key: &[u8],
    signature: &[u8],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT OR IGNORE INTO private_one_time_keys (user_id, device_id, key, signature)
         VALUES ($1, $2, $3, $4)",
        user_id,
        device_id,
        key,
        signature
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Deletes the device's oldest one-time keys beyond `keep`.
pub async fn trim_one_time_keys(
    conn: &mut SqliteConnection,
    user_id: i64,
    device_id: &[u8],
    keep: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM private_one_time_keys WHERE rowid IN (
             SELECT rowid FROM private_one_time_keys
             WHERE user_id = $1 AND device_id = $2
             ORDER BY rowid DESC
             LIMIT -1 OFFSET $3)",
        user_id,
        device_id,
        keep
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Takes one of the device's one-time keys, oldest first, as (key,
/// signature). One statement, so two claims never get the same key.
pub async fn claim_one_time_key(
    conn: &mut SqliteConnection,
    user_id: i64,
    device_id: &[u8],
) -> Result<Option<(Vec<u8>, Vec<u8>)>, sqlx::Error> {
    let row = sqlx::query!(
        "DELETE FROM private_one_time_keys WHERE rowid = (
             SELECT rowid FROM private_one_time_keys
             WHERE user_id = $1 AND device_id = $2
             ORDER BY rowid LIMIT 1)
         RETURNING key, signature",
        user_id,
        device_id
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|row| (row.key, row.signature)))
}

pub async fn count_one_time_keys(
    conn: &mut SqliteConnection,
    user_id: i64,
    device_id: &[u8],
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT COUNT(*) FROM private_one_time_keys WHERE user_id = $1 AND device_id = $2",
        user_id,
        device_id
    )
    .fetch_one(conn)
    .await
}

pub async fn count_pending(
    conn: &mut SqliteConnection,
    user_id: i64,
    device_id: &[u8],
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT COUNT(*) FROM private_pending WHERE user_id = $1 AND device_id = $2",
        user_id,
        device_id
    )
    .fetch_one(conn)
    .await
}

/// The member's device ids, oldest first.
pub async fn device_ids(
    conn: &mut SqliteConnection,
    user_id: i64,
) -> Result<Vec<Vec<u8>>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT device_id FROM private_devices WHERE user_id = $1 ORDER BY created_at, rowid",
        user_id
    )
    .fetch_all(conn)
    .await
}

/// Fails on a foreign key when the device is gone.
pub async fn insert_pending(
    conn: &mut SqliteConnection,
    id: i64,
    user_id: i64,
    device_id: &[u8],
    event: &[u8],
    friend_request_id: Option<i64>,
    now_ms: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO private_pending (id, user_id, device_id, event, friend_request_id, created_at)
         VALUES ($1, $2, $3, $4, $5, $6)",
        id,
        user_id,
        device_id,
        event,
        friend_request_id,
        now_ms
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Deletes the device's oldest events beyond `keep`.
pub async fn trim_pending(
    conn: &mut SqliteConnection,
    user_id: i64,
    device_id: &[u8],
    keep: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM private_pending WHERE id IN (
             SELECT id FROM private_pending
             WHERE user_id = $1 AND device_id = $2
             ORDER BY id DESC
             LIMIT -1 OFFSET $3)",
        user_id,
        device_id,
        keep
    )
    .execute(conn)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRow {
    pub id: i64,
    /// An encoded `PendingEvent`.
    pub event: Vec<u8>,
}

/// The device's events after `after_id`, oldest first, at most `limit`.
pub async fn pending_after(
    conn: &mut SqliteConnection,
    user_id: i64,
    device_id: &[u8],
    after_id: i64,
    limit: i64,
) -> Result<Vec<PendingRow>, sqlx::Error> {
    sqlx::query_as!(
        PendingRow,
        "SELECT id, event FROM private_pending
         WHERE user_id = $1 AND device_id = $2 AND id > $3
         ORDER BY id
         LIMIT $4",
        user_id,
        device_id,
        after_id,
        limit
    )
    .fetch_all(conn)
    .await
}

/// Deletes those of `ids` that wait for this device; `ids` is a JSON array.
pub async fn delete_pending(
    conn: &mut SqliteConnection,
    user_id: i64,
    device_id: &[u8],
    ids: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM private_pending
         WHERE user_id = $1 AND device_id = $2
           AND id IN (SELECT value FROM json_each($3))",
        user_id,
        device_id,
        ids
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn delete_pending_before(
    conn: &mut SqliteConnection,
    before_ms: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM private_pending WHERE created_at < $1",
        before_ms
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Deletes the devices last seen before `before_ms`, with their keys and
/// events. Returns them as (user id, device id).
pub async fn delete_devices_unseen_since(
    conn: &mut SqliteConnection,
    before_ms: i64,
) -> Result<Vec<(i64, Vec<u8>)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"DELETE FROM private_devices WHERE last_seen_at < $1
           RETURNING user_id AS "user_id!", device_id"#,
        before_ms
    )
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.user_id, row.device_id))
        .collect())
}

/// When the device sent this send id, if it did within the last day.
pub async fn recent_send(
    conn: &mut SqliteConnection,
    user_id: i64,
    device_id: &[u8],
    send_id: &[u8],
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT created_at FROM private_recent_sends
         WHERE user_id = $1 AND device_id = $2 AND send_id = $3",
        user_id,
        device_id,
        send_id
    )
    .fetch_optional(conn)
    .await
}

/// Records a send id; `false` when it is already recorded.
pub async fn insert_recent_send(
    conn: &mut SqliteConnection,
    user_id: i64,
    device_id: &[u8],
    send_id: &[u8],
    now_ms: i64,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "INSERT INTO private_recent_sends (user_id, device_id, send_id, created_at)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (user_id, device_id, send_id) DO NOTHING",
        user_id,
        device_id,
        send_id,
        now_ms
    )
    .execute(conn)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn delete_recent_sends_before(
    conn: &mut SqliteConnection,
    before_ms: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM private_recent_sends WHERE created_at < $1",
        before_ms
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Deletes the member's devices (with their keys and events), settings,
/// own blocks, and friend requests to or from them with the events that
/// carry those requests. Run it in a transaction.
pub async fn forget_member(conn: &mut SqliteConnection, user_id: i64) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM private_pending WHERE friend_request_id IN (
             SELECT id FROM private_friend_requests WHERE from_id = $1 OR to_id = $1)",
        user_id
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "DELETE FROM private_friend_requests WHERE from_id = $1 OR to_id = $1",
        user_id
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!("DELETE FROM private_devices WHERE user_id = $1", user_id)
        .execute(&mut *conn)
        .await?;
    sqlx::query!("DELETE FROM private_privacy WHERE user_id = $1", user_id)
        .execute(&mut *conn)
        .await?;
    sqlx::query!("DELETE FROM private_blocks WHERE owner_id = $1", user_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Everything stored for the member. Intended for tests.
pub async fn counts(
    conn: &mut SqliteConnection,
    user_id: i64,
) -> Result<PrivateCounts, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT
               (SELECT COUNT(*) FROM private_devices WHERE user_id = $1) AS "devices!: i64",
               (SELECT COUNT(*) FROM private_one_time_keys WHERE user_id = $1)
                   AS "one_time_keys!: i64",
               (SELECT COUNT(*) FROM private_pending WHERE user_id = $1) AS "pending!: i64",
               (SELECT COUNT(*) FROM private_friend_requests
                WHERE from_id = $1 OR to_id = $1) AS "friend_requests!: i64""#,
        user_id
    )
    .fetch_one(conn)
    .await?;
    Ok(PrivateCounts {
        devices: row.devices,
        one_time_keys: row.one_time_keys,
        pending: row.pending,
        friend_requests: row.friend_requests,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FriendRequestRow {
    pub id: i64,
    pub from_id: i64,
    pub to_id: i64,
    pub created_at: i64,
}

pub async fn outgoing_friend_request_count(
    conn: &mut SqliteConnection,
    from_id: i64,
) -> Result<i64, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT COUNT(*) AS "count!: i64" FROM private_friend_requests WHERE from_id = $1"#,
        from_id
    )
    .fetch_one(conn)
    .await?;
    Ok(row.count)
}

pub async fn friend_request(
    conn: &mut SqliteConnection,
    id: i64,
) -> Result<Option<FriendRequestRow>, sqlx::Error> {
    sqlx::query_as!(
        FriendRequestRow,
        "SELECT id, from_id, to_id, created_at FROM private_friend_requests WHERE id = $1",
        id
    )
    .fetch_optional(conn)
    .await
}

pub async fn friend_request_between(
    conn: &mut SqliteConnection,
    a: i64,
    b: i64,
) -> Result<Option<FriendRequestRow>, sqlx::Error> {
    sqlx::query_as!(
        FriendRequestRow,
        "SELECT id, from_id, to_id, created_at FROM private_friend_requests
         WHERE (from_id = $1 AND to_id = $2) OR (from_id = $2 AND to_id = $1)",
        a,
        b
    )
    .fetch_optional(conn)
    .await
}

pub async fn insert_friend_request(
    conn: &mut SqliteConnection,
    id: i64,
    from_id: i64,
    to_id: i64,
    now_ms: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO private_friend_requests (id, from_id, to_id, created_at)
         VALUES ($1, $2, $3, $4)",
        id,
        from_id,
        to_id,
        now_ms
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn delete_friend_request_by_target(
    conn: &mut SqliteConnection,
    id: i64,
    to_id: i64,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "DELETE FROM private_friend_requests WHERE id = $1 AND to_id = $2",
        id,
        to_id
    )
    .execute(conn)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn delete_friend_request_by_requester(
    conn: &mut SqliteConnection,
    id: i64,
    from_id: i64,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "DELETE FROM private_friend_requests WHERE id = $1 AND from_id = $2",
        id,
        from_id
    )
    .execute(conn)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn delete_pending_by_friend_request(
    conn: &mut SqliteConnection,
    friend_request_id: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM private_pending WHERE friend_request_id = $1",
        friend_request_id
    )
    .execute(conn)
    .await?;
    Ok(())
}
