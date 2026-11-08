//! Managing plugins (design 2026-11-07 §17). Every request needs
//! `MANAGE_PLUGINS`.

use opencord_proto::v1 as proto;
use proto::response::Result as Response;

use super::{Ctx, ack};
use crate::error::ApiError;
use crate::plugins::{self, install, review};

/// `FetchPluginLog`'s page size when none is asked for, and its largest.
const LOG_PAGE: u32 = 200;
const LOG_PAGE_MAX: u32 = 500;

pub async fn fetch(ctx: &Ctx<'_>) -> Result<Response, ApiError> {
    install::require_manager(ctx.state, ctx.user_id)?;
    let rows = crate::db::plugins::plugins(&mut *ctx.state.db.acquire().await?).await?;
    let mut list = Vec::with_capacity(rows.len());
    for row in rows {
        if let Some(plugin) = plugins::describe(ctx.state, &row.id, Some(ctx.user_id)).await? {
            list.push(plugin);
        }
    }
    Ok(Response::Plugins(proto::Plugins { plugins: list }))
}

pub async fn fetch_reviews(ctx: &Ctx<'_>) -> Result<Response, ApiError> {
    install::require_manager(ctx.state, ctx.user_id)?;
    let mut reviews = Vec::new();
    for pending in ctx.state.plugins.reviews.all() {
        reviews.push(review::to_proto(ctx.state, &pending, ctx.user_id).await?);
    }
    Ok(Response::PluginReviews(proto::PluginReviews { reviews }))
}

pub async fn confirm(
    ctx: &Ctx<'_>,
    request: proto::ConfirmPluginReview,
) -> Result<Response, ApiError> {
    let plugin = install::confirm(ctx.state, ctx.user_id, request).await?;
    Ok(Response::Plugin(plugin))
}

pub async fn discard(
    ctx: &Ctx<'_>,
    request: proto::DiscardPluginReview,
) -> Result<Response, ApiError> {
    install::require_manager(ctx.state, ctx.user_id)?;
    let pending = ctx
        .state
        .plugins
        .reviews
        .take(&request.review_id)
        .ok_or_else(|| ApiError::not_found("review"))?;
    review::remove_folder(&pending.folder).await;
    review::removed(ctx.state, pending.id.clone());
    Ok(ack())
}

pub async fn set_permissions(
    ctx: &Ctx<'_>,
    request: proto::SetPluginPermissions,
) -> Result<Response, ApiError> {
    let plugin = install::set_permissions(ctx.state, ctx.user_id, request).await?;
    Ok(Response::Plugin(plugin))
}

pub async fn set_config(
    ctx: &Ctx<'_>,
    request: proto::SetPluginConfig,
) -> Result<Response, ApiError> {
    let plugin = install::set_config(ctx.state, ctx.user_id, request).await?;
    Ok(Response::Plugin(plugin))
}

pub async fn set_running(
    ctx: &Ctx<'_>,
    request: proto::SetPluginRunning,
) -> Result<Response, ApiError> {
    let plugin = install::set_running(ctx.state, ctx.user_id, request).await?;
    Ok(Response::Plugin(plugin))
}

pub async fn restart(ctx: &Ctx<'_>, request: proto::RestartPlugin) -> Result<Response, ApiError> {
    let plugin = install::restart(ctx.state, ctx.user_id, request).await?;
    Ok(Response::Plugin(plugin))
}

pub async fn uninstall(
    ctx: &Ctx<'_>,
    request: proto::UninstallPlugin,
) -> Result<Response, ApiError> {
    install::uninstall(ctx.state, ctx.user_id, request).await?;
    Ok(ack())
}

/// The plugin's log, newest page first; lines oldest first.
pub async fn fetch_log(
    ctx: &Ctx<'_>,
    request: proto::FetchPluginLog,
) -> Result<Response, ApiError> {
    install::require_manager(ctx.state, ctx.user_id)?;
    let limit = match request.limit {
        0 => LOG_PAGE,
        limit if limit <= LOG_PAGE_MAX => limit,
        _ => {
            return Err(ApiError::invalid_argument(format!(
                "limit is at most {LOG_PAGE_MAX}"
            )));
        }
    };
    let lines = match ctx.state.plugins.get(&request.plugin_id) {
        Some(handle) => handle.log().lines(request.before, limit as usize),
        None => {
            // Installed but not run since the server started: nothing yet.
            crate::db::plugins::plugin(&mut *ctx.state.db.acquire().await?, &request.plugin_id)
                .await?
                .ok_or_else(|| ApiError::not_found("plugin"))?;
            Vec::new()
        }
    };
    Ok(Response::PluginLog(proto::PluginLog { lines }))
}
