//! Chat command requests (Phase 3 plan §17.5): running a command, its
//! autocomplete, and the Commands page.

use std::time::Instant;

use opencord_common::permissions::Permissions;
use opencord_common::snowflake;
use opencord_proto::v1 as proto;
use proto::event::Kind as Event;
use proto::response::Result as Response;

use super::Ctx;
use crate::commands::music::{self, Reply};
use crate::commands::options::Options;
use crate::commands::{self, CommandSettings};
use crate::db::messages;
use crate::error::ApiError;
use crate::permissions::{require_base, require_channel};
use crate::state::Audience;

/// Nonces are at most this long.
const NONCE_MAX: usize = 64;

/// The command, on, and allowed in `channel_id`.
fn usable(ctx: &Ctx<'_>, name: &str, channel_id: i64) -> Result<&'static commands::Spec, ApiError> {
    let spec = commands::find(name).ok_or_else(|| {
        ApiError::new(
            proto::ErrorCode::CommandUnknown,
            format!("/{} isn't a command here", name.trim_start_matches('/')),
        )
    })?;
    let settings = ctx.state.commands.settings();
    if !settings.enabled(spec.name) {
        return Err(ApiError::new(
            proto::ErrorCode::CommandDisabled,
            format!("/{} is turned off on this server", spec.name),
        ));
    }
    if !settings.allowed_in(spec.name, channel_id) {
        return Err(ApiError::new(
            proto::ErrorCode::CommandNotAllowedHere,
            format!("/{} can't be used in this channel", spec.name),
        ));
    }
    Ok(spec)
}

pub async fn execute(ctx: &Ctx<'_>, request: proto::ExecuteCommand) -> Result<Response, ApiError> {
    if request.nonce.chars().count() > NONCE_MAX {
        return Err(ApiError::invalid_argument("the nonce is too long"));
    }
    {
        // Commands are typed where messages are (§17.1).
        let guild = ctx.state.guild();
        require_channel(
            &guild,
            ctx.user_id,
            request.channel_id,
            Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES,
        )?;
    }
    let spec = usable(ctx, &request.name, request.channel_id)?;
    let options = Options::parse(spec, &request.options)?;
    let reply = match spec.feature {
        "music" => music::run(ctx, spec, &options).await?,
        _ => return Err(ApiError::internal()),
    };
    let public = !ctx.state.commands.settings().results_private;
    match reply {
        Reply::Public(content) if public => {
            let id = ctx.state.ids.next_id();
            let mut conn = ctx.state.db.acquire().await?;
            messages::insert_result(
                &mut conn,
                id,
                request.channel_id,
                ctx.user_id,
                &content,
                Some(spec.name),
            )
            .await?;
            let message = proto::Message {
                id,
                channel_id: request.channel_id,
                author_id: ctx.user_id,
                content,
                created_at_ms: snowflake::timestamp_ms(id),
                edited_at_ms: None,
                nonce: None,
                interaction: Some(proto::MessageInteraction {
                    user_id: ctx.user_id,
                    command: spec.name.to_owned(),
                }),
                attachments: Vec::new(),
            };
            ctx.state.broadcast(
                Event::MessageCreate(proto::MessageCreate {
                    message: Some(message),
                }),
                Audience::Channel(request.channel_id),
            );
            Ok(Response::CommandResult(proto::CommandResult {
                message_id: Some(id),
                ephemeral: false,
            }))
        }
        Reply::Public(content) | Reply::Private(content) => {
            ephemeral(ctx, request.channel_id, &request.nonce, content);
            Ok(Response::CommandResult(proto::CommandResult {
                message_id: None,
                ephemeral: true,
            }))
        }
    }
}

/// A reply only the sending session sees (§17.3); never stored.
fn ephemeral(ctx: &Ctx<'_>, channel_id: i64, nonce: &str, content: String) {
    if let Some(session) = ctx.state.sessions.get(ctx.session_id) {
        let event = proto::Event {
            kind: Some(Event::EphemeralMessage(proto::EphemeralMessage {
                channel_id,
                nonce: nonce.to_owned(),
                content,
            })),
        };
        session.push_event(&event, Instant::now());
    }
}

pub async fn autocomplete(
    ctx: &Ctx<'_>,
    request: proto::AutocompleteCommand,
) -> Result<Response, ApiError> {
    ctx.state
        .rate_limits
        .check_autocomplete(ctx.user_id)
        .map_err(ApiError::rate_limited)?;
    let spec = usable(ctx, &request.name, request.channel_id)?;
    let autocompletes = spec
        .option(&request.option)
        .is_some_and(|option| option.autocomplete);
    let choices = if autocompletes && spec.name == "play" {
        music::autocomplete(ctx, &request.value).await?
    } else {
        Vec::new()
    };
    Ok(Response::CommandChoices(proto::CommandChoices { choices }))
}

pub fn fetch_settings(ctx: &Ctx<'_>) -> Result<Response, ApiError> {
    require_base(&ctx.state.guild(), ctx.user_id, Permissions::MANAGE_SERVER)?;
    Ok(Response::Commands(commands::all(ctx.state)))
}

pub async fn update_settings(
    ctx: &Ctx<'_>,
    request: proto::UpdateCommandSettings,
) -> Result<Response, ApiError> {
    require_base(&ctx.state.guild(), ctx.user_id, Permissions::MANAGE_SERVER)?;
    let _writes = ctx.state.write_lock().await;
    let mut settings: CommandSettings = ctx.state.commands.settings();
    if !request.name.is_empty() {
        let spec = commands::find(&request.name).ok_or_else(|| ApiError::not_found("command"))?;
        let name = spec.name.to_owned();
        if let Some(enabled) = request.enabled {
            settings.disabled.retain(|off| *off != name);
            if !enabled {
                settings.disabled.push(name.clone());
            }
        }
        if let Some(channels) = request.channel_ids {
            let guild = ctx.state.guild();
            if channels
                .ids
                .iter()
                .any(|id| !guild.channels.contains_key(id))
            {
                return Err(ApiError::not_found("channel"));
            }
            if channels.ids.is_empty() {
                settings.channels.remove(&name);
            } else {
                settings.channels.insert(name, channels.ids);
            }
        }
    }
    if let Some(public) = request.results_public {
        settings.results_private = !public;
    }
    let mut conn = ctx.state.db.acquire().await?;
    ctx.state.commands.save(&mut conn, settings).await?;
    tracing::info!(user_id = ctx.user_id, "the command settings changed");
    commands::announce(ctx.state);
    Ok(Response::Commands(commands::all(ctx.state)))
}
