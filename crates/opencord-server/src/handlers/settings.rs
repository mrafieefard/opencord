use opencord_common::channel::ChannelKind;
use opencord_common::permissions::Permissions;
use opencord_common::validation;
use opencord_proto::v1 as proto;
use proto::event::Kind as Event;
use proto::response::Result as Response;

use super::Ctx;
use crate::db::meta::ServerMeta;
use crate::error::ApiError;
use crate::permissions::require_base;
use crate::state::{AppState, Audience, now_ms};
use crate::voice::settings::VoiceSettings;
use crate::voice::{self, media_token};

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

pub async fn update_voice_settings(
    ctx: &Ctx<'_>,
    request: proto::UpdateVoiceSettings,
) -> Result<Response, ApiError> {
    let _writes = ctx.state.write_lock().await;
    let updated = {
        let guild = ctx.state.guild();
        require_base(&guild, ctx.user_id, Permissions::MANAGE_SERVER)?;
        guild.voice_settings.apply(&request, |channel_id| {
            guild
                .channels
                .get(&channel_id)
                .is_some_and(|channel| channel.kind == ChannelKind::Voice)
        })?
    };
    save_voice_settings(ctx.state, updated.clone()).await?;
    Ok(Response::VoiceSettings(updated.to_proto()))
}

/// Saves, caches and announces voice settings; people may become
/// (un)suppressed when the AFK channel changes. Callers hold the write lock.
pub async fn save_voice_settings(
    state: &AppState,
    settings: VoiceSettings,
) -> Result<(), ApiError> {
    settings.save(&mut *state.db.acquire().await?).await?;
    state.guild_mut().voice_settings = settings.clone();
    state.broadcast(
        Event::VoiceSettingsUpdate(proto::VoiceSettingsUpdate {
            settings: Some(settings.to_proto()),
        }),
        Audience::Everyone,
    );
    voice::reconcile(state);
    voice::limits_changed(state, &state.voice_nodes.assigned());
    Ok(())
}

pub fn refresh_media_token(ctx: &Ctx<'_>) -> Response {
    let server_id = ctx.state.guild().meta.server_id;
    Response::MediaToken(media_token::issue(
        &ctx.state.voice_key,
        ctx.user_id,
        &server_id,
        now_ms(),
    ))
}
