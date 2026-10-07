use opencord_common::channel::ChannelKind;
use opencord_common::limits::{
    CHANNEL_BITRATE_DEFAULT, CHANNEL_BITRATE_MIN, MAX_CHANNELS, USER_LIMIT_MAX,
};
use opencord_common::permissions::{Overwrite, OverwriteTarget, Permissions};
use opencord_common::validation;
use opencord_proto::v1 as proto;
use proto::response::Result as Response;

use super::settings::save_voice_settings;
use super::{Ctx, ack};
use crate::db::channels;
use crate::error::ApiError;
use crate::guild::{Channel, Guild, overwrite_row, overwrite_target_to_db};
use crate::permissions::{require_base, require_channel, require_grantable};
use crate::state::now_ms;
use crate::visibility::Visibility;
use crate::voice::settings::VoiceSettings;

pub async fn create(ctx: &Ctx<'_>, request: proto::CreateChannel) -> Result<Response, ApiError> {
    let kind = kind_from_proto(request.kind)?;
    let name = validation::channel_name(kind, &request.name)?;
    let topic = optional_topic(request.topic.as_deref())?;
    let parent_id = request.parent_id.filter(|id| *id != 0);

    let _writes = ctx.state.write_lock().await;
    let position = {
        let guild = ctx.state.guild();
        require_base(&guild, ctx.user_id, Permissions::MANAGE_CHANNELS)?;
        if guild.channels.len() >= MAX_CHANNELS {
            return Err(ApiError::conflict(format!(
                "a server can have at most {MAX_CHANNELS} channels"
            )));
        }
        require_valid_parent(&guild, kind, parent_id, None)?;
        guild
            .channels
            .values()
            .filter(|channel| channel.parent_id == parent_id)
            .map(|channel| channel.position + 1)
            .max()
            .unwrap_or(0)
    };
    let channel = Channel {
        id: ctx.state.ids.next_id(),
        kind,
        name,
        topic,
        parent_id,
        position,
        overwrites: Vec::new(),
        bitrate: CHANNEL_BITRATE_DEFAULT,
        user_limit: 0,
        text_in_voice: true,
    };
    channels::insert(
        &mut *ctx.state.db.acquire().await?,
        &channel.to_row(),
        now_ms(),
    )
    .await?;

    let before = Visibility::capture(ctx.state);
    ctx.state
        .guild_mut()
        .channels
        .insert(channel.id, channel.clone());
    before.announce(ctx.state, &[]);
    Ok(Response::Channel(channel.to_proto()))
}

pub async fn update(ctx: &Ctx<'_>, request: proto::UpdateChannel) -> Result<Response, ApiError> {
    let _writes = ctx.state.write_lock().await;
    let updated = {
        let guild = ctx.state.guild();
        let channel = require_channel(
            &guild,
            ctx.user_id,
            request.channel_id,
            Permissions::MANAGE_CHANNELS,
        )?;
        let name = match &request.name {
            Some(name) => validation::channel_name(channel.kind, name)?,
            None => channel.name.clone(),
        };
        let topic = match &request.topic {
            Some(topic) => validation::topic(topic)?,
            None => channel.topic.clone(),
        };
        let parent_id = match request.parent_id {
            None => channel.parent_id,
            Some(0) => None,
            Some(parent_id) => {
                require_valid_parent(&guild, channel.kind, Some(parent_id), Some(channel.id))?;
                Some(parent_id)
            }
        };
        let voice_change = request.bitrate.is_some()
            || request.user_limit.is_some()
            || request.text_in_voice.is_some();
        if voice_change && channel.kind != ChannelKind::Voice {
            return Err(ApiError::invalid_argument(
                "only voice channels have a bitrate, a user limit and voice chat",
            ));
        }
        let max_bitrate = guild.voice_settings.max_voice_bitrate;
        if let Some(bitrate) = request.bitrate
            && !(CHANNEL_BITRATE_MIN..=max_bitrate).contains(&bitrate)
        {
            return Err(ApiError::invalid_argument(format!(
                "the bitrate must be from {CHANNEL_BITRATE_MIN} to {max_bitrate} bits per second"
            )));
        }
        if request
            .user_limit
            .is_some_and(|limit| limit > USER_LIMIT_MAX)
        {
            return Err(ApiError::invalid_argument(format!(
                "the user limit must be from 0 to {USER_LIMIT_MAX}"
            )));
        }
        Channel {
            name,
            topic,
            parent_id,
            bitrate: request.bitrate.unwrap_or(channel.bitrate),
            user_limit: request.user_limit.unwrap_or(channel.user_limit),
            text_in_voice: request.text_in_voice.unwrap_or(channel.text_in_voice),
            ..channel.clone()
        }
    };
    channels::update(&mut *ctx.state.db.acquire().await?, &updated.to_row()).await?;
    replace_and_announce(ctx, vec![updated.clone()]);
    Ok(Response::Channel(updated.to_proto()))
}

pub async fn delete(ctx: &Ctx<'_>, request: proto::DeleteChannel) -> Result<Response, ApiError> {
    let _writes = ctx.state.write_lock().await;
    let children: Vec<i64> = {
        let guild = ctx.state.guild();
        require_channel(
            &guild,
            ctx.user_id,
            request.channel_id,
            Permissions::MANAGE_CHANNELS,
        )?;
        guild
            .channels
            .values()
            .filter(|channel| channel.parent_id == Some(request.channel_id))
            .map(|channel| channel.id)
            .collect()
    };
    channels::delete(&mut *ctx.state.db.acquire().await?, request.channel_id).await?;

    let before = Visibility::capture(ctx.state);
    {
        let mut guild = ctx.state.guild_mut();
        guild.channels.remove(&request.channel_id);
        for child_id in &children {
            if let Some(child) = guild.channels.get_mut(child_id) {
                child.parent_id = None;
            }
        }
    }
    before.announce(ctx.state, &children);
    let afk_cleared = {
        let guild = ctx.state.guild();
        (guild.voice_settings.afk_channel_id == Some(request.channel_id)).then(|| VoiceSettings {
            afk_channel_id: None,
            ..guild.voice_settings.clone()
        })
    };
    if let Some(settings) = afk_cleared {
        save_voice_settings(ctx.state, settings).await?;
    }
    Ok(ack())
}

pub async fn reorder(ctx: &Ctx<'_>, request: proto::ReorderChannels) -> Result<Response, ApiError> {
    if request.positions.is_empty() {
        return Err(ApiError::invalid_argument("no channels to reorder"));
    }
    let _writes = ctx.state.write_lock().await;
    let moved: Vec<Channel> = {
        let guild = ctx.state.guild();
        require_base(&guild, ctx.user_id, Permissions::MANAGE_CHANNELS)?;
        request
            .positions
            .iter()
            .map(|position| {
                let channel = require_channel(
                    &guild,
                    ctx.user_id,
                    position.channel_id,
                    Permissions::VIEW_CHANNEL,
                )?;
                Ok(Channel {
                    position: position.position,
                    ..channel.clone()
                })
            })
            .collect::<Result<_, ApiError>>()?
    };
    let mut tx = ctx.state.db.begin().await?;
    for channel in &moved {
        channels::update(&mut tx, &channel.to_row()).await?;
    }
    tx.commit().await?;
    replace_and_announce(ctx, moved);
    Ok(ack())
}

pub async fn set_overwrite(
    ctx: &Ctx<'_>,
    request: proto::SetChannelOverwrite,
) -> Result<Response, ApiError> {
    let overwrite = request
        .overwrite
        .ok_or_else(|| ApiError::invalid_argument("the overwrite is missing"))?;
    let target = target_from_proto(overwrite.target_kind, overwrite.target_id)?;
    let allow = Permissions::from_bits_truncate(overwrite.allow);
    let deny = Permissions::from_bits_truncate(overwrite.deny);
    if allow.intersects(deny) {
        return Err(ApiError::invalid_argument(
            "a permission cannot be both allowed and denied",
        ));
    }

    let _writes = ctx.state.write_lock().await;
    let (updated, new_overwrite) = {
        let guild = ctx.state.guild();
        let channel = require_channel(
            &guild,
            ctx.user_id,
            request.channel_id,
            Permissions::MANAGE_ROLES,
        )?;
        let target_exists = match target {
            OverwriteTarget::Role(role_id) => guild.roles.contains_key(&role_id),
            OverwriteTarget::Member(user_id) => guild.members.contains_key(&user_id),
        };
        if !target_exists {
            return Err(ApiError::not_found("overwrite target"));
        }
        let held = guild.channel_permissions(ctx.user_id, channel.id);
        require_grantable(held, allow | deny)?;
        let new_overwrite = Overwrite {
            target,
            allow,
            deny,
        };
        let overwrites = channel
            .overwrites
            .iter()
            .filter(|existing| existing.target != target)
            .copied()
            .chain([new_overwrite])
            .collect();
        (
            Channel {
                overwrites,
                ..channel.clone()
            },
            new_overwrite,
        )
    };
    channels::upsert_overwrite(
        &mut *ctx.state.db.acquire().await?,
        &overwrite_row(updated.id, &new_overwrite),
    )
    .await?;
    replace_and_announce(ctx, vec![updated.clone()]);
    Ok(Response::Channel(updated.to_proto()))
}

pub async fn delete_overwrite(
    ctx: &Ctx<'_>,
    request: proto::DeleteChannelOverwrite,
) -> Result<Response, ApiError> {
    let target = target_from_proto(request.target_kind, request.target_id)?;
    let _writes = ctx.state.write_lock().await;
    let updated = {
        let guild = ctx.state.guild();
        let channel = require_channel(
            &guild,
            ctx.user_id,
            request.channel_id,
            Permissions::MANAGE_ROLES,
        )?;
        if !channel
            .overwrites
            .iter()
            .any(|existing| existing.target == target)
        {
            return Err(ApiError::not_found("overwrite"));
        }
        Channel {
            overwrites: channel
                .overwrites
                .iter()
                .filter(|existing| existing.target != target)
                .copied()
                .collect(),
            ..channel.clone()
        }
    };
    let (target_kind, target_id) = overwrite_target_to_db(target);
    channels::delete_overwrite(
        &mut *ctx.state.db.acquire().await?,
        updated.id,
        target_kind,
        target_id,
    )
    .await?;
    replace_and_announce(ctx, vec![updated.clone()]);
    Ok(Response::Channel(updated.to_proto()))
}

/// Puts the channels in the cache and tells everyone affected.
fn replace_and_announce(ctx: &Ctx<'_>, channels: Vec<Channel>) {
    let ids: Vec<i64> = channels.iter().map(|channel| channel.id).collect();
    let before = Visibility::capture(ctx.state);
    {
        let mut guild = ctx.state.guild_mut();
        for channel in channels {
            guild.channels.insert(channel.id, channel);
        }
    }
    before.announce(ctx.state, &ids);
}

fn require_valid_parent(
    guild: &Guild,
    kind: ChannelKind,
    parent_id: Option<i64>,
    channel_id: Option<i64>,
) -> Result<(), ApiError> {
    let Some(parent_id) = parent_id else {
        return Ok(());
    };
    if kind == ChannelKind::Category {
        return Err(ApiError::invalid_argument(
            "categories cannot be inside other categories",
        ));
    }
    if Some(parent_id) == channel_id {
        return Err(ApiError::invalid_argument(
            "a channel cannot be its own parent",
        ));
    }
    match guild.channels.get(&parent_id) {
        Some(parent) if parent.kind == ChannelKind::Category => Ok(()),
        Some(_) => Err(ApiError::invalid_argument("the parent must be a category")),
        None => Err(ApiError::not_found("category")),
    }
}

fn optional_topic(topic: Option<&str>) -> Result<Option<String>, ApiError> {
    Ok(topic.map(validation::topic).transpose()?.flatten())
}

fn kind_from_proto(kind: i32) -> Result<ChannelKind, ApiError> {
    match proto::ChannelKind::try_from(kind) {
        Ok(proto::ChannelKind::Text) => Ok(ChannelKind::Text),
        Ok(proto::ChannelKind::Voice) => Ok(ChannelKind::Voice),
        Ok(proto::ChannelKind::Category) => Ok(ChannelKind::Category),
        _ => Err(ApiError::invalid_argument("unknown channel kind")),
    }
}

fn target_from_proto(kind: i32, id: i64) -> Result<OverwriteTarget, ApiError> {
    match proto::OverwriteTarget::try_from(kind) {
        Ok(proto::OverwriteTarget::Role) => Ok(OverwriteTarget::Role(id)),
        Ok(proto::OverwriteTarget::Member) => Ok(OverwriteTarget::Member(id)),
        _ => Err(ApiError::invalid_argument("unknown overwrite target")),
    }
}
