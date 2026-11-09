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

enum UsableCommand {
    Builtin(&'static commands::Spec),
    Plugin(commands::PluginCommand),
}

impl UsableCommand {
    fn name(&self) -> &str {
        match self {
            Self::Builtin(s) => s.name,
            Self::Plugin(p) => &p.spec.name,
        }
    }
}

/// The command, on, and allowed in `channel_id`.
fn usable(ctx: &Ctx<'_>, name: &str, channel_id: i64) -> Result<UsableCommand, ApiError> {
    let name_trimmed = name.trim_start_matches('/').to_ascii_lowercase();
    let command = if let Some(spec) = commands::find(&name_trimmed) {
        UsableCommand::Builtin(spec)
    } else if let Some(plugin_cmd) = ctx.state.commands.find_plugin(&name_trimmed) {
        UsableCommand::Plugin(plugin_cmd)
    } else {
        return Err(ApiError::new(
            proto::ErrorCode::CommandUnknown,
            format!("/{name_trimmed} isn't a command here"),
        ));
    };

    let settings = ctx.state.commands.settings();
    if !settings.enabled(command.name()) {
        return Err(ApiError::new(
            proto::ErrorCode::CommandDisabled,
            format!("/{} is turned off on this server", command.name()),
        ));
    }
    if !settings.allowed_in(command.name(), channel_id) {
        return Err(ApiError::new(
            proto::ErrorCode::CommandNotAllowedHere,
            format!("/{} can't be used in this channel", command.name()),
        ));
    }
    Ok(command)
}

fn parse_plugin_options(
    cmd_name: &str,
    specs: &[proto::CommandOptionSpec],
    sent: &[proto::CommandOptionValue],
) -> Result<serde_json::Map<String, serde_json::Value>, ApiError> {
    let mut values = serde_json::Map::new();
    for option in sent {
        let known = specs
            .iter()
            .find(|s| s.name == option.name)
            .ok_or_else(|| {
                ApiError::new(
                    proto::ErrorCode::CommandInvalidOptions,
                    format!("/{cmd_name} has no option {}", option.name),
                )
            })?;
        let value = option.value.trim();
        if value.is_empty() {
            continue;
        }
        if value.chars().count() > crate::commands::options::VALUE_MAX {
            return Err(ApiError::new(
                proto::ErrorCode::CommandInvalidOptions,
                format!("{} is too long", known.name),
            ));
        }
        let kind = proto::CommandOptionKind::try_from(known.kind)
            .unwrap_or(proto::CommandOptionKind::Unspecified);
        let json_val = match kind {
            proto::CommandOptionKind::Integer => {
                let number: i64 = value.parse().map_err(|_| {
                    ApiError::new(
                        proto::ErrorCode::CommandInvalidOptions,
                        format!("{} is a whole number", known.name),
                    )
                })?;
                let (min, max) = (known.min, known.max);
                if (min, max) != (0, 0) && !(min..=max).contains(&number) {
                    return Err(ApiError::new(
                        proto::ErrorCode::CommandInvalidOptions,
                        format!("{} is from {min} to {max}", known.name),
                    ));
                }
                serde_json::json!(number)
            }
            proto::CommandOptionKind::Boolean => {
                let b = crate::commands::options::yes_or_no(value).ok_or_else(|| {
                    ApiError::new(
                        proto::ErrorCode::CommandInvalidOptions,
                        format!("{} is yes or no", known.name),
                    )
                })?;
                serde_json::json!(b)
            }
            proto::CommandOptionKind::Choice => {
                if !known.choices.iter().any(|c| c.value == value) {
                    let all: Vec<&str> = known.choices.iter().map(|c| c.value.as_str()).collect();
                    return Err(ApiError::new(
                        proto::ErrorCode::CommandInvalidOptions,
                        format!("{} is one of {}", known.name, all.join(", ")),
                    ));
                }
                serde_json::json!(value)
            }
            proto::CommandOptionKind::User | proto::CommandOptionKind::Channel => {
                value.parse::<i64>().map_err(|_| {
                    ApiError::new(
                        proto::ErrorCode::CommandInvalidOptions,
                        format!("{} isn't one", known.name),
                    )
                })?;
                serde_json::json!(value)
            }
            proto::CommandOptionKind::Text | proto::CommandOptionKind::Unspecified => {
                serde_json::json!(value)
            }
        };
        if values.insert(known.name.clone(), json_val).is_some() {
            return Err(ApiError::new(
                proto::ErrorCode::CommandInvalidOptions,
                format!("{} is given twice", known.name),
            ));
        }
    }
    for opt in specs.iter().filter(|o| o.required) {
        if !values.contains_key(&opt.name) {
            return Err(ApiError::new(
                proto::ErrorCode::CommandInvalidOptions,
                format!("/{cmd_name} needs {}", opt.name),
            ));
        }
    }
    Ok(values)
}

fn plugin_name(ctx: &Ctx<'_>, plugin_id: &str) -> String {
    ctx.state
        .guild()
        .plugin_users
        .values()
        .find(|u| u.plugin_id == plugin_id)
        .map(|u| u.name.clone())
        .unwrap_or_else(|| plugin_id.to_owned())
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
    let command = usable(ctx, &request.name, request.channel_id)?;
    match command {
        UsableCommand::Builtin(spec) => {
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
        UsableCommand::Plugin(plugin_cmd) => {
            let has_reg = ctx
                .state
                .plugins
                .get(&plugin_cmd.plugin_id)
                .is_some_and(|h| {
                    h.holds(crate::plugins::permissions::PluginPermission::RegisterCommands)
                });
            if !has_reg {
                let name = plugin_name(ctx, &plugin_cmd.plugin_id);
                return Err(ApiError::new(
                    proto::ErrorCode::PluginUnavailable,
                    format!("{name} is not permitted to handle commands"),
                ));
            }

            let options_map = parse_plugin_options(
                &plugin_cmd.spec.name,
                &plugin_cmd.spec.options,
                &request.options,
            )?;
            let payload = {
                let guild = ctx.state.guild();
                serde_json::json!({
                    "name": plugin_cmd.spec.name,
                    "options": serde_json::Value::Object(options_map),
                    "user": crate::plugins::events::user_table(&guild, ctx.user_id),
                    "channel_id": request.channel_id.to_string(),
                })
            };
            let (tx, rx) = tokio::sync::oneshot::channel();
            let event = crate::plugins::PluginEvent {
                name: "command".to_owned(),
                payload,
                reply: Some(tx),
            };
            ctx.state.plugins.deliver(&plugin_cmd.plugin_id, event);

            let answer = match tokio::time::timeout(std::time::Duration::from_secs(3), rx).await {
                Ok(Ok(val)) => val,
                _ => {
                    let name = plugin_name(ctx, &plugin_cmd.plugin_id);
                    return Err(ApiError::new(
                        proto::ErrorCode::PluginUnavailable,
                        format!("{name} didn't answer"),
                    ));
                }
            };

            let content_raw = answer
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let content = match opencord_common::validation::message_content(content_raw) {
                Ok(c) => c,
                Err(_) => {
                    let name = plugin_name(ctx, &plugin_cmd.plugin_id);
                    return Err(ApiError::new(
                        proto::ErrorCode::PluginUnavailable,
                        format!("{name} sent an invalid reply"),
                    ));
                }
            };
            let only_you = answer
                .get("only_you")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let public_results = !ctx.state.commands.settings().results_private;
            if only_you || !public_results {
                ephemeral(ctx, request.channel_id, &request.nonce, content);
                return Ok(Response::CommandResult(proto::CommandResult {
                    message_id: None,
                    ephemeral: true,
                }));
            }

            // Check if plugin can send public messages in this channel
            let (has_perm, can_send_in_channel, plugin_user) = {
                let guild = ctx.state.guild();
                let has_perm = ctx
                    .state
                    .plugins
                    .get(&plugin_cmd.plugin_id)
                    .is_some_and(|h| {
                        h.holds(crate::plugins::permissions::PluginPermission::SendMessages)
                    });
                let plugin_user = guild
                    .plugin_users
                    .iter()
                    .find(|(_, u)| u.plugin_id == plugin_cmd.plugin_id)
                    .map(|(&id, _)| id);
                let can_send_in_channel = plugin_user.is_some_and(|user_id| {
                    guild
                        .channel_permissions(user_id, request.channel_id)
                        .contains(Permissions::SEND_MESSAGES)
                });
                (has_perm, can_send_in_channel, plugin_user)
            };

            if has_perm
                && can_send_in_channel
                && let Some(author_id) = plugin_user
            {
                let id = ctx.state.ids.next_id();
                let mut conn = ctx.state.db.acquire().await?;
                messages::insert_result(
                    &mut conn,
                    id,
                    request.channel_id,
                    author_id,
                    &content,
                    Some(&plugin_cmd.spec.name),
                )
                .await?;
                let message = proto::Message {
                    id,
                    channel_id: request.channel_id,
                    author_id,
                    content,
                    created_at_ms: snowflake::timestamp_ms(id),
                    edited_at_ms: None,
                    nonce: None,
                    interaction: Some(proto::MessageInteraction {
                        user_id: ctx.user_id,
                        command: plugin_cmd.spec.name.clone(),
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
            } else {
                ephemeral(ctx, request.channel_id, &request.nonce, content);
                Ok(Response::CommandResult(proto::CommandResult {
                    message_id: None,
                    ephemeral: true,
                }))
            }
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
    let command = usable(ctx, &request.name, request.channel_id)?;
    match command {
        UsableCommand::Builtin(spec) => {
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
        UsableCommand::Plugin(plugin_cmd) => {
            let autocompletes = plugin_cmd
                .spec
                .options
                .iter()
                .find(|o| o.name == request.option)
                .is_some_and(|o| o.autocomplete);
            if !autocompletes {
                return Ok(Response::CommandChoices(proto::CommandChoices {
                    choices: Vec::new(),
                }));
            }
            let payload = {
                let guild = ctx.state.guild();
                serde_json::json!({
                    "name": plugin_cmd.spec.name,
                    "option": request.option,
                    "value": request.value,
                    "user": crate::plugins::events::user_table(&guild, ctx.user_id),
                    "channel_id": request.channel_id.to_string(),
                })
            };
            let (tx, rx) = tokio::sync::oneshot::channel();
            let event = crate::plugins::PluginEvent {
                name: "command_autocomplete".to_owned(),
                payload,
                reply: Some(tx),
            };
            ctx.state.plugins.deliver(&plugin_cmd.plugin_id, event);

            let choices = match tokio::time::timeout(std::time::Duration::from_secs(1), rx).await {
                Ok(Ok(val)) => {
                    let mut list = Vec::new();
                    if let Some(choices_arr) = val.get("choices").and_then(|c| c.as_array()) {
                        for item in choices_arr {
                            if list.len() >= 25 {
                                break;
                            }
                            if let (Some(name), Some(val)) = (
                                item.get("name").and_then(|n| n.as_str()),
                                item.get("value").and_then(|v| v.as_str()),
                            ) {
                                list.push(proto::CommandChoice {
                                    name: name.to_owned(),
                                    value: val.to_owned(),
                                    artwork_id: String::new(),
                                });
                            }
                        }
                    }
                    list
                }
                _ => Vec::new(),
            };
            Ok(Response::CommandChoices(proto::CommandChoices { choices }))
        }
    }
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
        let name = request.name.trim_start_matches('/').to_ascii_lowercase();
        let exists =
            commands::find(&name).is_some() || ctx.state.commands.find_plugin(&name).is_some();
        if !exists {
            return Err(ApiError::not_found("command"));
        }
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
