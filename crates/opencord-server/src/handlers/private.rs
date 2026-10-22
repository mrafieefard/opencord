//! Gateway handlers for private messages: devices and their keys, the
//! events waiting for them, privacy settings and blocks (design 2026-10-22).

use std::collections::HashSet;

use opencord_common::private::{
    self as rules, DEVICE_ID_LEN, KEY_LEN, MAX_ACK_IDS, MAX_BLOCKED_IDS, MAX_CIPHERTEXT_BYTES,
    MAX_CIPHERTEXTS, MAX_CLAIM_DEVICES, MAX_DEVICES_PER_USER, MAX_FETCH_DEVICES_USERS,
    MAX_STORED_ONE_TIME_KEYS, PENDING_PAGE_BYTES, PENDING_PAGE_EVENTS, SEND_ID_LEN, SIGNATURE_LEN,
};
use opencord_proto::v1 as proto;
use proto::event::Kind as Event;
use proto::response::Result as Response;
use sqlx::SqliteConnection;

use super::{Ctx, ack};
use crate::db::private::{self as db, DeviceRow, PrivacyRow};
use crate::error::ApiError;
use crate::private::{Pending, Privacy, devices_of, insert_pending, push_pending};
use crate::state::{Audience, now_ms};

/// The session's device, or `PRIVATE_MESSAGES_UNAVAILABLE` when it has none
/// (or private messages are off).
pub(crate) fn require_device(ctx: &Ctx<'_>) -> Result<[u8; 16], ApiError> {
    let unregistered = || ApiError::private_messages_unavailable("register this device first");
    if !ctx.state.private.enabled() {
        return Err(unregistered());
    }
    ctx.session.private_device().ok_or_else(unregistered)
}

pub async fn register_device(
    ctx: &Ctx<'_>,
    request: proto::RegisterDevice,
) -> Result<Response, ApiError> {
    require_enabled(ctx)?;
    // 1. Shape.
    let keys = request
        .device
        .ok_or_else(|| ApiError::invalid_argument("the device's keys are missing"))?;
    let device_id = check_device_keys(&keys)?;
    let fallback = request
        .fallback_key
        .ok_or_else(|| ApiError::invalid_argument("the fallback key is missing"))?;
    check_prekey(&fallback)?;
    check_one_time_keys(&request.one_time_keys)?;
    // 2. One device per session.
    if ctx
        .session
        .private_device()
        .is_some_and(|held| held != device_id)
    {
        return Err(ApiError::conflict(
            "this connection already registered another device",
        ));
    }
    // 3. Signed by the caller's identity key.
    let identity = {
        let guild = ctx.state.guild();
        guild
            .members
            .get(&ctx.user_id)
            .map(|member| member.user.public_key.clone())
            .ok_or_else(|| ApiError::not_found("member"))?
    };
    if !rules::verify_device(
        &identity,
        &keys.device_id,
        &keys.curve25519,
        &keys.ed25519,
        &keys.signature,
    ) {
        return Err(ApiError::invalid_argument(
            "the device's signature does not match your identity",
        ));
    }
    // 4. Every prekey signed by the device.
    verify_prekeys(
        &keys.ed25519,
        &keys.device_id,
        &request.one_time_keys,
        Some(&fallback),
    )?;
    // 5. Store it, its keys, and drop the devices beyond the limit.
    let device = DeviceRow {
        device_id: keys.device_id,
        curve25519: keys.curve25519,
        ed25519: keys.ed25519,
        signature: keys.signature,
        fallback_key: fallback.key,
        fallback_signature: fallback.signature,
    };
    let mut tx = ctx.state.db.begin().await?;
    db::delete_one_time_keys_if_replaced(&mut tx, ctx.user_id, &device).await?;
    db::upsert_device(&mut tx, ctx.user_id, &device, now_ms()).await?;
    for key in &request.one_time_keys {
        db::insert_one_time_key(
            &mut tx,
            ctx.user_id,
            &device.device_id,
            &key.key,
            &key.signature,
        )
        .await?;
    }
    db::trim_one_time_keys(
        &mut tx,
        ctx.user_id,
        &device.device_id,
        to_i64(MAX_STORED_ONE_TIME_KEYS),
    )
    .await?;
    let removed = db::delete_devices_beyond(
        &mut tx,
        ctx.user_id,
        &device.device_id,
        to_i64(MAX_DEVICES_PER_USER - 1),
    )
    .await?;
    tx.commit().await?;
    // 6. This session alone holds the device; none holds a removed one.
    for session in ctx.state.sessions.for_user(ctx.user_id) {
        let unbind = session.id != ctx.session.id
            && session.private_device().is_some_and(|held| {
                held == device_id || removed.iter().any(|gone| gone.as_slice() == held)
            });
        if unbind {
            session.set_private_device(None);
        }
    }
    ctx.session.set_private_device(Some(device_id));
    // 7.
    registration(ctx, &device_id).await
}

pub async fn upload_keys(ctx: &Ctx<'_>, request: proto::UploadKeys) -> Result<Response, ApiError> {
    require_enabled(ctx)?;
    // 1.
    let device_id = require_device(ctx)?;
    // 2. Shape.
    if let Some(fallback) = &request.fallback_key {
        check_prekey(fallback)?;
    }
    check_one_time_keys(&request.one_time_keys)?;
    // 3. Signed by the device.
    let device = db::device(&mut *ctx.state.db.acquire().await?, ctx.user_id, &device_id).await?;
    let Some(device) = device else {
        return Err(unbind_removed_device(ctx));
    };
    verify_prekeys(
        &device.ed25519,
        &device_id,
        &request.one_time_keys,
        request.fallback_key.as_ref(),
    )?;
    // 4–5. Touching it first takes the write lock, so the count below
    // holds until the commit.
    let mut tx = ctx.state.db.begin().await?;
    if !db::touch_device(&mut tx, ctx.user_id, &device_id, now_ms()).await? {
        return Err(unbind_removed_device(ctx));
    }
    for key in &request.one_time_keys {
        db::insert_one_time_key(&mut tx, ctx.user_id, &device_id, &key.key, &key.signature).await?;
    }
    if let Some(fallback) = &request.fallback_key {
        db::set_fallback_key(
            &mut tx,
            ctx.user_id,
            &device_id,
            &fallback.key,
            &fallback.signature,
        )
        .await?;
    }
    let stored = db::count_one_time_keys(&mut tx, ctx.user_id, &device_id).await?;
    if stored > to_i64(MAX_STORED_ONE_TIME_KEYS) {
        // Dropping the transaction rolls it back.
        return Err(ApiError::invalid_argument("too many one-time keys"));
    }
    tx.commit().await?;
    // 6.
    registration(ctx, &device_id).await
}

pub async fn unregister_device(ctx: &Ctx<'_>) -> Result<Response, ApiError> {
    require_enabled(ctx)?;
    let Some(device_id) = ctx.session.private_device() else {
        return Ok(ack());
    };
    // Its keys and waiting events go with it.
    db::delete_device(&mut *ctx.state.db.acquire().await?, ctx.user_id, &device_id).await?;
    ctx.session.set_private_device(None);
    Ok(ack())
}

pub async fn fetch_devices(
    ctx: &Ctx<'_>,
    request: proto::FetchDevices,
) -> Result<Response, ApiError> {
    require_enabled(ctx)?;
    // 1.
    require_device(ctx)?;
    // 2.
    if request.user_ids.is_empty() || request.user_ids.len() > MAX_FETCH_DEVICES_USERS {
        return Err(ApiError::invalid_argument(
            "fetch the devices of one or two members",
        ));
    }
    require_members(ctx, request.user_ids.iter().copied())?;
    // 3.
    let mut conn = ctx.state.db.acquire().await?;
    let mut users = Vec::with_capacity(request.user_ids.len());
    for user_id in request.user_ids {
        let devices = db::devices(&mut conn, user_id).await?;
        users.push(proto::UserDevices {
            user_id,
            devices: devices.into_iter().map(device_keys).collect(),
        });
    }
    Ok(Response::DeviceLists(proto::DeviceLists { users }))
}

pub async fn claim_keys(ctx: &Ctx<'_>, request: proto::ClaimKeys) -> Result<Response, ApiError> {
    require_enabled(ctx)?;
    // 1.
    require_device(ctx)?;
    // 2.
    ctx.state
        .rate_limits
        .check_key_claim(ctx.user_id)
        .map_err(ApiError::rate_limited)?;
    // 3.
    if request.devices.is_empty() || request.devices.len() > MAX_CLAIM_DEVICES {
        return Err(ApiError::invalid_argument(format!(
            "claim keys for 1 to {MAX_CLAIM_DEVICES} devices"
        )));
    }
    require_members(ctx, request.devices.iter().map(|device| device.user_id))?;
    // 4. A one-time key each, else the fallback key; unknown devices are
    // left out.
    let mut conn = ctx.state.db.acquire().await?;
    let mut bundles = Vec::with_capacity(request.devices.len());
    for wanted in request.devices {
        let Some(device) = db::device(&mut conn, wanted.user_id, &wanted.device_id).await? else {
            continue;
        };
        let claimed = db::claim_one_time_key(&mut conn, wanted.user_id, &wanted.device_id).await?;
        let (prekey, fallback) = match claimed {
            Some((key, signature)) => (proto::Prekey { key, signature }, false),
            None => (
                proto::Prekey {
                    key: device.fallback_key.clone(),
                    signature: device.fallback_signature.clone(),
                },
                true,
            ),
        };
        bundles.push(proto::DeviceBundle {
            user_id: wanted.user_id,
            device: Some(device_keys(device)),
            prekey: Some(prekey),
            fallback,
        });
    }
    Ok(Response::ClaimedKeys(proto::ClaimedKeys { bundles }))
}

pub async fn fetch_pending_events(
    ctx: &Ctx<'_>,
    request: proto::FetchPendingEvents,
) -> Result<Response, ApiError> {
    require_enabled(ctx)?;
    // 1.
    let device_id = require_device(ctx)?;
    // 2.
    let limit = match request.limit {
        0 => PENDING_PAGE_EVENTS,
        limit => limit.min(PENDING_PAGE_EVENTS),
    };
    // 3. One more than the page, to tell whether more wait.
    let mut conn = ctx.state.db.acquire().await?;
    let rows = db::pending_after(
        &mut conn,
        ctx.user_id,
        &device_id,
        request.after_id,
        i64::from(limit) + 1,
    )
    .await?;
    // 4. Within the byte budget, but never empty.
    let mut bytes = 0;
    let mut taken = 0;
    for row in rows.iter().take(limit as usize) {
        bytes += row.event.len();
        if taken > 0 && bytes > PENDING_PAGE_BYTES {
            break;
        }
        taken += 1;
    }
    // 5.
    let more = rows.len() > taken;
    // 6.
    if !db::touch_device(&mut conn, ctx.user_id, &device_id, now_ms()).await? {
        return Err(unbind_removed_device(ctx));
    }
    let events = rows[..taken]
        .iter()
        .map(|row| crate::private::decode_pending(&row.event))
        .collect::<Result<_, _>>()?;
    Ok(Response::PendingEvents(proto::PendingEvents {
        events,
        more,
    }))
}

pub async fn ack_pending_events(
    ctx: &Ctx<'_>,
    request: proto::AckPendingEvents,
) -> Result<Response, ApiError> {
    require_enabled(ctx)?;
    // 1.
    let device_id = require_device(ctx)?;
    // 2.
    if request.ids.len() > MAX_ACK_IDS {
        return Err(ApiError::invalid_argument(format!(
            "acknowledge at most {MAX_ACK_IDS} events at once"
        )));
    }
    // 3. Only this device's.
    let ids = serde_json::to_string(&request.ids).map_err(|error| {
        tracing::error!(%error, "could not encode the acknowledged ids");
        ApiError::internal()
    })?;
    let mut conn = ctx.state.db.acquire().await?;
    db::delete_pending(&mut conn, ctx.user_id, &device_id, &ids).await?;
    // 4.
    if !db::touch_device(&mut conn, ctx.user_id, &device_id, now_ms()).await? {
        return Err(unbind_removed_device(ctx));
    }
    Ok(ack())
}

pub async fn send_private_message(
    ctx: &Ctx<'_>,
    request: proto::SendPrivateMessage,
) -> Result<Response, ApiError> {
    require_enabled(ctx)?;
    // 1.
    let device_id = require_device(ctx)?;
    let recipient_id = request.recipient_id;
    // 2.
    ctx.state
        .rate_limits
        .check_private_message(ctx.user_id, recipient_id)
        .map_err(ApiError::rate_limited)?;
    // 3.
    let people = people(ctx, recipient_id)?;
    // 4–5.
    may_reach(ctx, recipient_id, &people, &request.slip)?;
    // 6.
    let targets = check_copies(&request)?;
    // 7. A repeat stores nothing and answers the first time.
    let mut conn = ctx.state.db.acquire().await?;
    if let Some(sent_at_ms) =
        db::recent_send(&mut conn, ctx.user_id, &device_id, &request.send_id).await?
    {
        return Ok(sent(sent_at_ms));
    }
    // 8.
    check_targets(&mut conn, ctx.user_id, recipient_id, &device_id, &targets).await?;
    drop(conn);
    // 9. Recording the send id first takes the write lock, so a repeat
    // sent meanwhile is seen, and the devices checked again below can't
    // change before the commit.
    let now = now_ms();
    let items = copies_to_deliver(
        request.ciphertexts,
        &targets,
        &people.caller,
        &device_id,
        now,
    );
    let mut tx = ctx.state.db.begin().await?;
    if !db::insert_recent_send(&mut tx, ctx.user_id, &device_id, &request.send_id, now).await? {
        let sent_at_ms = db::recent_send(&mut tx, ctx.user_id, &device_id, &request.send_id)
            .await?
            .ok_or_else(ApiError::internal)?;
        return Ok(sent(sent_at_ms));
    }
    check_targets(&mut tx, ctx.user_id, recipient_id, &device_id, &targets).await?;
    let pushed = match insert_pending(&mut tx, ctx.state, items, now).await {
        Ok(pushed) => pushed,
        Err(sqlx::Error::Database(error)) if error.is_foreign_key_violation() => {
            return Err(ApiError::device_list_changed());
        }
        Err(error) => return Err(error.into()),
    };
    tx.commit().await?;
    push_pending(ctx.state, &pushed);
    Ok(sent(now))
}

pub fn start_private_typing(
    ctx: &Ctx<'_>,
    request: &proto::StartPrivateTyping,
) -> Result<Response, ApiError> {
    require_enabled(ctx)?;
    // 1.
    let people = people(ctx, request.recipient_id)?;
    // 2. A refusal looks like success, so typing never reveals a block.
    if may_reach(ctx, request.recipient_id, &people, &request.slip).is_ok() {
        // 3.
        ctx.state.broadcast(
            Event::PrivateTypingStart(proto::PrivateTypingStart {
                user_id: ctx.user_id,
            }),
            Audience::User(request.recipient_id),
        );
    }
    Ok(ack())
}

/// The caller, and the person they write to.
struct People {
    caller: proto::User,
    recipient_key: Vec<u8>,
}

/// The recipient is a member and not the caller.
fn people(ctx: &Ctx<'_>, recipient_id: i64) -> Result<People, ApiError> {
    let guild = ctx.state.guild();
    let recipient = guild
        .members
        .get(&recipient_id)
        .ok_or_else(|| ApiError::not_found("member"))?;
    if recipient_id == ctx.user_id {
        return Err(ApiError::invalid_argument("you can't message yourself"));
    }
    let caller = guild
        .members
        .get(&ctx.user_id)
        .ok_or_else(|| ApiError::not_found("member"))?;
    Ok(People {
        caller: caller.user.to_proto(),
        recipient_key: recipient.user.public_key.clone(),
    })
}

/// The recipient hasn't blocked the caller, and takes messages from them:
/// from anyone, or with the recipient's permission slip for the caller.
fn may_reach(
    ctx: &Ctx<'_>,
    recipient_id: i64,
    people: &People,
    slip: &[u8],
) -> Result<(), ApiError> {
    let private = &ctx.state.private;
    if private.has_blocked(recipient_id, ctx.user_id) {
        return Err(ApiError::undeliverable());
    }
    let open = private.privacy(recipient_id).allow_message_requests;
    if !open && !rules::verify_slip(&people.recipient_key, &people.caller.public_key, slip) {
        return Err(ApiError::message_requests_closed());
    }
    Ok(())
}

/// Each copy's (user, device), in the copies' order, once every copy has
/// its shape and no device has two.
fn check_copies(request: &proto::SendPrivateMessage) -> Result<Vec<(i64, [u8; 16])>, ApiError> {
    check_len(&request.send_id, SEND_ID_LEN, "a send id")?;
    let copies = &request.ciphertexts;
    if copies.is_empty() || copies.len() > MAX_CIPHERTEXTS {
        return Err(ApiError::invalid_argument(format!(
            "a message has 1 to {MAX_CIPHERTEXTS} encrypted copies"
        )));
    }
    let mut targets = Vec::with_capacity(copies.len());
    for copy in copies {
        let device: [u8; 16] = copy.device_id.as_slice().try_into().map_err(|_| {
            ApiError::invalid_argument(format!("a device id must be {DEVICE_ID_LEN} bytes"))
        })?;
        if copy.olm_type > 1 {
            return Err(ApiError::invalid_argument(
                "an encrypted copy's type must be 0 or 1",
            ));
        }
        if copy.body.is_empty() || copy.body.len() > MAX_CIPHERTEXT_BYTES {
            return Err(ApiError::invalid_argument(format!(
                "an encrypted copy must be 1 to {MAX_CIPHERTEXT_BYTES} bytes"
            )));
        }
        if targets.contains(&(copy.user_id, device)) {
            return Err(ApiError::invalid_argument("two copies for one device"));
        }
        targets.push((copy.user_id, device));
    }
    Ok(targets)
}

/// The copies go to exactly the recipient's devices and the caller's
/// other ones.
async fn check_targets(
    conn: &mut SqliteConnection,
    caller: i64,
    recipient: i64,
    device_id: &[u8; 16],
    targets: &[(i64, [u8; 16])],
) -> Result<(), ApiError> {
    let recipient_devices = devices_of(conn, recipient).await?;
    if recipient_devices.is_empty() {
        return Err(ApiError::private_messages_unavailable(
            "they can't get private messages on this server",
        ));
    }
    let own_devices = devices_of(conn, caller).await?;
    let expected: HashSet<(i64, [u8; 16])> = recipient_devices
        .into_iter()
        .map(|device| (recipient, device))
        .chain(
            own_devices
                .into_iter()
                .filter(|device| device != device_id)
                .map(|device| (caller, device)),
        )
        .collect();
    let given: HashSet<(i64, [u8; 16])> = targets.iter().copied().collect();
    if given == expected {
        Ok(())
    } else {
        Err(ApiError::device_list_changed())
    }
}

/// One `PrivateMessage` per copy, for the device it was encrypted for.
fn copies_to_deliver(
    copies: Vec<proto::DeviceCiphertext>,
    targets: &[(i64, [u8; 16])],
    sender: &proto::User,
    sender_device: &[u8; 16],
    sent_at_ms: i64,
) -> Vec<Pending> {
    copies
        .into_iter()
        .zip(targets.iter().copied())
        .map(|(copy, (user_id, device_id))| Pending {
            user_id,
            device_id,
            kind: proto::pending_event::Kind::PrivateMessage(proto::PrivateMessage {
                sender: Some(sender.clone()),
                sender_device_id: sender_device.to_vec(),
                olm_type: copy.olm_type,
                body: copy.body,
                sent_at_ms,
            }),
            friend_request_id: None,
        })
        .collect()
}

fn sent(sent_at_ms: i64) -> Response {
    Response::PrivateMessageSent(proto::PrivateMessageSent { sent_at_ms })
}

pub async fn update_privacy(
    ctx: &Ctx<'_>,
    request: proto::UpdatePrivacy,
) -> Result<Response, ApiError> {
    require_enabled(ctx)?;
    // 1.
    if request.blocked_user_ids.len() > MAX_BLOCKED_IDS {
        return Err(ApiError::invalid_argument(format!(
            "at most {MAX_BLOCKED_IDS} members can be blocked"
        )));
    }
    // In step with kicks and bans, which forget a member's settings, and
    // with this member's other sessions.
    let _writes = ctx.state.write_lock().await;
    // 2. Members only, never the caller.
    let blocked: HashSet<i64> = {
        let guild = ctx.state.guild();
        if !guild.members.contains_key(&ctx.user_id) {
            return Err(ApiError::not_found("member"));
        }
        request
            .blocked_user_ids
            .iter()
            .copied()
            .filter(|id| *id != ctx.user_id && guild.members.contains_key(id))
            .collect()
    };
    // 3.
    let row = PrivacyRow {
        user_id: ctx.user_id,
        allow_friend_requests: request.allow_friend_requests,
        allow_message_requests: request.allow_message_requests,
    };
    let blocked_ids: Vec<i64> = blocked.iter().copied().collect();
    let mut tx = ctx.state.db.begin().await?;
    db::save_privacy(&mut tx, &row, &blocked_ids, now_ms()).await?;
    tx.commit().await?;
    // 4.
    ctx.state.private.set_privacy(
        ctx.user_id,
        Privacy {
            allow_friend_requests: row.allow_friend_requests,
            allow_message_requests: row.allow_message_requests,
            blocked,
        },
    );
    Ok(ack())
}

fn require_enabled(ctx: &Ctx<'_>) -> Result<(), ApiError> {
    if ctx.state.private.enabled() {
        Ok(())
    } else {
        Err(ApiError::private_messages_unavailable(
            "private messages are turned off on this server",
        ))
    }
}

fn require_members(ctx: &Ctx<'_>, mut user_ids: impl Iterator<Item = i64>) -> Result<(), ApiError> {
    let guild = ctx.state.guild();
    if user_ids.all(|user_id| guild.members.contains_key(&user_id)) {
        Ok(())
    } else {
        Err(ApiError::not_found("member"))
    }
}

/// The session's device was removed (pushed out by newer ones): the
/// session no longer holds it.
fn unbind_removed_device(ctx: &Ctx<'_>) -> ApiError {
    ctx.session.set_private_device(None);
    ApiError::private_messages_unavailable("register this device first")
}

/// The device's counts, read after its changes were committed.
async fn registration(ctx: &Ctx<'_>, device_id: &[u8]) -> Result<Response, ApiError> {
    let mut conn = ctx.state.db.acquire().await?;
    let one_time_keys = db::count_one_time_keys(&mut conn, ctx.user_id, device_id).await?;
    let pending = db::count_pending(&mut conn, ctx.user_id, device_id).await?;
    Ok(Response::DeviceRegistration(proto::DeviceRegistration {
        one_time_keys_left: u32::try_from(one_time_keys).unwrap_or(u32::MAX),
        pending_events: u32::try_from(pending).unwrap_or(u32::MAX),
    }))
}

fn device_keys(device: DeviceRow) -> proto::DeviceKeys {
    proto::DeviceKeys {
        device_id: device.device_id,
        curve25519: device.curve25519,
        ed25519: device.ed25519,
        signature: device.signature,
    }
}

/// The device's id, once every field has its length.
fn check_device_keys(keys: &proto::DeviceKeys) -> Result<[u8; 16], ApiError> {
    check_len(&keys.curve25519, KEY_LEN, "a device's Curve25519 key")?;
    check_len(&keys.ed25519, KEY_LEN, "a device's Ed25519 key")?;
    check_len(&keys.signature, SIGNATURE_LEN, "a device's signature")?;
    keys.device_id.as_slice().try_into().map_err(|_| {
        ApiError::invalid_argument(format!("a device id must be {DEVICE_ID_LEN} bytes"))
    })
}

fn check_prekey(prekey: &proto::Prekey) -> Result<(), ApiError> {
    check_len(&prekey.key, KEY_LEN, "a prekey")?;
    check_len(&prekey.signature, SIGNATURE_LEN, "a prekey's signature")
}

fn check_one_time_keys(keys: &[proto::Prekey]) -> Result<(), ApiError> {
    if keys.len() > MAX_STORED_ONE_TIME_KEYS {
        return Err(ApiError::invalid_argument(format!(
            "at most {MAX_STORED_ONE_TIME_KEYS} one-time keys at once"
        )));
    }
    keys.iter().try_for_each(check_prekey)
}

fn check_len(bytes: &[u8], len: usize, what: &str) -> Result<(), ApiError> {
    if bytes.len() == len {
        Ok(())
    } else {
        Err(ApiError::invalid_argument(format!(
            "{what} must be {len} bytes"
        )))
    }
}

/// Each one-time key and the fallback key, if given, signed by the device's
/// Ed25519 key as what it is.
fn verify_prekeys(
    ed25519: &[u8],
    device_id: &[u8],
    one_time_keys: &[proto::Prekey],
    fallback: Option<&proto::Prekey>,
) -> Result<(), ApiError> {
    let signed = one_time_keys
        .iter()
        .map(|prekey| (prekey, false))
        .chain(fallback.map(|prekey| (prekey, true)))
        .all(|(prekey, is_fallback)| {
            rules::verify_prekey(
                ed25519,
                device_id,
                &prekey.key,
                is_fallback,
                &prekey.signature,
            )
        });
    if signed {
        Ok(())
    } else {
        Err(ApiError::invalid_argument(
            "a prekey's signature does not match the device",
        ))
    }
}

fn to_i64(count: usize) -> i64 {
    i64::try_from(count).unwrap_or(i64::MAX)
}
