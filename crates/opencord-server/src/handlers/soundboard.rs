//! Playing, editing and deleting soundboard sounds (Phase 2 plan §11).
//! Uploads come over HTTP (`media`).

use std::time::{Duration, Instant};

use opencord_common::permissions::Permissions;
use opencord_common::sound::{SoundRef, VOLUME_DEFAULT, VOLUME_MAX};
use opencord_common::validation;
use opencord_proto::v1 as proto;
use proto::event::Kind;
use proto::response::Result as Response;

use super::{Ctx, ack};
use crate::db::sounds;
use crate::error::ApiError;
use crate::permissions::{require_base, require_channel};
use crate::soundboard::{Sound, remove_file};
use crate::state::{Audience, now_ms};
use crate::voice::states::suppressed;

/// Checks everything the player's client checked, again (plan §11.5), and
/// tells everyone connected to the channel to play the sound.
pub async fn play(
    ctx: &Ctx<'_>,
    request: proto::PlaySoundboardSound,
) -> Result<Response, ApiError> {
    let sound_ref = SoundRef::parse(&request.sound_ref)
        .ok_or_else(|| ApiError::invalid_argument("that is not a sound"))?;
    let external = match sound_ref {
        SoundRef::External(hash) => {
            let mut conn = ctx.state.db.acquire().await?;
            sounds::external(&mut conn, &hash).await?
        }
        _ => None,
    };
    let effect = {
        let guild = ctx.state.guild();
        let settings = &guild.voice_settings;
        if !settings.soundboard_enabled {
            return Err(ApiError::forbidden(
                "the soundboard is turned off on this server",
            ));
        }
        let mut voice = ctx.state.voice();
        let state = voice
            .get(ctx.user_id)
            .filter(|state| state.channel_id == request.channel_id)
            .cloned()
            .ok_or_else(|| ApiError::voice_not_connected("you are"))?;
        let moderation = voice.moderation(ctx.user_id);
        if state.self_deaf || moderation.deaf {
            return Err(ApiError::forbidden("sounds can't be played while deafened"));
        }
        if moderation.mute {
            return Err(ApiError::forbidden(
                "sounds can't be played while server muted",
            ));
        }
        if suppressed(&guild, ctx.user_id, request.channel_id) {
            return Err(ApiError::forbidden("you can't speak in this channel"));
        }
        require_channel(
            &guild,
            ctx.user_id,
            request.channel_id,
            Permissions::USE_SOUNDBOARD,
        )?;
        let (name, emoji, volume, duration_ms) = match sound_ref {
            SoundRef::Server(hash) => {
                let sound = guild
                    .sounds
                    .values()
                    .find(|sound| sound.sha256 == hash)
                    .ok_or_else(|| ApiError::not_found("sound"))?;
                (
                    sound.name.clone(),
                    sound.emoji.clone(),
                    sound.volume,
                    sound.duration_ms,
                )
            }
            SoundRef::External(_) => {
                if !settings.allow_external_sounds {
                    return Err(ApiError::forbidden(
                        "sounds from other servers are turned off here",
                    ));
                }
                require_channel(
                    &guild,
                    ctx.user_id,
                    request.channel_id,
                    Permissions::USE_EXTERNAL_SOUNDS,
                )?;
                let row = external
                    .as_ref()
                    .ok_or_else(|| ApiError::not_found("sound"))?;
                (
                    validation::sound_name(request.name.as_deref().unwrap_or_default())?,
                    validation::sound_emoji(request.emoji.as_deref().unwrap_or_default())?,
                    check_volume(request.volume)?,
                    u32::try_from(row.duration_ms).unwrap_or(u32::MAX),
                )
            }
            SoundRef::Default(sound) => {
                if !settings.allow_default_sounds {
                    return Err(ApiError::forbidden("default sounds are turned off here"));
                }
                (
                    sound.name.to_owned(),
                    Some(sound.emoji.to_owned()),
                    VOLUME_DEFAULT,
                    sound.duration_ms,
                )
            }
        };
        voice.sounds.start(
            Instant::now(),
            ctx.user_id,
            request.channel_id,
            Duration::from_secs(u64::from(settings.sound_cooldown_s)),
            Duration::from_millis(u64::from(duration_ms)),
        )?;
        proto::VoiceChannelEffect {
            channel_id: request.channel_id,
            user_id: ctx.user_id,
            sound_ref: sound_ref.to_string(),
            name,
            emoji,
            volume,
            started_at_ms: now_ms(),
        }
    };
    if let SoundRef::External(hash) = sound_ref {
        let mut conn = ctx.state.db.acquire().await?;
        sounds::touch_external(&mut conn, &hash, now_ms()).await?;
    }
    ctx.state.broadcast(
        Kind::VoiceChannelEffect(effect),
        Audience::Voice(request.channel_id),
    );
    Ok(ack())
}

pub async fn update(
    ctx: &Ctx<'_>,
    request: proto::UpdateSoundboardSound,
) -> Result<Response, ApiError> {
    let _writes = ctx.state.write_lock().await;
    let updated = {
        let guild = ctx.state.guild();
        require_base(&guild, ctx.user_id, Permissions::MANAGE_SOUNDBOARD)?;
        let current = guild
            .sounds
            .get(&request.sound_id)
            .ok_or_else(|| ApiError::not_found("sound"))?;
        Sound {
            name: match request.name.as_deref() {
                Some(name) => validation::sound_name(name)?,
                None => current.name.clone(),
            },
            emoji: match request.emoji.as_deref() {
                Some(emoji) => validation::sound_emoji(emoji)?,
                None => current.emoji.clone(),
            },
            volume: match request.volume {
                Some(volume) => check_volume(Some(volume))?,
                None => current.volume,
            },
            ..current.clone()
        }
    };
    let mut conn = ctx.state.db.acquire().await?;
    sounds::update(
        &mut conn,
        updated.id,
        &updated.name,
        updated.emoji.as_deref(),
        i64::from(updated.volume),
    )
    .await?;
    ctx.state
        .guild_mut()
        .sounds
        .insert(updated.id, updated.clone());
    let shown = updated.to_proto();
    ctx.state.broadcast(
        Kind::SoundboardSoundUpdate(proto::SoundboardSoundUpdate {
            sound: Some(shown.clone()),
        }),
        Audience::Everyone,
    );
    Ok(Response::SoundboardSound(shown))
}

pub async fn delete(
    ctx: &Ctx<'_>,
    request: proto::DeleteSoundboardSound,
) -> Result<Response, ApiError> {
    let _writes = ctx.state.write_lock().await;
    let hash = {
        let guild = ctx.state.guild();
        require_base(&guild, ctx.user_id, Permissions::MANAGE_SOUNDBOARD)?;
        guild
            .sounds
            .get(&request.sound_id)
            .ok_or_else(|| ApiError::not_found("sound"))?
            .sha256
    };
    let mut conn = ctx.state.db.acquire().await?;
    sounds::delete(&mut conn, request.sound_id).await?;
    let still_used = {
        let mut guild = ctx.state.guild_mut();
        guild.sounds.remove(&request.sound_id);
        guild.sounds.values().any(|sound| sound.sha256 == hash)
    };
    // The same file may also be another server's sound played here.
    if !still_used && sounds::external(&mut conn, &hash).await?.is_none() {
        remove_file(&ctx.state.config.server.data_dir, &hash).await;
    }
    ctx.state.broadcast(
        Kind::SoundboardSoundDelete(proto::SoundboardSoundDelete {
            sound_id: request.sound_id,
        }),
        Audience::Everyone,
    );
    Ok(ack())
}

/// Absent means the default, 100 %.
pub fn check_volume(volume: Option<u32>) -> Result<u32, ApiError> {
    match volume {
        None => Ok(VOLUME_DEFAULT),
        Some(volume) if volume <= VOLUME_MAX => Ok(volume),
        Some(_) => Err(ApiError::invalid_argument(format!(
            "a sound's volume is 0 to {VOLUME_MAX} %"
        ))),
    }
}
