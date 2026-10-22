//! Domain requests (Phase 3 plan §10): the owner's only. Each answers with
//! the network as it is now; checks run on and report through
//! `NetworkStatus`.

use opencord_proto::v1 as proto;
use proto::response::Result as Response;

use super::Ctx;
use super::network::{info, require_owner};
use crate::certs::providers::ProviderKind;
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

pub async fn request_certificate(
    ctx: &Ctx<'_>,
    request: proto::RequestCertificate,
) -> Result<Response, ApiError> {
    require_owner(ctx)?;
    ctx.state
        .certificates
        .request(
            ctx.state,
            request.domain_id,
            request.email,
            request.agree_terms,
        )
        .await?;
    Ok(info(ctx))
}

pub async fn upload_certificate(
    ctx: &Ctx<'_>,
    request: proto::UploadCertificate,
) -> Result<Response, ApiError> {
    require_owner(ctx)?;
    if request.chain_pem.len() > MAX_PEM || request.key_pem.len() > MAX_PEM {
        return Err(ApiError::invalid_argument("the files are too large"));
    }
    ctx.state
        .certificates
        .upload(
            ctx.state,
            request.domain_id,
            &request.chain_pem,
            &request.key_pem,
        )
        .await?;
    Ok(info(ctx))
}

pub async fn set_dns_provider(
    ctx: &Ctx<'_>,
    request: proto::SetDnsProvider,
) -> Result<Response, ApiError> {
    require_owner(ctx)?;
    let kind = ProviderKind::parse(&request.kind)
        .ok_or_else(|| ApiError::invalid_argument("Cloudflare, DuckDNS or deSEC"))?;
    if request.token.len() > 512 {
        return Err(ApiError::invalid_argument("the token is too long"));
    }
    ctx.state
        .certificates
        .set_provider(ctx.state, kind, &request.token)
        .await?;
    tracing::info!(
        user_id = ctx.user_id,
        kind = kind.as_str(),
        connected = !request.token.trim().is_empty(),
        "the owner changed a DNS provider"
    );
    ctx.state.network.changed(ctx.state);
    Ok(info(ctx))
}

pub async fn add_free_subdomain(
    ctx: &Ctx<'_>,
    request: proto::AddFreeSubdomain,
) -> Result<Response, ApiError> {
    require_owner(ctx)?;
    if request.token.len() > 512 || request.name.len() > 300 {
        return Err(ApiError::invalid_argument("too long"));
    }
    crate::domains::free::add(ctx.state, request).await?;
    ctx.state.network.changed(ctx.state);
    Ok(info(ctx))
}

pub async fn start_tunnel(
    ctx: &Ctx<'_>,
    request: proto::StartTunnel,
) -> Result<Response, ApiError> {
    require_owner(ctx)?;
    if request.token.len() > 4096 || request.hostname.len() > 300 {
        return Err(ApiError::invalid_argument("too long"));
    }
    let kind = proto::TunnelKind::try_from(request.kind).unwrap_or_default();
    crate::tunnel::start(ctx.state, kind, &request.token, &request.hostname).await?;
    tracing::info!(user_id = ctx.user_id, kind = ?kind, "the owner started a tunnel");
    Ok(info(ctx))
}

pub async fn stop_tunnel(ctx: &Ctx<'_>) -> Result<Response, ApiError> {
    require_owner(ctx)?;
    crate::tunnel::stop(ctx.state).await?;
    Ok(info(ctx))
}

pub async fn tunnel_log(ctx: &Ctx<'_>) -> Result<Response, ApiError> {
    require_owner(ctx)?;
    Ok(Response::TunnelLog(proto::TunnelLog {
        lines: ctx.state.tunnel.log(),
    }))
}

pub async fn install_cloudflared(ctx: &Ctx<'_>) -> Result<Response, ApiError> {
    require_owner(ctx)?;
    crate::tunnel::install::start(ctx.state)?;
    Ok(info(ctx))
}

/// A certificate chain or key larger than this is not one.
const MAX_PEM: usize = 64 * 1024;
