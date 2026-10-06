use opencord_common::validation;
use opencord_proto::v1 as proto;
use proto::response::Result as Response;

use super::roles::broadcast_member_update;
use super::{Ctx, ack};
use crate::db::users;
use crate::error::ApiError;

pub async fn update_profile(
    ctx: &Ctx<'_>,
    request: proto::UpdateProfile,
) -> Result<Response, ApiError> {
    let display_name = validation::display_name(&request.display_name)?;
    let _writes = ctx.state.write_lock().await;
    if !ctx.state.guild().members.contains_key(&ctx.user_id) {
        return Err(ApiError::not_found("member"));
    }
    users::set_display_name(
        &mut *ctx.state.db.acquire().await?,
        ctx.user_id,
        &display_name,
    )
    .await?;
    let member = {
        let mut guild = ctx.state.guild_mut();
        let member = guild
            .members
            .get_mut(&ctx.user_id)
            .ok_or_else(|| ApiError::not_found("member"))?;
        member.user.display_name = display_name;
        member.clone()
    };
    broadcast_member_update(ctx, &member);
    Ok(Response::User(member.user.to_proto()))
}

pub fn update_presence(
    ctx: &Ctx<'_>,
    request: proto::UpdatePresence,
) -> Result<Response, ApiError> {
    let status = proto::PresenceStatus::try_from(request.status)
        .ok()
        .filter(|status| *status != proto::PresenceStatus::Unspecified)
        .ok_or_else(|| ApiError::invalid_argument("unknown presence status"))?;
    ctx.state.presence.set(ctx.user_id, status);
    ctx.state.broadcast_presence(ctx.user_id);
    Ok(ack())
}
