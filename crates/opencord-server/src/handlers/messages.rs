use std::collections::{HashMap, HashSet};

use opencord_common::channel::ChannelKind;
use opencord_common::limits::NONCE_MAX_CHARS;
use opencord_common::permissions::Permissions;
use opencord_common::{snowflake, upload, validation};
use opencord_proto::v1 as proto;
use proto::event::Kind as Event;
use proto::response::Result as Response;

use super::{Ctx, ack};
use crate::attachments::http::attachment_to_proto;
use crate::db::attachments;
use crate::db::messages::{self, MessageRow};
use crate::error::ApiError;
use crate::permissions::require_channel;
use crate::state::{Audience, now_ms};

pub async fn send(ctx: &Ctx<'_>, request: proto::SendMessage) -> Result<Response, ApiError> {
    let ids = &request.attachment_ids;
    if !ids.is_empty() {
        if ids.len() > upload::MAX_ATTACHMENTS {
            return Err(ApiError::invalid_argument(
                "a message can have at most 10 files",
            ));
        }
        let mut seen = HashSet::new();
        for id in ids {
            if !seen.insert(*id) {
                return Err(ApiError::invalid_argument(
                    "the same file is attached twice",
                ));
            }
        }
    }

    let content = if ids.is_empty() {
        validation::message_content(&request.content)?
    } else {
        validation::message_content_or_empty(&request.content)?
    };

    if request.nonce.chars().count() > NONCE_MAX_CHARS {
        return Err(ApiError::invalid_argument(format!(
            "nonce must be at most {NONCE_MAX_CHARS} characters"
        )));
    }
    require_message_channel(
        ctx,
        request.channel_id,
        Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES,
    )?;
    ctx.state
        .rate_limits
        .check_message(ctx.user_id, request.channel_id)
        .map_err(ApiError::rate_limited)?;

    let mut rows = Vec::new();
    if !ids.is_empty() {
        let mut conn = ctx.state.db.acquire().await?;
        for &id in ids {
            let row = attachments::find(&mut conn, id)
                .await?
                .ok_or_else(|| ApiError::not_found("upload"))?;
            if row.uploader_id != ctx.user_id
                || row.state != "ready"
                || row.channel_id != request.channel_id
            {
                return Err(ApiError::not_found("upload"));
            }
            rows.push(row);
        }
    }

    {
        let guild = ctx.state.guild();
        if rows.iter().any(|r| r.kind == "voice") {
            if ids.len() != 1 || !content.is_empty() {
                return Err(ApiError::invalid_argument(
                    "a voice message is sent on its own",
                ));
            }
            require_channel(
                &guild,
                ctx.user_id,
                request.channel_id,
                Permissions::SEND_VOICE_MESSAGES,
            )?;
        } else if !ids.is_empty() {
            require_channel(
                &guild,
                ctx.user_id,
                request.channel_id,
                Permissions::ATTACH_FILES,
            )?;
        }
    }

    let id = ctx.state.ids.next_id();
    let mut tx = ctx.state.db.begin().await?;
    messages::insert(&mut tx, id, request.channel_id, ctx.user_id, &content).await?;
    if !ids.is_empty()
        && !attachments::mark_sent(&mut tx, ids, id, ctx.user_id, request.channel_id).await?
    {
        // Dropping tx rolls the message back.
        return Err(ApiError::not_found("upload"));
    }
    tx.commit().await?;

    let message_attachments = rows.iter().map(attachment_to_proto).collect();
    let message = proto::Message {
        id,
        channel_id: request.channel_id,
        author_id: ctx.user_id,
        content,
        created_at_ms: snowflake::timestamp_ms(id),
        edited_at_ms: None,
        nonce: (!request.nonce.is_empty()).then_some(request.nonce),
        interaction: None,
        attachments: message_attachments,
    };
    ctx.state.broadcast(
        Event::MessageCreate(proto::MessageCreate {
            message: Some(message.clone()),
        }),
        Audience::Channel(message.channel_id),
    );
    Ok(Response::Message(message))
}

pub async fn edit(ctx: &Ctx<'_>, request: proto::EditMessage) -> Result<Response, ApiError> {
    let mut conn = ctx.state.db.acquire().await?;
    let existing = messages::find(&mut conn, request.message_id)
        .await?
        .ok_or_else(|| ApiError::not_found("message"))?;
    require_channel(
        &ctx.state.guild(),
        ctx.user_id,
        existing.channel_id,
        Permissions::VIEW_CHANNEL,
    )?;
    if existing.author_id != ctx.user_id {
        return Err(ApiError::forbidden("you can only edit your own messages"));
    }

    let files = attachments::for_messages(&mut conn, &[existing.id]).await?;
    let content = if !files.is_empty() {
        validation::message_content_or_empty(&request.content)?
    } else {
        validation::message_content(&request.content)?
    };

    if files.iter().any(|f| f.kind == "voice") && !content.is_empty() {
        return Err(ApiError::invalid_argument("a voice message has no text"));
    }

    let edited_at = now_ms();
    messages::update_content(&mut conn, existing.id, &content, edited_at).await?;
    let message_attachments = files.iter().map(attachment_to_proto).collect();
    let message = to_proto(
        MessageRow {
            content,
            edited_at: Some(edited_at),
            ..existing
        },
        message_attachments,
    );
    ctx.state.broadcast(
        Event::MessageUpdate(proto::MessageUpdate {
            message: Some(message.clone()),
        }),
        Audience::Channel(message.channel_id),
    );
    Ok(Response::Message(message))
}

pub async fn delete(ctx: &Ctx<'_>, request: proto::DeleteMessage) -> Result<Response, ApiError> {
    let mut tx = ctx.state.db.begin().await?;
    let existing = messages::find(&mut tx, request.message_id)
        .await?
        .ok_or_else(|| ApiError::not_found("message"))?;
    let needed = if existing.author_id == ctx.user_id {
        Permissions::VIEW_CHANNEL
    } else {
        Permissions::VIEW_CHANNEL | Permissions::MANAGE_MESSAGES
    };
    require_channel(&ctx.state.guild(), ctx.user_id, existing.channel_id, needed)?;
    messages::mark_deleted(&mut tx, existing.id).await?;
    attachments::delete_for_message(&mut tx, existing.id).await?;
    tx.commit().await?;

    ctx.state.broadcast(
        Event::MessageDelete(proto::MessageDelete {
            channel_id: existing.channel_id,
            message_id: existing.id,
        }),
        Audience::Channel(existing.channel_id),
    );
    Ok(ack())
}

pub async fn fetch(ctx: &Ctx<'_>, request: proto::FetchMessages) -> Result<Response, ApiError> {
    require_channel(
        &ctx.state.guild(),
        ctx.user_id,
        request.channel_id,
        Permissions::VIEW_CHANNEL | Permissions::READ_HISTORY,
    )?;
    let limit = i64::from(validation::fetch_limit(request.limit));
    let before = request.before.unwrap_or(i64::MAX);
    let mut conn = ctx.state.db.acquire().await?;
    let rows = messages::list(&mut conn, request.channel_id, before, limit).await?;

    let ids: Vec<i64> = rows.iter().map(|row| row.id).collect();
    let mut files: HashMap<i64, Vec<proto::Attachment>> = HashMap::new();
    for file in attachments::for_messages(&mut conn, &ids).await? {
        if let Some(message_id) = file.message_id {
            files
                .entry(message_id)
                .or_default()
                .push(attachment_to_proto(&file));
        }
    }

    Ok(Response::Messages(proto::MessageList {
        messages: rows
            .into_iter()
            .map(|row| {
                let atts = files.remove(&row.id).unwrap_or_default();
                to_proto(row, atts)
            })
            .collect(),
    }))
}

pub fn start_typing(ctx: &Ctx<'_>, request: proto::StartTyping) -> Result<Response, ApiError> {
    require_message_channel(
        ctx,
        request.channel_id,
        Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES,
    )?;
    ctx.state.broadcast(
        Event::TypingStart(proto::TypingStart {
            channel_id: request.channel_id,
            user_id: ctx.user_id,
        }),
        Audience::Channel(request.channel_id),
    );
    Ok(ack())
}

pub(crate) fn require_message_channel(
    ctx: &Ctx<'_>,
    channel_id: i64,
    needed: Permissions,
) -> Result<(), ApiError> {
    let guild = ctx.state.guild();
    let channel = require_channel(&guild, ctx.user_id, channel_id, needed)?;
    // Voice channels hold a chat too (Phase 2 §12), as on Discord, unless
    // their settings turn it off (§5.3).
    match channel.kind {
        ChannelKind::Text => Ok(()),
        ChannelKind::Voice if channel.text_in_voice => Ok(()),
        ChannelKind::Voice => Err(ApiError::invalid_argument(
            "this voice channel's text chat is off",
        )),
        _ => Err(ApiError::invalid_argument(
            "only text and voice channels have messages",
        )),
    }
}

fn to_proto(row: MessageRow, attachments: Vec<proto::Attachment>) -> proto::Message {
    proto::Message {
        id: row.id,
        channel_id: row.channel_id,
        author_id: row.author_id,
        content: row.content,
        created_at_ms: snowflake::timestamp_ms(row.id),
        edited_at_ms: row.edited_at,
        nonce: None,
        interaction: row
            .interaction_command
            .map(|command| proto::MessageInteraction {
                user_id: row.author_id,
                command,
            }),
        attachments,
    }
}
