use opencord_common::permissions::{OverwriteTarget, Permissions};
use opencord_common::validation;
use opencord_proto::v1 as proto;
use proto::event::Kind as Event;
use proto::response::Result as Response;

use super::roles::broadcast_member_update;
use super::{Ctx, ack};
use crate::db::{bans, channels, members, users};
use crate::error::ApiError;
use crate::gateway::session::CloseCode;
use crate::guild::{Guild, Member};
use crate::permissions::{require_base, require_outranks_member};
use crate::state::{Audience, now_ms};
use crate::visibility::Visibility;

pub async fn kick(ctx: &Ctx<'_>, request: proto::KickMember) -> Result<Response, ApiError> {
    optional_reason(request.reason.as_deref())?;
    let _writes = ctx.state.write_lock().await;
    let overwrite_channels = {
        let guild = ctx.state.guild();
        require_base(&guild, ctx.user_id, Permissions::KICK_MEMBERS)?;
        require_other(ctx, request.user_id)?;
        if !guild.members.contains_key(&request.user_id) {
            return Err(ApiError::not_found("member"));
        }
        require_outranks_member(&guild, ctx.user_id, request.user_id)?;
        channels_with_member_overwrite(&guild, request.user_id)
    };
    let mut tx = ctx.state.db.begin().await?;
    channels::delete_overwrites_for_target(&mut tx, "member", request.user_id).await?;
    members::delete(&mut tx, request.user_id).await?;
    tx.commit().await?;
    remove_member(ctx, request.user_id, CloseCode::KICKED, &overwrite_channels);
    Ok(ack())
}

pub async fn ban(ctx: &Ctx<'_>, request: proto::BanMember) -> Result<Response, ApiError> {
    let reason = optional_reason(request.reason.as_deref())?;
    let _writes = ctx.state.write_lock().await;
    let (is_member, overwrite_channels) = {
        let guild = ctx.state.guild();
        require_base(&guild, ctx.user_id, Permissions::BAN_MEMBERS)?;
        require_other(ctx, request.user_id)?;
        let is_member = guild.members.contains_key(&request.user_id);
        if is_member {
            require_outranks_member(&guild, ctx.user_id, request.user_id)?;
        }
        (
            is_member,
            channels_with_member_overwrite(&guild, request.user_id),
        )
    };
    let mut tx = ctx.state.db.begin().await?;
    if users::find(&mut tx, request.user_id).await?.is_none() {
        return Err(ApiError::not_found("user"));
    }
    bans::insert(
        &mut tx,
        request.user_id,
        reason.as_deref(),
        ctx.user_id,
        now_ms(),
    )
    .await?;
    if is_member {
        channels::delete_overwrites_for_target(&mut tx, "member", request.user_id).await?;
        members::delete(&mut tx, request.user_id).await?;
    }
    tx.commit().await?;
    if is_member {
        remove_member(ctx, request.user_id, CloseCode::BANNED, &overwrite_channels);
    }
    Ok(ack())
}

pub async fn unban(ctx: &Ctx<'_>, request: proto::UnbanMember) -> Result<Response, ApiError> {
    require_base(&ctx.state.guild(), ctx.user_id, Permissions::BAN_MEMBERS)?;
    if bans::delete(&mut *ctx.state.db.acquire().await?, request.user_id).await? {
        Ok(ack())
    } else {
        Err(ApiError::not_found("ban"))
    }
}

pub async fn fetch_bans(ctx: &Ctx<'_>) -> Result<Response, ApiError> {
    require_base(&ctx.state.guild(), ctx.user_id, Permissions::BAN_MEMBERS)?;
    let rows = bans::list(&mut *ctx.state.db.acquire().await?).await?;
    Ok(Response::Bans(proto::BanList {
        bans: rows
            .into_iter()
            .map(|row| proto::Ban {
                user: Some(proto::User {
                    id: row.user_id,
                    public_key: row.public_key,
                    display_name: row.display_name,
                }),
                reason: row.reason,
                banned_by: row.banned_by,
                created_at_ms: row.created_at,
            })
            .collect(),
    }))
}

pub async fn update_nickname(
    ctx: &Ctx<'_>,
    request: proto::UpdateNickname,
) -> Result<Response, ApiError> {
    let nickname = match request.nickname.as_deref() {
        Some(nickname) => validation::nickname(nickname)?,
        None => None,
    };
    let _writes = ctx.state.write_lock().await;
    let updated = {
        let guild = ctx.state.guild();
        let member = guild
            .members
            .get(&request.user_id)
            .ok_or_else(|| ApiError::not_found("member"))?;
        if request.user_id == ctx.user_id {
            require_base(&guild, ctx.user_id, Permissions::CHANGE_NICKNAME)?;
        } else {
            require_base(&guild, ctx.user_id, Permissions::MANAGE_NICKNAMES)?;
            require_outranks_member(&guild, ctx.user_id, request.user_id)?;
        }
        Member {
            nickname,
            ..member.clone()
        }
    };
    members::set_nickname(
        &mut *ctx.state.db.acquire().await?,
        updated.user.id,
        updated.nickname.as_deref(),
    )
    .await?;
    ctx.state
        .guild_mut()
        .members
        .insert(updated.user.id, updated.clone());
    broadcast_member_update(ctx, &updated);
    Ok(Response::Member(updated.to_proto()))
}

/// Ends the user's sessions, then drops them from the cache and announces
/// it. The database rows must already be gone.
fn remove_member(ctx: &Ctx<'_>, user_id: i64, close: CloseCode, overwrite_channels: &[i64]) {
    for session in ctx.state.sessions.for_user(user_id) {
        session.end(close);
        ctx.state.sessions.remove(&session.id);
    }
    ctx.state.presence.clear(user_id);
    let before = Visibility::capture(ctx.state);
    {
        let mut guild = ctx.state.guild_mut();
        guild.members.remove(&user_id);
        let target = OverwriteTarget::Member(user_id);
        for channel_id in overwrite_channels {
            if let Some(channel) = guild.channels.get_mut(channel_id) {
                channel
                    .overwrites
                    .retain(|overwrite| overwrite.target != target);
            }
        }
    }
    ctx.state.broadcast(
        Event::MemberLeave(proto::MemberLeave { user_id }),
        Audience::Everyone,
    );
    before.announce(ctx.state, overwrite_channels);
}

fn channels_with_member_overwrite(guild: &Guild, user_id: i64) -> Vec<i64> {
    let target = OverwriteTarget::Member(user_id);
    guild
        .channels
        .values()
        .filter(|channel| channel.overwrites.iter().any(|o| o.target == target))
        .map(|channel| channel.id)
        .collect()
}

fn require_other(ctx: &Ctx<'_>, user_id: i64) -> Result<(), ApiError> {
    if user_id == ctx.user_id {
        Err(ApiError::invalid_argument("you cannot do that to yourself"))
    } else {
        Ok(())
    }
}

fn optional_reason(reason: Option<&str>) -> Result<Option<String>, ApiError> {
    Ok(reason.map(validation::reason).transpose()?.flatten())
}
