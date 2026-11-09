//! Chat command requests (Phase 3 plan §17.5): running a command, its
//! autocomplete, and the Commands page.

use std::time::Instant;

use opencord_common::permissions::Permissions;
use opencord_common::snowflake;
use opencord_plugins::limits::{AUTOCOMPLETE_TIME, COMMAND_REPLY_TIME};
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
use crate::plugins::command_calls::CommandAnswer;
use crate::plugins::permissions::PluginPermission;
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

    fn settings_key(&self) -> String {
        match self {
            Self::Builtin(s) => commands::settings_key(s.feature, s.name),
            Self::Plugin(p) => commands::settings_key(&p.spec.feature, &p.spec.name),
        }
    }
}

fn not_a_command(name: &str) -> ApiError {
    ApiError::new(
        proto::ErrorCode::CommandUnknown,
        format!("/{name} isn't a command here"),
    )
}

/// The command `name` in `source`. With no source (an older app, or a name
/// typed without picking) Opencord's runs if it's set up here, then the
/// only plugin with the name; two plugins are never guessed between.
fn resolve(ctx: &Ctx<'_>, name: &str, source: &str) -> Result<UsableCommand, ApiError> {
    if let Some(plugin_id) = source.strip_prefix(commands::PLUGIN_SOURCE_PREFIX) {
        return ctx
            .state
            .commands
            .find_plugin(plugin_id, name)
            .map(UsableCommand::Plugin)
            .ok_or_else(|| not_a_command(name));
    }
    if !source.is_empty() {
        return Err(not_a_command(name));
    }
    let builtin = commands::find(name);
    let mut plugins = ctx.state.commands.plugins_named(name);
    if let Some(spec) = builtin
        && (plugins.is_empty() || commands::feature_ready(ctx.state, spec.feature))
    {
        return Ok(UsableCommand::Builtin(spec));
    }
    match plugins.len() {
        0 => Err(not_a_command(name)),
        1 => Ok(UsableCommand::Plugin(plugins.remove(0))),
        _ => Err(ApiError::new(
            proto::ErrorCode::CommandUnknown,
            format!("Two commands are called /{name}. Pick one from the list."),
        )),
    }
}

/// The command, on, and allowed in `channel_id`.
fn usable(
    ctx: &Ctx<'_>,
    name: &str,
    source: &str,
    channel_id: i64,
) -> Result<UsableCommand, ApiError> {
    let name = name.trim_start_matches('/').to_ascii_lowercase();
    let command = resolve(ctx, &name, source)?;
    let settings = ctx.state.commands.settings();
    let key = command.settings_key();
    if !settings.enabled(&key) {
        return Err(ApiError::new(
            proto::ErrorCode::CommandDisabled,
            format!("/{} is turned off on this server", command.name()),
        ));
    }
    if !settings.allowed_in(&key, channel_id) {
        return Err(ApiError::new(
            proto::ErrorCode::CommandNotAllowedHere,
            format!("/{} can't be used in this channel", command.name()),
        ));
    }
    Ok(command)
}

/// The key `name` in `source` has on the Commands page, if it exists.
fn settings_key_of(ctx: &Ctx<'_>, name: &str, source: &str) -> Option<String> {
    if let Some(plugin_id) = source.strip_prefix(commands::PLUGIN_SOURCE_PREFIX) {
        let command = ctx.state.commands.find_plugin(plugin_id, name)?;
        return Some(commands::settings_key(
            &command.spec.feature,
            &command.spec.name,
        ));
    }
    if !source.is_empty() {
        return None;
    }
    commands::find(name).map(|spec| commands::settings_key(spec.feature, spec.name))
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
    let command = usable(ctx, &request.name, &request.source, request.channel_id)?;
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
                        Some(messages::CommandRun {
                            name: spec.name,
                            user_id: ctx.user_id,
                            state: proto::CommandState::Answered as i32,
                        }),
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
                            state: proto::CommandState::Answered as i32,
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
                .is_some_and(|h| h.holds(PluginPermission::RegisterCommands));
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
                reply: None,
                command: Some(tx),
            };
            ctx.state.plugins.deliver(&plugin_cmd.plugin_id, event);

            let unavailable = |what: &str| {
                let name = plugin_name(ctx, &plugin_cmd.plugin_id);
                ApiError::new(
                    proto::ErrorCode::PluginUnavailable,
                    format!("{name} {what}"),
                )
            };
            match tokio::time::timeout(COMMAND_REPLY_TIME, rx).await {
                Ok(Ok(CommandAnswer::Reply { content, only_you })) => {
                    post_reply(ctx, &request, &plugin_cmd, content, only_you).await
                }
                Ok(Ok(CommandAnswer::Failed)) => Err(unavailable("ran into an error")),
                _ => Err(unavailable("didn't answer")),
            }
        }
    }
}

/// Whether the plugin may post in the channel, and as which user.
fn plugin_poster(ctx: &Ctx<'_>, plugin_id: &str, channel_id: i64) -> Option<i64> {
    let has_permission = ctx
        .state
        .plugins
        .get(plugin_id)
        .is_some_and(|handle| handle.holds(PluginPermission::SendMessages));
    if !has_permission {
        return None;
    }
    let guild = ctx.state.guild();
    let (&user_id, _) = guild
        .plugin_users
        .iter()
        .find(|(_, user)| user.plugin_id == plugin_id)?;
    guild
        .channel_permissions(user_id, channel_id)
        .contains(Permissions::SEND_MESSAGES)
        .then_some(user_id)
}

/// A plugin's reply: in the channel when it can post there and the member
/// is meant to share it, otherwise only to the member.
async fn post_reply(
    ctx: &Ctx<'_>,
    request: &proto::ExecuteCommand,
    plugin_cmd: &commands::PluginCommand,
    content: String,
    only_you: bool,
) -> Result<Response, ApiError> {
    let public = !only_you && !ctx.state.commands.settings().results_private;
    let poster = if public {
        plugin_poster(ctx, &plugin_cmd.plugin_id, request.channel_id)
    } else {
        None
    };
    let Some(author_id) = poster else {
        ephemeral(ctx, request.channel_id, &request.nonce, content);
        return Ok(Response::CommandResult(proto::CommandResult {
            message_id: None,
            ephemeral: true,
        }));
    };
    let id = ctx.state.ids.next_id();
    let state = proto::CommandState::Answered as i32;
    let mut conn = ctx.state.db.acquire().await?;
    messages::insert_result(
        &mut conn,
        id,
        request.channel_id,
        author_id,
        &content,
        Some(messages::CommandRun {
            name: &plugin_cmd.spec.name,
            user_id: ctx.user_id,
            state,
        }),
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
            state,
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

/// A reply only the sending session sees (§17.3); never stored.
fn ephemeral(ctx: &Ctx<'_>, channel_id: i64, nonce: &str, content: String) {
    if let Some(session) = ctx.state.sessions.get(ctx.session_id) {
        let event = proto::Event {
            kind: Some(Event::EphemeralMessage(proto::EphemeralMessage {
                channel_id,
                nonce: nonce.to_owned(),
                content,
                state: proto::CommandState::Answered as i32,
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
    let command = usable(ctx, &request.name, &request.source, request.channel_id)?;
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
                command: None,
            };
            ctx.state.plugins.deliver(&plugin_cmd.plugin_id, event);

            let choices = match tokio::time::timeout(AUTOCOMPLETE_TIME, rx).await {
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
        let key = settings_key_of(ctx, &name, &request.source)
            .ok_or_else(|| ApiError::not_found("command"))?;
        if let Some(enabled) = request.enabled {
            settings.disabled.retain(|off| *off != key);
            if !enabled {
                settings.disabled.push(key.clone());
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
                settings.channels.remove(&key);
            } else {
                settings.channels.insert(key, channels.ids);
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
