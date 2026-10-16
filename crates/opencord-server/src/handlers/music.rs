//! Music requests (Phase 3 plan §16, §18.1). The server enforces every
//! permission and limit; clients only hide what a member can't do.

use opencord_common::channel::ChannelKind;
use opencord_common::permissions::Permissions;
use opencord_proto::v1 as proto;
use proto::response::Result as Response;

use super::Ctx;
use crate::db;
use crate::error::ApiError;
use crate::music::queue::{Item, Placement, Repeat};
use crate::music::session::{Caller, Command};
use crate::music::settings::QUEUE_MAX;
use crate::music::{self, search};
use crate::permissions::{missing, require_base, require_channel, require_outranks_member};
use crate::rate_limit::MusicLimit;
use crate::state::now_ms;

const REASON_MAX: usize = 200;

/// A voice channel the member can see; their permissions there.
fn voice_channel(ctx: &Ctx<'_>, channel_id: i64) -> Result<Permissions, ApiError> {
    let guild = ctx.state.guild();
    let channel = require_channel(&guild, ctx.user_id, channel_id, Permissions::VIEW_CHANNEL)?;
    if channel.kind != ChannelKind::Voice {
        return Err(ApiError::invalid_argument("music plays in voice channels"));
    }
    Ok(guild.channel_permissions(ctx.user_id, channel_id))
}

fn not_configured() -> ApiError {
    ApiError::new(
        proto::ErrorCode::MusicNotConfigured,
        "Music isn't set up on this server",
    )
}

/// A member acting on a channel's music: counted toward `limit`, music set
/// up, not blocked, connected to the channel unless they hold
/// `MUSIC_MANAGE`, and holding `needed` there.
fn act(
    ctx: &Ctx<'_>,
    channel_id: i64,
    needed: Permissions,
    limit: MusicLimit,
) -> Result<Caller, ApiError> {
    ctx.state
        .rate_limits
        .check_music(limit, ctx.user_id)
        .map_err(ApiError::rate_limited)?;
    may_act(ctx, channel_id, needed)
}

/// [`act`] without counting it: music set up, not blocked, connected (or
/// `MUSIC_MANAGE`) and holding `needed`.
pub(crate) fn may_act(
    ctx: &Ctx<'_>,
    channel_id: i64,
    needed: Permissions,
) -> Result<Caller, ApiError> {
    if !music::configured(ctx.state) {
        return Err(not_configured());
    }
    let permissions = voice_channel(ctx, channel_id)?;
    if ctx.state.music.is_blocked(ctx.user_id) {
        return Err(ApiError::new(
            proto::ErrorCode::MusicBlocked,
            "You're blocked from music on this server",
        ));
    }
    let connected = ctx
        .state
        .voice()
        .get(ctx.user_id)
        .is_some_and(|voice| voice.channel_id == channel_id);
    if !connected && !permissions.contains(Permissions::MUSIC_MANAGE) {
        return Err(ApiError::new(
            proto::ErrorCode::MusicNotInChannel,
            "Join the voice channel to control its music",
        ));
    }
    if !permissions.contains(needed) {
        return Err(missing(needed - permissions));
    }
    Ok(Caller {
        user_id: ctx.user_id,
        permissions,
    })
}

fn session(session: proto::MusicSession) -> Response {
    Response::MusicSession(Box::new(session))
}

/// Asks the channel's session; answers with its state after.
async fn ask(
    ctx: &Ctx<'_>,
    channel_id: i64,
    command: impl FnOnce(music::session::Reply) -> Command,
) -> Result<Response, ApiError> {
    ctx.state
        .music
        .ask(ctx.state, channel_id, command)
        .await?
        .map(session)
}

pub async fn subscribe(
    ctx: &Ctx<'_>,
    request: proto::MusicSubscribe,
) -> Result<Response, ApiError> {
    if !music::supported(ctx.state) {
        return Err(not_configured());
    }
    voice_channel(ctx, request.channel_id)?;
    ctx.state
        .music
        .subscribe(request.channel_id, ctx.session_id, request.subscribe);
    let snapshot = ctx
        .state
        .music
        .ask(ctx.state, request.channel_id, |reply| Command::Snapshot {
            reply,
        })
        .await?;
    Ok(session(snapshot))
}

pub async fn search(ctx: &Ctx<'_>, request: proto::MusicSearch) -> Result<Response, ApiError> {
    act(
        ctx,
        request.channel_id,
        Permissions::MUSIC_ADD,
        MusicLimit::Search,
    )?;
    let results = search::search(ctx.state, ctx.user_id, &request).await?;
    Ok(Response::MusicSearchResults(results))
}

pub async fn enqueue(ctx: &Ctx<'_>, request: proto::MusicEnqueue) -> Result<Response, ApiError> {
    let caller = act(
        ctx,
        request.channel_id,
        Permissions::MUSIC_ADD,
        MusicLimit::Enqueue,
    )?;
    let placement = match (
        proto::MusicPlacement::try_from(request.placement).unwrap_or_default(),
        request.before_item_id,
    ) {
        (proto::MusicPlacement::Next, _) => Placement::Next,
        (proto::MusicPlacement::Now, _) => Placement::Now,
        (_, Some(before)) => Placement::Before(before),
        (_, None) => Placement::End,
    };
    if placement != Placement::End && !caller.permissions.contains(Permissions::MUSIC_MANAGE_QUEUE)
    {
        return Err(missing(
            Permissions::MUSIC_MANAGE_QUEUE - caller.permissions,
        ));
    }
    let count = request.result_ids.len() + request.history_item_ids.len();
    if count == 0 || count > QUEUE_MAX as usize {
        return Err(ApiError::invalid_argument(
            "add 1 to 500 search results at once",
        ));
    }
    let mut tracks = ctx
        .state
        .music
        .results
        .tracks(ctx.user_id, &request.result_ids)?;
    if !request.history_item_ids.is_empty() {
        let again = ctx
            .state
            .music
            .ask(ctx.state, request.channel_id, |reply| Command::History {
                item_ids: request.history_item_ids.clone(),
                reply,
            })
            .await?;
        if again.len() != request.history_item_ids.len() {
            return Err(ApiError::not_found(
                "recently played track (it's no longer in the history)",
            ));
        }
        tracks.extend(again);
    }
    let settings = ctx.state.music.settings();
    let manage = caller.permissions.contains(Permissions::MUSIC_MANAGE);
    for track in &tracks {
        let info = &track.info;
        if track.is_unsafe_source() && !ctx.state.config.music.allow_unsafe_sources {
            return Err(ApiError::new(
                proto::ErrorCode::MusicSourceNotAllowed,
                "Links to files and other addresses can't be played here",
            ));
        }
        if info.is_stream && !settings.allow_streams {
            return Err(ApiError::new(
                proto::ErrorCode::MusicStreamsNotAllowed,
                format!("Live streams can't be played here: '{}'", info.title),
            ));
        }
        if !manage
            && !info.is_stream
            && settings.max_track_ms > 0
            && info.length_ms > settings.max_track_ms
        {
            return Err(ApiError::new(
                proto::ErrorCode::MusicTrackTooLong,
                format!(
                    "'{}' is longer than the {} minutes allowed",
                    info.title,
                    settings.max_track_ms / 60_000
                ),
            ));
        }
    }
    let items = tracks
        .into_iter()
        .map(|track| Item {
            id: ctx.state.ids.next_id().to_string(),
            track,
            requester_id: ctx.user_id,
            added_at_ms: now_ms(),
        })
        .collect();
    ask(ctx, request.channel_id, |reply| Command::Enqueue {
        caller,
        items,
        placement,
        reply,
    })
    .await
}

pub async fn playback(ctx: &Ctx<'_>, request: proto::MusicPlayback) -> Result<Response, ApiError> {
    let caller = act(
        ctx,
        request.channel_id,
        Permissions::empty(),
        MusicLimit::Control,
    )?;
    let action = proto::MusicAction::try_from(request.action).unwrap_or_default();
    ask(ctx, request.channel_id, |reply| Command::Playback {
        caller,
        action,
        position_ms: request.position_ms,
        expected_item_id: request.expected_item_id,
        reply,
    })
    .await
}

pub async fn set_mode(ctx: &Ctx<'_>, request: proto::MusicSetMode) -> Result<Response, ApiError> {
    act(
        ctx,
        request.channel_id,
        Permissions::MUSIC_CONTROL,
        MusicLimit::Control,
    )?;
    let repeat = request
        .repeat
        .map(|repeat| {
            Repeat::from_proto(repeat).ok_or_else(|| ApiError::invalid_argument("unknown repeat"))
        })
        .transpose()?;
    ask(ctx, request.channel_id, |reply| Command::SetMode {
        repeat,
        shuffle: request.shuffle,
        reply,
    })
    .await
}

pub async fn queue_move(
    ctx: &Ctx<'_>,
    request: proto::MusicQueueMove,
) -> Result<Response, ApiError> {
    act(
        ctx,
        request.channel_id,
        Permissions::MUSIC_MANAGE_QUEUE,
        MusicLimit::Control,
    )?;
    ask(ctx, request.channel_id, |reply| Command::Move {
        item_id: request.item_id,
        before_item_id: request.before_item_id,
        reply,
    })
    .await
}

pub async fn queue_remove(
    ctx: &Ctx<'_>,
    request: proto::MusicQueueRemove,
) -> Result<Response, ApiError> {
    let caller = act(
        ctx,
        request.channel_id,
        Permissions::empty(),
        MusicLimit::Control,
    )?;
    if request.item_ids.is_empty() || request.item_ids.len() > QUEUE_MAX as usize {
        return Err(ApiError::invalid_argument("remove 1 to 500 items at once"));
    }
    ask(ctx, request.channel_id, |reply| Command::Remove {
        caller,
        item_ids: request.item_ids,
        reply,
    })
    .await
}

pub async fn queue_clear(
    ctx: &Ctx<'_>,
    request: &proto::MusicQueueClear,
) -> Result<Response, ApiError> {
    act(
        ctx,
        request.channel_id,
        Permissions::MUSIC_MANAGE_QUEUE,
        MusicLimit::Control,
    )?;
    ask(ctx, request.channel_id, |reply| Command::Clear { reply }).await
}

/// The settings with what is available now: search sources at least one
/// connected node offers, and whether this server runs music at all.
fn settings_proto(ctx: &Ctx<'_>) -> proto::MusicSettings {
    music::settings_proto(ctx.state)
}

pub fn fetch_settings(ctx: &Ctx<'_>) -> Response {
    Response::MusicSettings(Box::new(settings_proto(ctx)))
}

pub async fn update_settings(
    ctx: &Ctx<'_>,
    request: proto::UpdateMusicSettings,
) -> Result<Response, ApiError> {
    require_base(&ctx.state.guild(), ctx.user_id, Permissions::MUSIC_MANAGE)?;
    let _writes = ctx.state.write_lock().await;
    let before = ctx.state.music.settings();
    let domains = ctx.state.domains.all();
    let after = before.apply(&request, |host| {
        domains.iter().any(|domain| {
            domain.active && domain.hostname == host && domain.trusted_certificate(ctx.state)
        })
    })?;
    after.save(&mut *ctx.state.db.acquire().await?).await?;
    ctx.state.music.set_settings(after.clone());
    tracing::info!(
        user_id = ctx.user_id,
        enabled = after.enabled,
        "the music settings changed"
    );
    if before.enabled && !after.enabled {
        music::end(ctx.state, None);
    }
    Ok(Response::MusicSettings(Box::new(music::announce_settings(
        ctx.state,
    ))))
}

pub fn fetch_blocks(ctx: &Ctx<'_>) -> Result<Response, ApiError> {
    require_base(&ctx.state.guild(), ctx.user_id, Permissions::MUSIC_MANAGE)?;
    Ok(Response::MusicBlocks(ctx.state.music.blocks()))
}

pub async fn block_member(
    ctx: &Ctx<'_>,
    request: proto::MusicBlockMember,
) -> Result<Response, ApiError> {
    {
        let guild = ctx.state.guild();
        require_base(&guild, ctx.user_id, Permissions::MUSIC_MANAGE)?;
        if !guild.members.contains_key(&request.user_id) {
            return Err(ApiError::not_found("member"));
        }
        if request.user_id == ctx.user_id {
            return Err(ApiError::invalid_argument("you can't block yourself"));
        }
        if request.blocked {
            require_outranks_member(&guild, ctx.user_id, request.user_id)?;
        }
    }
    let reason = request.reason.trim();
    if reason.chars().count() > REASON_MAX {
        return Err(ApiError::invalid_argument(
            "a reason is at most 200 characters",
        ));
    }
    let mut conn = ctx.state.db.acquire().await?;
    if request.blocked {
        let row = db::music::BlockRow {
            user_id: request.user_id,
            reason: reason.to_owned(),
            blocked_by: ctx.user_id,
            created_at: now_ms(),
        };
        db::music::put_block(&mut conn, &row).await?;
        ctx.state.music.set_block(request.user_id, Some(row));
    } else {
        db::music::delete_block(&mut conn, request.user_id).await?;
        ctx.state.music.set_block(request.user_id, None);
    }
    tracing::info!(
        user_id = ctx.user_id,
        member = request.user_id,
        blocked = request.blocked,
        "a member's music block changed"
    );
    Ok(Response::MusicBlocks(ctx.state.music.blocks()))
}
