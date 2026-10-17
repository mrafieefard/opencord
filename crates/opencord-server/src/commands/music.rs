//! The music commands (Phase 3 plan §17.2). Each runs the same request as
//! the music panel, so the same permissions, limits and rate limits apply.
//! They act on the voice channel the member is connected to on this
//! server; with Manage music, `channel` names another.

use opencord_common::channel::ChannelKind;
use opencord_common::permissions::Permissions;
use opencord_proto::v1 as proto;
use proto::response::Result as Response;

use super::options::{Options, parse_time};
use super::{OptionSpec, Spec};
use crate::error::ApiError;
use crate::handlers::{Ctx, music as handlers};
use crate::music::session::Command;
use crate::permissions::require_channel;
use proto::CommandOptionKind as Kind;
use proto::MusicAction as Action;

const CHANNEL: OptionSpec = OptionSpec::new(
    "channel",
    "Another voice channel (Manage music)",
    Kind::Channel,
);
const QUERY: OptionSpec = OptionSpec::new("query", "Words or a link", Kind::Text)
    .required()
    .autocomplete();
const NEXT: OptionSpec = OptionSpec::new("next", "Play it next", Kind::Boolean);
const PAGE: OptionSpec = OptionSpec::new("page", "Which page", Kind::Integer).range(1, 50);
const TIME: OptionSpec = OptionSpec::new("time", "Where, like 1:30", Kind::Text).required();
const POSITION: OptionSpec = OptionSpec::new("position", "Its place in Up next", Kind::Integer)
    .required()
    .range(1, 500);
const FROM: OptionSpec = OptionSpec::new("from", "Its place now", Kind::Integer)
    .required()
    .range(1, 500);
const TO: OptionSpec = OptionSpec::new("to", "Its new place", Kind::Integer)
    .required()
    .range(1, 500);
const MODE: OptionSpec = OptionSpec::new("mode", "Off, the queue or the track", Kind::Choice)
    .required()
    .choices(&[("Off", "off"), ("Queue", "queue"), ("Track", "track")]);

const fn spec(
    name: &'static str,
    description: &'static str,
    options: &'static [OptionSpec],
    permissions: Permissions,
    permission_summary: &'static str,
) -> Spec {
    Spec {
        name,
        aliases: &[],
        description,
        options,
        feature: "music",
        permissions,
        permission_summary,
    }
}

pub const SPECS: &[Spec] = &[
    spec(
        "play",
        "Play a song, or add it to the queue",
        &[QUERY, NEXT, CHANNEL],
        Permissions::MUSIC_ADD,
        "Add music; next needs Manage the queue",
    ),
    spec(
        "pause",
        "Pause the music",
        &[CHANNEL],
        Permissions::MUSIC_CONTROL,
        "Control playback",
    ),
    spec(
        "resume",
        "Play the music again",
        &[CHANNEL],
        Permissions::MUSIC_CONTROL,
        "Control playback",
    ),
    spec(
        "skip",
        "Skip the track that plays",
        &[CHANNEL],
        Permissions::empty(),
        "Control playback, Add music for your own track, or vote to skip",
    ),
    spec(
        "previous",
        "Play the track before again",
        &[CHANNEL],
        Permissions::MUSIC_CONTROL,
        "Control playback",
    ),
    spec(
        "seek",
        "Go to a time in the track",
        &[TIME, CHANNEL],
        Permissions::MUSIC_CONTROL,
        "Control playback",
    ),
    spec(
        "stop",
        "Stop the music and clear the queue",
        &[CHANNEL],
        Permissions::MUSIC_MANAGE,
        "Manage music",
    ),
    spec(
        "queue",
        "Show what comes next, only to you",
        &[PAGE, CHANNEL],
        Permissions::empty(),
        "Anyone who can see the channel",
    ),
    Spec {
        aliases: &["np"],
        ..spec(
            "nowplaying",
            "Show what plays, only to you",
            &[CHANNEL],
            Permissions::empty(),
            "Anyone who can see the channel",
        )
    },
    spec(
        "remove",
        "Take a track out of the queue",
        &[POSITION, CHANNEL],
        Permissions::MUSIC_ADD,
        "Add music for your own tracks; others need Manage the queue",
    ),
    spec(
        "move",
        "Move a track in the queue",
        &[FROM, TO, CHANNEL],
        Permissions::MUSIC_MANAGE_QUEUE,
        "Manage the queue",
    ),
    spec(
        "clear",
        "Clear the queue",
        &[CHANNEL],
        Permissions::MUSIC_MANAGE_QUEUE,
        "Manage the queue",
    ),
    spec(
        "shuffle",
        "Turn shuffle on or off",
        &[CHANNEL],
        Permissions::MUSIC_CONTROL,
        "Control playback",
    ),
    spec(
        "repeat",
        "Repeat nothing, the queue or the track",
        &[MODE, CHANNEL],
        Permissions::MUSIC_CONTROL,
        "Control playback",
    ),
];

/// What a command said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    /// For the channel, unless results go only to who ran it.
    Public(String),
    /// Only to who ran it.
    Private(String),
}

fn private(text: impl Into<String>) -> Result<Reply, ApiError> {
    Ok(Reply::Private(text.into()))
}

/// The voice channel a command acts on: `channel` with Manage music there,
/// else the one the member is connected to.
fn target(ctx: &Ctx<'_>, options: &Options) -> Result<Option<(i64, String)>, ApiError> {
    let chosen = options.integer("channel");
    let channel_id = match chosen {
        Some(id) => id,
        None => match ctx.state.voice().get(ctx.user_id) {
            Some(voice) => voice.channel_id,
            None => return Ok(None),
        },
    };
    let guild = ctx.state.guild();
    let channel = require_channel(&guild, ctx.user_id, channel_id, Permissions::VIEW_CHANNEL)?;
    if channel.kind != ChannelKind::Voice {
        return Err(ApiError::new(
            proto::ErrorCode::CommandInvalidOptions,
            format!("#{} isn't a voice channel", channel.name),
        ));
    }
    if chosen.is_some()
        && !guild
            .channel_permissions(ctx.user_id, channel_id)
            .contains(Permissions::MUSIC_MANAGE)
    {
        return Err(crate::permissions::missing(Permissions::MUSIC_MANAGE));
    }
    Ok(Some((channel_id, channel.name.clone())))
}

fn session_of(response: Response) -> proto::MusicSession {
    match response {
        Response::MusicSession(session) => *session,
        _ => proto::MusicSession::default(),
    }
}

/// The channel's player as it is.
async fn snapshot(ctx: &Ctx<'_>, channel_id: i64) -> Result<proto::MusicSession, ApiError> {
    ctx.state
        .music
        .ask(ctx.state, channel_id, |reply| Command::Snapshot { reply })
        .await
}

fn track_line(item: &proto::MusicQueueItem) -> String {
    let track = item.track.clone().unwrap_or_default();
    if track.artist.is_empty() {
        format!("*{}*", track.title)
    } else {
        format!("*{}* — {}", track.title, track.artist)
    }
}

fn playback(channel_id: i64, action: Action) -> proto::MusicPlayback {
    proto::MusicPlayback {
        channel_id,
        action: action as i32,
        position_ms: None,
        expected_item_id: None,
    }
}

/// Runs a music command.
pub async fn run(ctx: &Ctx<'_>, spec: &Spec, options: &Options) -> Result<Reply, ApiError> {
    let Some((channel_id, name)) = target(ctx, options)? else {
        return private("Join a voice channel to use music commands");
    };
    match spec.name {
        "play" => play(ctx, channel_id, &name, options).await,
        "pause" | "resume" => {
            let pause = spec.name == "pause";
            let action = if pause { Action::Pause } else { Action::Resume };
            handlers::playback(ctx, playback(channel_id, action)).await?;
            Ok(Reply::Public(format!(
                "{} the music in {name}",
                if pause { "Paused" } else { "Resumed" }
            )))
        }
        "skip" => {
            let session = snapshot(ctx, channel_id).await?;
            let Some(item) = session.now_playing else {
                return private("Nothing is playing");
            };
            let perms = ctx.state.guild().channel_permissions(ctx.user_id, channel_id);
            let can_control = perms.contains(Permissions::MUSIC_CONTROL);
            let can_own_skip = item.requester_id == ctx.user_id && perms.contains(Permissions::MUSIC_ADD);
            if can_control || can_own_skip {
                let mut request = playback(channel_id, Action::Next);
                request.expected_item_id = Some(item.item_id.clone());
                handlers::playback(ctx, request).await?;
                Ok(Reply::Public(format!(
                    "Skipped {} in {name}",
                    track_line(&item)
                )))
            } else {
                handlers::vote_skip(
                    ctx,
                    proto::MusicVoteSkip {
                        channel_id,
                        expected_item_id: item.item_id.clone(),
                    },
                )
                .await?;
                Ok(Reply::Public(format!(
                    "Voted to skip {} in {name}",
                    track_line(&item)
                )))
            }
        }
        "previous" => {
            let session = snapshot(ctx, channel_id).await?;
            let Some(item) = session.now_playing else {
                return private("Nothing is playing");
            };
            let mut request = playback(channel_id, Action::Previous);
            request.expected_item_id = Some(item.item_id.clone());
            handlers::playback(ctx, request).await?;
            Ok(Reply::Public(format!("Went back in {name}")))
        }
        "seek" => {
            let time = options.text("time").unwrap_or_default();
            let Some(position_ms) = parse_time(time) else {
                return Err(ApiError::new(
                    proto::ErrorCode::CommandInvalidOptions,
                    "time is like 1:30",
                ));
            };
            let mut request = playback(channel_id, Action::Seek);
            request.position_ms = Some(position_ms);
            handlers::playback(ctx, request).await?;
            Ok(Reply::Public(format!("Went to {time} in {name}")))
        }
        "stop" => {
            handlers::playback(ctx, playback(channel_id, Action::Stop)).await?;
            Ok(Reply::Public(format!("Stopped the music in {name}")))
        }
        "queue" => queue(ctx, channel_id, &name, options.integer("page").unwrap_or(1)).await,
        "nowplaying" => {
            let session = snapshot(ctx, channel_id).await?;
            match session.now_playing {
                Some(item) => private(format!("Now playing in {name}: {}", track_line(&item))),
                None => private(format!("Nothing is playing in {name}")),
            }
        }
        "remove" => {
            let session = snapshot(ctx, channel_id).await?;
            let item = nth(&session, options.integer("position"))?;
            handlers::queue_remove(
                ctx,
                proto::MusicQueueRemove {
                    channel_id,
                    item_ids: vec![item.item_id.clone()],
                },
            )
            .await?;
            Ok(Reply::Public(format!(
                "Removed {} from the queue in {name}",
                track_line(item)
            )))
        }
        "move" => {
            let session = snapshot(ctx, channel_id).await?;
            let item = nth(&session, options.integer("from"))?.clone();
            let to = usize::try_from(options.integer("to").unwrap_or(1).max(1) - 1).unwrap_or(0);
            let rest: Vec<&proto::MusicQueueItem> = session
                .queue
                .iter()
                .filter(|each| each.item_id != item.item_id)
                .collect();
            handlers::queue_move(
                ctx,
                proto::MusicQueueMove {
                    channel_id,
                    item_id: item.item_id.clone(),
                    before_item_id: rest.get(to).map(|each| each.item_id.clone()),
                },
            )
            .await?;
            Ok(Reply::Public(format!(
                "Moved {} to {} in the queue in {name}",
                track_line(&item),
                to.min(rest.len()) + 1
            )))
        }
        "clear" => {
            handlers::queue_clear(ctx, &proto::MusicQueueClear { channel_id }).await?;
            Ok(Reply::Public(format!("Cleared the queue in {name}")))
        }
        "shuffle" => {
            let on = !snapshot(ctx, channel_id).await?.shuffle;
            handlers::set_mode(
                ctx,
                proto::MusicSetMode {
                    channel_id,
                    repeat: None,
                    shuffle: Some(on),
                },
            )
            .await?;
            Ok(Reply::Public(format!(
                "Shuffle is {} in {name}",
                if on { "on" } else { "off" }
            )))
        }
        "repeat" => {
            let (repeat, said) = match options.text("mode") {
                Some("queue") => (proto::MusicRepeat::Queue, "repeats the queue"),
                Some("track") => (proto::MusicRepeat::Track, "repeats the track"),
                _ => (proto::MusicRepeat::Off, "repeats nothing"),
            };
            handlers::set_mode(
                ctx,
                proto::MusicSetMode {
                    channel_id,
                    repeat: Some(repeat as i32),
                    shuffle: None,
                },
            )
            .await?;
            Ok(Reply::Public(format!("{name} {said}")))
        }
        _ => Err(ApiError::new(
            proto::ErrorCode::CommandUnknown,
            format!("/{} isn't a command here", spec.name),
        )),
    }
}

/// The queue item at a 1-based place.
fn nth(
    session: &proto::MusicSession,
    position: Option<i64>,
) -> Result<&proto::MusicQueueItem, ApiError> {
    position
        .and_then(|position| usize::try_from(position - 1).ok())
        .and_then(|index| session.queue.get(index))
        .ok_or_else(|| {
            ApiError::new(
                proto::ErrorCode::CommandInvalidOptions,
                format!("Up next has {} tracks", session.queue.len()),
            )
        })
}

/// `query` is a result this member was given (picked from the
/// suggestions), else words or a link to search: the first track found,
/// or every track of a playlist.
async fn play(
    ctx: &Ctx<'_>,
    channel_id: i64,
    name: &str,
    options: &Options,
) -> Result<Reply, ApiError> {
    let query = options.text("query").unwrap_or_default();
    let next = options.boolean("next");
    let picked = ctx
        .state
        .music
        .results
        .tracks(ctx.user_id, &[query.to_owned()])
        .is_ok();
    let (ids, playlist) = if picked {
        (vec![query.to_owned()], None)
    } else {
        let found = match handlers::search(
            ctx,
            proto::MusicSearch {
                channel_id,
                query: query.to_owned(),
                source: String::new(),
            },
        )
        .await?
        {
            Response::MusicSearchResults(found) => found,
            _ => proto::MusicSearchResults::default(),
        };
        match (found.playlist, found.results.first()) {
            (Some(playlist), _) => (playlist.result_ids.clone(), Some(playlist.name)),
            (None, Some(first)) => (vec![first.result_id.clone()], None),
            (None, None) => return private(format!("Nothing found for “{query}”")),
        }
    };
    let count = ids.len();
    let session = session_of(
        handlers::enqueue(
            ctx,
            proto::MusicEnqueue {
                channel_id,
                result_ids: ids.clone(),
                history_item_ids: Vec::new(),
                before_item_id: None,
                placement: if next {
                    proto::MusicPlacement::Next
                } else {
                    proto::MusicPlacement::End
                } as i32,
            },
        )
        .await?,
    );
    if let Some(playlist) = playlist {
        return Ok(Reply::Public(format!(
            "Added {count} tracks from *{playlist}* to the queue in {name}"
        )));
    }
    let added = session
        .now_playing
        .iter()
        .chain(&session.queue)
        .chain(&session.history)
        .filter(|item| item.requester_id == ctx.user_id)
        .max_by_key(|item| item.added_at_ms.max(item.item_id.parse().unwrap_or(0)))
        .map(track_line)
        .unwrap_or_else(|| "a track".to_owned());
    Ok(Reply::Public(if next {
        format!("Added {added} to play next in {name}")
    } else {
        format!("Added {added} to the queue in {name}")
    }))
}

/// Up next, 10 a page, only to who asked.
async fn queue(ctx: &Ctx<'_>, channel_id: i64, name: &str, page: i64) -> Result<Reply, ApiError> {
    const PER_PAGE: usize = 10;
    let session = snapshot(ctx, channel_id).await?;
    let mut lines = Vec::new();
    match &session.now_playing {
        Some(item) => lines.push(format!("Now playing in {name}: {}", track_line(item))),
        None => lines.push(format!("Nothing is playing in {name}")),
    }
    if session.queue.is_empty() {
        lines.push("Up next is empty".to_owned());
        return private(lines.join("\n"));
    }
    let pages = session.queue.len().div_ceil(PER_PAGE);
    let page = usize::try_from(page.max(1)).unwrap_or(1).min(pages);
    lines.push(format!("Up next, page {page} of {pages}:"));
    for (index, item) in session
        .queue
        .iter()
        .enumerate()
        .skip((page - 1) * PER_PAGE)
        .take(PER_PAGE)
    {
        lines.push(format!("{}. {}", index + 1, track_line(item)));
    }
    private(lines.join("\n"))
}

/// Suggestions for `/play`'s query (§19.6), searched for the voice
/// channel the member is in: search results, each a result id to play
/// exactly that track. Nothing for links.
pub async fn autocomplete(
    ctx: &Ctx<'_>,
    value: &str,
) -> Result<Vec<proto::CommandChoice>, ApiError> {
    let value = value.trim();
    if value.chars().count() < 2 || value.contains("://") {
        return Ok(Vec::new());
    }
    let Some(channel_id) = ctx
        .state
        .voice()
        .get(ctx.user_id)
        .map(|voice| voice.channel_id)
    else {
        return Ok(Vec::new());
    };
    handlers::may_act(ctx, channel_id, Permissions::MUSIC_ADD)?;
    let found = crate::music::search::search(
        ctx.state,
        ctx.user_id,
        &proto::MusicSearch {
            channel_id,
            query: value.to_owned(),
            source: String::new(),
        },
    )
    .await?;
    Ok(found
        .results
        .into_iter()
        .take(10)
        .map(|result| {
            let track = result.track.unwrap_or_default();
            let length = if track.is_stream {
                "LIVE".to_owned()
            } else {
                let seconds = track.duration_ms / 1000;
                format!("{}:{:02}", seconds / 60, seconds % 60)
            };
            let mut name = format!("{} — {} · {length}", track.title, track.artist);
            if name.chars().count() > 100 {
                name = name.chars().take(99).collect::<String>() + "…";
            }
            proto::CommandChoice {
                name,
                value: result.result_id,
                artwork_id: track.artwork_id,
            }
        })
        .collect())
}
