//! Server-side permission checks built on `opencord_common::permissions`.

use opencord_common::permissions::{Permissions, can_grant};

use crate::error::ApiError;
use crate::guild::{Channel, Guild};

/// Server-wide permissions; returns everything the user holds.
pub fn require_base(
    guild: &Guild,
    user_id: i64,
    needed: Permissions,
) -> Result<Permissions, ApiError> {
    let held = guild.base_permissions(user_id);
    if held.contains(needed) {
        Ok(held)
    } else {
        Err(missing(needed - held))
    }
}

/// A channel the user can view, with `needed` there. Channels the user
/// cannot view are reported as not found.
pub fn require_channel(
    guild: &Guild,
    user_id: i64,
    channel_id: i64,
    needed: Permissions,
) -> Result<&Channel, ApiError> {
    let held = guild.channel_permissions(user_id, channel_id);
    let channel = guild
        .channels
        .get(&channel_id)
        .filter(|_| held.contains(Permissions::VIEW_CHANNEL))
        .ok_or_else(|| ApiError::not_found("channel"))?;
    if held.contains(needed) {
        Ok(channel)
    } else {
        Err(missing(needed - held))
    }
}

pub fn require_outranks_member(
    guild: &Guild,
    actor_id: i64,
    target_id: i64,
) -> Result<(), ApiError> {
    let target_is_owner = guild.is_owner(target_id);
    let target_highest = guild.highest_position(target_id);
    let outranks = guild
        .with_member_context(actor_id, |actor| {
            actor.outranks_member(target_is_owner, target_highest)
        })
        .unwrap_or(false);
    if outranks {
        Ok(())
    } else {
        Err(ApiError::forbidden("that member ranks at or above you"))
    }
}

pub fn require_outranks_role(guild: &Guild, actor_id: i64, position: i32) -> Result<(), ApiError> {
    let outranks = guild
        .with_member_context(actor_id, |actor| actor.outranks_role(position))
        .unwrap_or(false);
    if outranks {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "that role ranks at or above your highest role",
        ))
    }
}

pub fn require_grantable(held: Permissions, requested: Permissions) -> Result<(), ApiError> {
    if can_grant(held, requested) {
        Ok(())
    } else {
        Err(ApiError::forbidden(format!(
            "you can only grant or deny permissions you have; missing: {}",
            names(requested - held)
        )))
    }
}

fn missing(permissions: Permissions) -> ApiError {
    ApiError::forbidden(format!("missing permission: {}", names(permissions)))
}

fn names(permissions: Permissions) -> String {
    permissions
        .iter_names()
        .map(|(name, _)| name)
        .collect::<Vec<_>>()
        .join(", ")
}
