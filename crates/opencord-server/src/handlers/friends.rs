//! Gateway handlers for friend requests, answers, cancellations and removals
//! (design 2026-10-22).

use opencord_common::private::verify_slip;
use opencord_proto::v1 as proto;
use proto::response::Result as Response;

use super::private::require_device;
use super::{Ctx, ack};
use crate::db::private as db;
use crate::db::users;
use crate::error::ApiError;
use crate::private::{Pending, deliver, devices_of, insert_pending, push_pending};
use crate::state::now_ms;

pub async fn send_friend_request(
    ctx: &Ctx<'_>,
    request: proto::SendFriendRequest,
) -> Result<Response, ApiError> {
    // 1. require_device.
    let caller_device = require_device(ctx)?;
    // 2. check_friend_request(caller).
    ctx.state
        .rate_limits
        .check_friend_request(ctx.user_id)
        .map_err(ApiError::rate_limited)?;
    // 3. The target is a member, and not the caller.
    let target_id = request.user_id;
    if target_id == ctx.user_id {
        return Err(ApiError::invalid_argument(
            "you can't send a friend request to yourself",
        ));
    }
    let (caller_user, target_user) = {
        let guild = ctx.state.guild();
        let target_member = guild
            .members
            .get(&target_id)
            .ok_or_else(|| ApiError::not_found("member"))?;
        let caller_member = guild
            .members
            .get(&ctx.user_id)
            .ok_or_else(|| ApiError::not_found("member"))?;
        (caller_member.user.to_proto(), target_member.user.to_proto())
    };
    // 4. has_blocked(target, caller) -> undeliverable().
    if ctx.state.private.has_blocked(target_id, ctx.user_id) {
        return Err(ApiError::undeliverable());
    }
    // 5. The target doesn't allow friend requests -> friend_requests_closed().
    if !ctx.state.private.privacy(target_id).allow_friend_requests {
        return Err(ApiError::friend_requests_closed());
    }
    // 6. The target has no devices -> private_messages_unavailable("they can't get friend requests on this server").
    let mut conn = ctx.state.db.acquire().await?;
    let target_devices = devices_of(&mut conn, target_id).await?;
    if target_devices.is_empty() {
        return Err(ApiError::private_messages_unavailable(
            "they can't get friend requests on this server",
        ));
    }
    let caller_devices = devices_of(&mut conn, ctx.user_id).await?;
    // 7. verify_slip(caller's key, target's key, slip) fails -> INVALID_ARGUMENT "the permission slip is not valid".
    if !verify_slip(
        &caller_user.public_key,
        &target_user.public_key,
        &request.slip,
    ) {
        return Err(ApiError::invalid_argument(
            "the permission slip is not valid",
        ));
    }
    // 8. Read:
    // the caller's outgoing count >= 100 -> INVALID_ARGUMENT "you have too many friend requests waiting".
    let outgoing_count = db::outgoing_friend_request_count(&mut conn, ctx.user_id).await?;
    if outgoing_count >= 100 {
        return Err(ApiError::invalid_argument(
            "you have too many friend requests waiting",
        ));
    }
    // A request between the pair exists:
    // - from the caller -> CONFLICT "you already sent them a friend request";
    // - from the target -> CONFLICT "they already sent you a friend request".
    if let Some(existing) = db::friend_request_between(&mut conn, ctx.user_id, target_id).await? {
        if existing.from_id == ctx.user_id {
            return Err(ApiError::conflict("you already sent them a friend request"));
        } else {
            return Err(ApiError::conflict("they already sent you a friend request"));
        }
    }
    // 9. One transaction:
    // - insert the request (Snowflake id);
    // - insert_pending: FriendRequestReceived { request_id, from: caller, slip } to the target's devices, with friend_request_id set;
    //   FriendRequestSent { request_id, to: target } to the caller's other devices.
    let request_id = ctx.state.ids.next_id();
    let now = now_ms();
    let mut tx = ctx.state.db.begin().await?;
    if let Err(err) =
        db::insert_friend_request(&mut tx, request_id, ctx.user_id, target_id, now).await
    {
        if let sqlx::Error::Database(ref db_err) = err
            && db_err.is_unique_violation()
        {
            return Err(ApiError::conflict("you already sent them a friend request"));
        }
        return Err(err.into());
    }

    let mut pending_items = Vec::new();
    for dev in target_devices {
        pending_items.push(Pending {
            user_id: target_id,
            device_id: dev,
            kind: proto::pending_event::Kind::FriendRequestReceived(proto::FriendRequestReceived {
                request_id,
                from: Some(caller_user.clone()),
                slip: request.slip.clone(),
            }),
            friend_request_id: Some(request_id),
        });
    }
    for dev in caller_devices {
        if dev != caller_device {
            pending_items.push(Pending {
                user_id: ctx.user_id,
                device_id: dev,
                kind: proto::pending_event::Kind::FriendRequestSent(proto::FriendRequestSent {
                    request_id,
                    to: Some(target_user.clone()),
                }),
                friend_request_id: None,
            });
        }
    }

    let pushed = insert_pending(&mut tx, ctx.state, pending_items, now).await?;
    tx.commit().await?;
    push_pending(ctx.state, &pushed);

    Ok(Response::FriendRequestCreated(
        proto::FriendRequestCreated {
            request_id,
            created_at_ms: now,
        },
    ))
}

pub async fn answer_friend_request(
    ctx: &Ctx<'_>,
    request: proto::AnswerFriendRequest,
) -> Result<Response, ApiError> {
    // 1. require_device.
    let caller_device = require_device(ctx)?;
    // 2. Read the request: missing, or the caller isn't its target -> NOT_FOUND "friend request".
    let mut conn = ctx.state.db.acquire().await?;
    let req = db::friend_request(&mut conn, request.request_id)
        .await?
        .ok_or_else(|| ApiError::not_found("friend request"))?;
    if req.to_id != ctx.user_id {
        return Err(ApiError::not_found("friend request"));
    }
    let requester_id = req.from_id;
    let (caller_user, requester_opt) = {
        let guild = ctx.state.guild();
        let caller = guild
            .members
            .get(&ctx.user_id)
            .ok_or_else(|| ApiError::not_found("member"))?
            .user
            .to_proto();
        let requester = guild.members.get(&requester_id).map(|m| m.user.to_proto());
        (caller, requester)
    };
    let requester_user = match requester_opt {
        Some(u) => u,
        None => {
            let u = users::find(&mut conn, requester_id)
                .await?
                .ok_or_else(|| ApiError::not_found("member"))?;
            proto::User {
                id: u.id,
                public_key: u.public_key,
                display_name: u.display_name,
                avatar_hash: u.avatar_hash.unwrap_or_default(),
            }
        }
    };
    // 3. An accept with a failing verify_slip(caller, requester, slip) -> INVALID_ARGUMENT.
    if request.accept
        && !verify_slip(
            &caller_user.public_key,
            &requester_user.public_key,
            &request.slip,
        )
    {
        return Err(ApiError::invalid_argument(
            "the permission slip is not valid",
        ));
    }
    // 4. Transaction: DELETE … WHERE id = ? AND to_id = ?, which returns rows_affected() == 1.
    // When false -> NOT_FOUND, sending nothing.
    let mut tx = ctx.state.db.begin().await?;
    let deleted =
        db::delete_friend_request_by_target(&mut tx, request.request_id, ctx.user_id).await?;
    if !deleted {
        return Err(ApiError::not_found("friend request"));
    }
    // Then insert_pending FriendRequestAnswered { request_id, requester, answerer: caller, accepted, slip (empty on a decline) }
    // to the requester's devices and the caller's other devices.
    let requester_devices = devices_of(&mut tx, requester_id).await?;
    let caller_devices = devices_of(&mut tx, ctx.user_id).await?;
    let now = now_ms();
    let slip = if request.accept {
        request.slip
    } else {
        Vec::new()
    };
    let answered_event = proto::FriendRequestAnswered {
        request_id: request.request_id,
        requester: Some(requester_user),
        answerer: Some(caller_user),
        accepted: request.accept,
        slip,
    };
    let mut items = Vec::new();
    for dev in requester_devices {
        items.push(Pending {
            user_id: requester_id,
            device_id: dev,
            kind: proto::pending_event::Kind::FriendRequestAnswered(answered_event.clone()),
            friend_request_id: None,
        });
    }
    for dev in caller_devices {
        if dev != caller_device {
            items.push(Pending {
                user_id: ctx.user_id,
                device_id: dev,
                kind: proto::pending_event::Kind::FriendRequestAnswered(answered_event.clone()),
                friend_request_id: None,
            });
        }
    }
    let pushed = insert_pending(&mut tx, ctx.state, items, now).await?;
    // Where messages go only between friends, the two are friends from
    // now on, before either app tells the server.
    let befriend = request.accept && ctx.state.private.friends_only();
    if befriend {
        db::add_friends(&mut tx, requester_id, ctx.user_id).await?;
    }
    // 5. Commit, push, ack. A requester who is no longer a member has no devices, so only the caller's other devices hear.
    tx.commit().await?;
    if befriend {
        ctx.state
            .private
            .set_friends(requester_id, ctx.user_id, true);
    }
    push_pending(ctx.state, &pushed);
    Ok(ack())
}

pub async fn cancel_friend_request(
    ctx: &Ctx<'_>,
    request: proto::CancelFriendRequest,
) -> Result<Response, ApiError> {
    // 1. require_device.
    let caller_device = require_device(ctx)?;
    // 2. The request exists and the caller sent it, else NOT_FOUND.
    let mut conn = ctx.state.db.acquire().await?;
    let req = db::friend_request(&mut conn, request.request_id)
        .await?
        .ok_or_else(|| ApiError::not_found("friend request"))?;
    if req.from_id != ctx.user_id {
        return Err(ApiError::not_found("friend request"));
    }
    let target_id = req.to_id;
    let (caller_user, target_opt) = {
        let guild = ctx.state.guild();
        let caller = guild
            .members
            .get(&ctx.user_id)
            .ok_or_else(|| ApiError::not_found("member"))?
            .user
            .to_proto();
        let target = guild.members.get(&target_id).map(|m| m.user.to_proto());
        (caller, target)
    };
    let target_user = match target_opt {
        Some(u) => u,
        None => {
            let u = users::find(&mut conn, target_id)
                .await?
                .ok_or_else(|| ApiError::not_found("member"))?;
            proto::User {
                id: u.id,
                public_key: u.public_key,
                display_name: u.display_name,
                avatar_hash: u.avatar_hash.unwrap_or_default(),
            }
        }
    };
    // 3. Transaction:
    // - DELETE … WHERE id = ? AND from_id = ? (false -> NOT_FOUND);
    // - delete the pending rows with this friend_request_id;
    // - insert_pending FriendRequestCancelled { request_id, requester: caller, target } to the target's devices and the caller's other devices.
    let mut tx = ctx.state.db.begin().await?;
    let deleted =
        db::delete_friend_request_by_requester(&mut tx, request.request_id, ctx.user_id).await?;
    if !deleted {
        return Err(ApiError::not_found("friend request"));
    }
    db::delete_pending_by_friend_request(&mut tx, request.request_id).await?;

    let target_devices = devices_of(&mut tx, target_id).await?;
    let caller_devices = devices_of(&mut tx, ctx.user_id).await?;
    let now = now_ms();
    let cancelled_event = proto::FriendRequestCancelled {
        request_id: request.request_id,
        requester: Some(caller_user),
        target: Some(target_user),
    };
    let mut items = Vec::new();
    for dev in target_devices {
        items.push(Pending {
            user_id: target_id,
            device_id: dev,
            kind: proto::pending_event::Kind::FriendRequestCancelled(cancelled_event.clone()),
            friend_request_id: None,
        });
    }
    for dev in caller_devices {
        if dev != caller_device {
            items.push(Pending {
                user_id: ctx.user_id,
                device_id: dev,
                kind: proto::pending_event::Kind::FriendRequestCancelled(cancelled_event.clone()),
                friend_request_id: None,
            });
        }
    }
    let pushed = insert_pending(&mut tx, ctx.state, items, now).await?;
    // 4. Commit, push, ack.
    tx.commit().await?;
    push_pending(ctx.state, &pushed);
    Ok(ack())
}

pub async fn remove_friend(
    ctx: &Ctx<'_>,
    request: proto::RemoveFriend,
) -> Result<Response, ApiError> {
    // 1. require_device.
    let caller_device = require_device(ctx)?;
    // 2. The target is a member and not the caller.
    let target_id = request.user_id;
    if target_id == ctx.user_id {
        return Err(ApiError::invalid_argument(
            "you can't remove yourself as a friend",
        ));
    }
    let (caller_user, target_user) = {
        let guild = ctx.state.guild();
        let target_member = guild
            .members
            .get(&target_id)
            .ok_or_else(|| ApiError::not_found("member"))?;
        let caller_member = guild
            .members
            .get(&ctx.user_id)
            .ok_or_else(|| ApiError::not_found("member"))?;
        (caller_member.user.to_proto(), target_member.user.to_proto())
    };
    // 3. deliver FriendRemoved { remover: caller, removed: target } to the target's devices and the caller's other devices.
    let mut conn = ctx.state.db.acquire().await?;
    let target_devices = devices_of(&mut conn, target_id).await?;
    let caller_devices = devices_of(&mut conn, ctx.user_id).await?;
    let removed_event = proto::FriendRemoved {
        remover: Some(caller_user),
        removed: Some(target_user),
    };
    let mut items = Vec::new();
    for dev in target_devices {
        items.push(Pending {
            user_id: target_id,
            device_id: dev,
            kind: proto::pending_event::Kind::FriendRemoved(removed_event.clone()),
            friend_request_id: None,
        });
    }
    for dev in caller_devices {
        if dev != caller_device {
            items.push(Pending {
                user_id: ctx.user_id,
                device_id: dev,
                kind: proto::pending_event::Kind::FriendRemoved(removed_event.clone()),
                friend_request_id: None,
            });
        }
    }
    deliver(ctx.state, items).await?;
    // 4. Off the friend lists kept where messages go only between friends.
    db::remove_friends(&mut *ctx.state.db.acquire().await?, ctx.user_id, target_id).await?;
    ctx.state.private.set_friends(ctx.user_id, target_id, false);
    Ok(ack())
}
