//! Lavalink node requests (Phase 3 plan §12, §18.1): `MANAGE_SERVER`.

use opencord_common::permissions::Permissions;
use opencord_proto::v1 as proto;
use proto::response::Result as Response;

use super::Ctx;
use crate::error::ApiError;
use crate::permissions::require_base;

fn require_manage_server(ctx: &Ctx<'_>) -> Result<(), ApiError> {
    require_base(&ctx.state.guild(), ctx.user_id, Permissions::MANAGE_SERVER).map(drop)
}

fn nodes(ctx: &Ctx<'_>) -> Response {
    Response::LavalinkNodes(Box::new(ctx.state.lavalink.info(ctx.state)))
}

pub fn fetch(ctx: &Ctx<'_>, request: &proto::FetchLavalinkNodes) -> Result<Response, ApiError> {
    require_manage_server(ctx)?;
    ctx.state.lavalink.watch(ctx.session_id, request.watch);
    Ok(nodes(ctx))
}

pub async fn create(
    ctx: &Ctx<'_>,
    request: proto::CreateLavalinkNode,
) -> Result<Response, ApiError> {
    require_manage_server(ctx)?;
    let settings = request
        .settings
        .ok_or_else(|| ApiError::invalid_argument("the node's settings are missing"))?;
    ctx.state
        .lavalink
        .create(ctx.state, &settings, &request.password)
        .await?;
    Ok(nodes(ctx))
}

pub async fn update(
    ctx: &Ctx<'_>,
    request: proto::UpdateLavalinkNode,
) -> Result<Response, ApiError> {
    require_manage_server(ctx)?;
    ctx.state.lavalink.update(ctx.state, &request).await?;
    Ok(nodes(ctx))
}

pub async fn delete(
    ctx: &Ctx<'_>,
    request: &proto::DeleteLavalinkNode,
) -> Result<Response, ApiError> {
    require_manage_server(ctx)?;
    ctx.state
        .lavalink
        .delete(ctx.state, request.node_id)
        .await?;
    Ok(nodes(ctx))
}

pub fn test(ctx: &Ctx<'_>, request: &proto::TestLavalinkNode) -> Result<Response, ApiError> {
    require_manage_server(ctx)?;
    crate::lavalink::start_test(ctx.state, request.node_id, ctx.session_id)?;
    Ok(Response::Ack(proto::Ack {}))
}
