//! Domains & network requests (Phase 3 plan §10): the owner's only.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use opencord_proto::v1 as proto;
use proto::response::Result as Response;

use super::Ctx;
use crate::error::ApiError;
use crate::network::settings;

fn require_owner(ctx: &Ctx<'_>) -> Result<(), ApiError> {
    if ctx.state.guild().is_owner(ctx.user_id) {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "only the server's owner manages its domains and network",
        ))
    }
}

fn info(ctx: &Ctx<'_>) -> Response {
    Response::NetworkInfo(Box::new(ctx.state.network.info()))
}

pub fn watch(ctx: &Ctx<'_>, request: &proto::WatchNetwork) -> Result<Response, ApiError> {
    require_owner(ctx)?;
    ctx.state.network.watch(ctx.session_id, request.watch);
    Ok(info(ctx))
}

pub async fn set_public_address(
    ctx: &Ctx<'_>,
    request: proto::SetPublicAddress,
) -> Result<Response, ApiError> {
    require_owner(ctx)?;
    let override_v4 = parse_override::<Ipv4Addr>(&request.override_ipv4, "an IPv4 address")?;
    let override_v6 = parse_override::<Ipv6Addr>(&request.override_ipv6, "an IPv6 address")?;
    let previous = ctx.state.network.settings();
    let settings = settings::NetworkSettings {
        auto_detect: request.auto_detect,
        override_v4: override_v4.map(IpAddr::V4),
        override_v6: override_v6.map(IpAddr::V6),
        ..previous.clone()
    };
    settings.save(&mut *ctx.state.db.acquire().await?).await?;
    ctx.state.network.replace_settings(ctx.state, settings);
    tracing::info!(
        user_id = ctx.user_id,
        auto_detect = request.auto_detect,
        ?override_v4,
        ?override_v6,
        "the owner set the public address"
    );
    if request.auto_detect && !previous.auto_detect {
        ctx.state.network.detect_now(ctx.state).await;
    }
    Ok(info(ctx))
}

pub async fn detect_public_address(ctx: &Ctx<'_>) -> Result<Response, ApiError> {
    require_owner(ctx)?;
    ctx.state.network.detect_now(ctx.state).await;
    Ok(info(ctx))
}

pub async fn update_settings(
    ctx: &Ctx<'_>,
    request: proto::UpdateNetworkSettings,
) -> Result<Response, ApiError> {
    require_owner(ctx)?;
    let settings = ctx.state.network.settings().apply(&request)?;
    settings.save(&mut *ctx.state.db.acquire().await?).await?;
    tracing::info!(
        user_id = ctx.user_id,
        ?settings,
        "the owner changed the network settings"
    );
    ctx.state.network.replace_settings(ctx.state, settings);
    Ok(info(ctx))
}

/// An override: empty for none, else an address that can be reached.
fn parse_override<T: std::str::FromStr + Into<IpAddr> + Copy>(
    text: &str,
    what: &str,
) -> Result<Option<T>, ApiError> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let ip: T = text
        .parse()
        .map_err(|_| ApiError::invalid_argument(format!("{text:?} is not {what}")))?;
    let as_ip: IpAddr = ip.into();
    if as_ip.is_unspecified() || as_ip.is_loopback() || as_ip.is_multicast() {
        return Err(ApiError::invalid_argument(format!(
            "{text} cannot be reached from the internet"
        )));
    }
    Ok(Some(ip))
}
