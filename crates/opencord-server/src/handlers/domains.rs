//! Domain requests (Phase 3 plan §10): the owner's only. Each answers with
//! the network as it is now; checks run on and report through
//! `NetworkStatus`.

use opencord_proto::v1 as proto;
use proto::response::Result as Response;

use super::Ctx;
use super::network::{info, require_owner};
use crate::domains::DomainKind;
use crate::error::ApiError;

pub async fn add(ctx: &Ctx<'_>, request: proto::AddDomain) -> Result<Response, ApiError> {
    require_owner(ctx)?;
    let kind = DomainKind::from_proto(request.kind)
        .ok_or_else(|| ApiError::invalid_argument("choose what kind of domain it is"))?;
    let port = request
        .public_port
        .map(|port| {
            u16::try_from(port)
                .ok()
                .filter(|port| *port != 0)
                .ok_or_else(|| ApiError::invalid_argument("the port is 1 to 65535"))
        })
        .transpose()?;
    ctx.state
        .domains
        .add(ctx.state, &request.hostname, kind, port)
        .await?;
    crate::domains::sync_local_listener(ctx.state).await;
    ctx.state.network.changed(ctx.state);
    Ok(info(ctx))
}

pub async fn update(ctx: &Ctx<'_>, request: proto::UpdateDomain) -> Result<Response, ApiError> {
    require_owner(ctx)?;
    ctx.state.domains.update(ctx.state, &request).await?;
    ctx.state.network.changed(ctx.state);
    Ok(info(ctx))
}

pub async fn delete(ctx: &Ctx<'_>, request: proto::DeleteDomain) -> Result<Response, ApiError> {
    require_owner(ctx)?;
    ctx.state
        .domains
        .delete(ctx.state, request.domain_id)
        .await?;
    crate::domains::sync_local_listener(ctx.state).await;
    ctx.state.network.changed(ctx.state);
    Ok(info(ctx))
}

pub fn check(ctx: &Ctx<'_>, request: &proto::CheckDomain) -> Result<Response, ApiError> {
    require_owner(ctx)?;
    ctx.state
        .domains
        .check_by_hand(ctx.state, request.domain_id)?;
    Ok(info(ctx))
}

pub async fn history(
    ctx: &Ctx<'_>,
    request: proto::FetchDomainChecks,
) -> Result<Response, ApiError> {
    require_owner(ctx)?;
    let checks = ctx
        .state
        .domains
        .history(ctx.state, request.domain_id)
        .await?;
    Ok(Response::DomainChecks(proto::DomainChecks { checks }))
}
