use opencord_common::channel::ChannelKind;
use opencord_common::limits::NONCE_MAX_CHARS;
use opencord_common::permissions::Permissions;
use opencord_common::{snowflake, validation};
use opencord_proto::v1 as proto;
use proto::event::Kind as Event;
use proto::response::Result as Response;

use super::{Ctx, ack};
use crate::db::messages::{self, MessageRow};
use crate::error::ApiError;
use crate::permissions::require_channel;
use crate::state::{Audience, now_ms};

pub async fn send(ctx: &Ctx<'_>, request: proto::SendMessage) -> Result<Response, ApiError> {
    let content = validation::message_content(&request.content)?;
    if request.nonce.chars().count() > NONCE_MAX_CHARS {
        return Err(ApiError::invalid_argument(format!(
            "nonce must be at most {NONCE_MAX_CHARS} characters"
        )));
    }
    require_text_channel(
        ctx,
        request.channel_id,
        Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES,
    )?;
    ctx.state
        .rate_limits
        .check_message(ctx.user_id, request.channel_id)
        .map_err(ApiError::rate_limited)?;

    let id = ctx.state.ids.next_id();
    let mut conn = ctx.state.db.acquire().await?;
    messages::insert(&mut conn, id, request.channel_id, ctx.user_id, &content).await?;
    let message = proto::Message {
        id,
        channel_id: request.channel_id,
        author_id: ctx.user_id,
        content,
        created_at_ms: snowflake::timestamp_ms(id),
        edited_at_ms: None,
        nonce: (!request.nonce.is_empty()).then_some(request.nonce),
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
    let content = validation::message_content(&request.content)?;
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
    let edited_at = now_ms();
    messages::update_content(&mut conn, existing.id, &content, edited_at).await?;
    let message = to_proto(MessageRow {
        content,
        edited_at: Some(edited_at),
        ..existing
    });
    ctx.state.broadcast(
        Event::MessageUpdate(proto::MessageUpdate {
            message: Some(message.clone()),
        }),
        Audience::Channel(message.channel_id),
    );
    Ok(Response::Message(message))
}

pub async fn delete(ctx: &Ctx<'_>, request: proto::DeleteMessage) -> Result<Response, ApiError> {
    let mut conn = ctx.state.db.acquire().await?;
    let existing = messages::find(&mut conn, request.message_id)
        .await?
        .ok_or_else(|| ApiError::not_found("message"))?;
    let needed = if existing.author_id == ctx.user_id {
        Permissions::VIEW_CHANNEL
    } else {
        Permissions::VIEW_CHANNEL | Permissions::MANAGE_MESSAGES
    };
    require_channel(&ctx.state.guild(), ctx.user_id, existing.channel_id, needed)?;
    messages::mark_deleted(&mut conn, existing.id).await?;
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
    Ok(Response::Messages(proto::MessageList {
        messages: rows.into_iter().map(to_proto).collect(),
    }))
}

pub fn start_typing(ctx: &Ctx<'_>, request: proto::StartTyping) -> Result<Response, ApiError> {
    require_text_channel(
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

fn require_text_channel(
    ctx: &Ctx<'_>,
    channel_id: i64,
    needed: Permissions,
) -> Result<(), ApiError> {
    let guild = ctx.state.guild();
    let channel = require_channel(&guild, ctx.user_id, channel_id, needed)?;
    if channel.kind == ChannelKind::Text {
        Ok(())
    } else {
        Err(ApiError::invalid_argument(
            "only text channels have messages",
        ))
    }
}

fn to_proto(row: MessageRow) -> proto::Message {
    proto::Message {
        id: row.id,
        channel_id: row.channel_id,
        author_id: row.author_id,
        content: row.content,
        created_at_ms: snowflake::timestamp_ms(row.id),
        edited_at_ms: row.edited_at,
        nonce: None,
    }
}
