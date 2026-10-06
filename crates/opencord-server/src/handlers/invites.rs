use opencord_common::permissions::Permissions;
use opencord_proto::v1 as proto;
use proto::response::Result as Response;

use super::{Ctx, ack};
use crate::db::invites::{self, InviteRow};
use crate::error::ApiError;
use crate::permissions::require_base;
use crate::state::now_ms;

pub async fn create(ctx: &Ctx<'_>, request: proto::CreateInvite) -> Result<Response, ApiError> {
    require_base(&ctx.state.guild(), ctx.user_id, Permissions::CREATE_INVITE)?;
    let invite = invites::create(
        &mut *ctx.state.db.acquire().await?,
        Some(ctx.user_id),
        request.max_uses,
        request.expires_in_s,
        now_ms(),
    )
    .await?;
    Ok(Response::Invite(to_proto(invite)))
}

pub async fn fetch(ctx: &Ctx<'_>) -> Result<Response, ApiError> {
    require_base(&ctx.state.guild(), ctx.user_id, Permissions::MANAGE_SERVER)?;
    let rows = invites::list(&mut *ctx.state.db.acquire().await?).await?;
    Ok(Response::Invites(proto::InviteList {
        invites: rows.into_iter().map(to_proto).collect(),
    }))
}

pub async fn revoke(ctx: &Ctx<'_>, request: proto::RevokeInvite) -> Result<Response, ApiError> {
    let mut conn = ctx.state.db.acquire().await?;
    let invite = invites::find(&mut conn, &request.code)
        .await?
        .ok_or_else(|| ApiError::not_found("invite"))?;
    if invite.created_by != Some(ctx.user_id) {
        require_base(&ctx.state.guild(), ctx.user_id, Permissions::MANAGE_SERVER)?;
    }
    invites::delete(&mut conn, &invite.code).await?;
    Ok(ack())
}

fn to_proto(row: InviteRow) -> proto::Invite {
    proto::Invite {
        code: row.code,
        created_by: row.created_by,
        created_at_ms: row.created_at,
        max_uses: row.max_uses.and_then(|uses| u32::try_from(uses).ok()),
        uses: u32::try_from(row.uses).unwrap_or(u32::MAX),
        expires_at_ms: row.expires_at,
    }
}
