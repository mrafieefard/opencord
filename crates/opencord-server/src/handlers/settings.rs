use opencord_common::permissions::Permissions;
use opencord_common::validation;
use opencord_proto::v1 as proto;
use proto::event::Kind as Event;
use proto::response::Result as Response;

use super::Ctx;
use crate::db::meta::ServerMeta;
use crate::error::ApiError;
use crate::permissions::require_base;
use crate::state::Audience;

pub async fn update_server(
    ctx: &Ctx<'_>,
    request: proto::UpdateServer,
) -> Result<Response, ApiError> {
    let name = request
        .name
        .as_deref()
        .map(validation::server_name)
        .transpose()?;
    let description = request
        .description
        .as_deref()
        .map(validation::server_description)
        .transpose()?;

    let _writes = ctx.state.write_lock().await;
    let meta = {
        let guild = ctx.state.guild();
        require_base(&guild, ctx.user_id, Permissions::MANAGE_SERVER)?;
        ServerMeta {
            name: name.unwrap_or_else(|| guild.meta.name.clone()),
            description: description.unwrap_or_else(|| guild.meta.description.clone()),
            open_join: request.open_join.unwrap_or(guild.meta.open_join),
            ..guild.meta.clone()
        }
    };
    meta.save(&mut *ctx.state.db.acquire().await?).await?;
    let info = {
        let mut guild = ctx.state.guild_mut();
        guild.meta = meta;
        guild.server_info()
    };
    ctx.state.broadcast(
        Event::ServerUpdate(proto::ServerUpdate {
            server: Some(info.clone()),
        }),
        Audience::Everyone,
    );
    Ok(Response::Server(info))
}
