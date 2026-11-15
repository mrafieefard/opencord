//! Commands that asked for more time (§26.3): the message or private line
//! saying "is thinking…", and what replaces it.
//!
//! Each thinking command is one entry. A reply, the time running out and
//! the plugin stopping all start by removing it, so exactly one of them
//! finishes the command.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::{Duration, Instant};

use opencord_common::snowflake;
use opencord_plugins::limits::COMMAND_THINKING_MAX;
use opencord_proto::v1 as proto;
use proto::event::Kind as Event;
use tokio::task::AbortHandle;

use crate::db::messages;
use crate::error::ApiError;
use crate::state::{AppState, Audience};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ThinkingId(u64);

/// Where the thinking line is, and the reply will be.
#[derive(Debug, Clone)]
pub enum Place {
    Channel { message_id: i64, author_id: i64 },
    OnlyYou { session_id: String, nonce: String },
}

/// What's needed to start thinking.
#[derive(Debug, Clone)]
pub struct StartThinking {
    pub plugin_id: String,
    pub plugin_name: String,
    pub command: String,
    pub channel_id: i64,
    pub member_id: i64,
    pub place: Place,
}

/// The time ran out, or the reply can't come.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GaveUp {
    NoAnswer,
    RanIntoError,
}

/// The command wasn't thinking any more: its time had run out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TooLate;

struct Entry {
    command: StartThinking,
    expiry: AbortHandle,
}

/// Every command thinking now.
#[derive(Default)]
pub struct Thinking {
    next: AtomicU64,
    entries: Mutex<HashMap<ThinkingId, Entry>>,
}

impl std::fmt::Debug for Thinking {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Thinking").finish_non_exhaustive()
    }
}

impl Thinking {
    fn locked(&self) -> std::sync::MutexGuard<'_, HashMap<ThinkingId, Entry>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn remove(&self, id: ThinkingId) -> Option<Entry> {
        self.locked().remove(&id)
    }
}

/// Starts thinking: the cap is checked, the line is posted, and the time
/// starts.
pub async fn start(state: &Arc<AppState>, command: StartThinking) -> Result<ThinkingId, ApiError> {
    let id = reserve(state, &command)?;
    if let Err(error) = post(state, &command).await {
        if let Some(entry) = state.commands.thinking.remove(id) {
            entry.expiry.abort();
        }
        return Err(error);
    }
    // The plugin stopped while the line was being posted: nothing will finish it.
    if !state.commands.thinking.locked().contains_key(&id) {
        finish(
            state,
            &command,
            format!("{} didn't answer", command.plugin_name),
            proto::CommandState::NoAnswer,
        )
        .await;
    }
    Ok(id)
}

fn reserve(state: &Arc<AppState>, command: &StartThinking) -> Result<ThinkingId, ApiError> {
    let limit = Duration::from_secs(u64::from(state.config.plugins.thinking_secs));
    let thinking = &state.commands.thinking;
    let mut entries = thinking.locked();
    let waiting = entries
        .values()
        .filter(|entry| entry.command.plugin_id == command.plugin_id)
        .count();
    if waiting >= COMMAND_THINKING_MAX {
        return Err(ApiError::new(
            proto::ErrorCode::PluginLimit,
            "too many commands waiting",
        ));
    }
    let id = ThinkingId(thinking.next.fetch_add(1, Ordering::Relaxed));
    let expiry = tokio::spawn(expire(Arc::downgrade(state), id, limit)).abort_handle();
    entries.insert(
        id,
        Entry {
            command: command.clone(),
            expiry,
        },
    );
    Ok(id)
}

async fn expire(state: Weak<AppState>, id: ThinkingId, limit: Duration) {
    tokio::time::sleep(limit).await;
    let Some(state) = state.upgrade() else { return };
    let Some(entry) = state.commands.thinking.remove(id) else {
        return;
    };
    let text = format!("{} didn't answer", entry.command.plugin_name);
    finish(&state, &entry.command, text, proto::CommandState::NoAnswer).await;
}

/// The plugin's reply. `TooLate` when the command had already finished.
pub async fn answer(state: &Arc<AppState>, id: ThinkingId, content: String) -> Result<(), TooLate> {
    let entry = state.commands.thinking.remove(id).ok_or(TooLate)?;
    entry.expiry.abort();
    finish(
        state,
        &entry.command,
        content,
        proto::CommandState::Answered,
    )
    .await;
    Ok(())
}

/// Ends a thinking command with no reply.
pub async fn give_up(state: &Arc<AppState>, id: ThinkingId, why: GaveUp) {
    let Some(entry) = state.commands.thinking.remove(id) else {
        return;
    };
    entry.expiry.abort();
    let text = ending_text(&entry.command.plugin_name, why);
    finish(state, &entry.command, text, proto::CommandState::NoAnswer).await;
}

/// A stopped plugin can't reply to anything it had thinking.
pub async fn abandon(state: &Arc<AppState>, plugin_id: &str) {
    let entries: Vec<Entry> = {
        let mut all = state.commands.thinking.locked();
        let ids: Vec<ThinkingId> = all
            .iter()
            .filter(|(_, entry)| entry.command.plugin_id == plugin_id)
            .map(|(id, _)| *id)
            .collect();
        ids.iter().filter_map(|id| all.remove(id)).collect()
    };
    for entry in entries {
        entry.expiry.abort();
        let text = ending_text(&entry.command.plugin_name, GaveUp::NoAnswer);
        finish(state, &entry.command, text, proto::CommandState::NoAnswer).await;
    }
}

fn ending_text(plugin_name: &str, why: GaveUp) -> String {
    match why {
        GaveUp::NoAnswer => format!("{plugin_name} didn't answer"),
        GaveUp::RanIntoError => format!("{plugin_name} ran into an error"),
    }
}

fn the_message(
    command: &StartThinking,
    message_id: i64,
    author_id: i64,
    content: String,
    state: proto::CommandState,
) -> proto::Message {
    proto::Message {
        id: message_id,
        channel_id: command.channel_id,
        author_id,
        content,
        created_at_ms: snowflake::timestamp_ms(message_id),
        edited_at_ms: None,
        nonce: None,
        interaction: Some(proto::MessageInteraction {
            user_id: command.member_id,
            command: command.command.clone(),
            state: state as i32,
        }),
        attachments: Vec::new(),
    }
}

fn push_only_you(
    state: &AppState,
    command: &StartThinking,
    session_id: &str,
    nonce: &str,
    content: String,
    command_state: proto::CommandState,
) {
    // Gone means the member left; there's nobody to tell.
    if let Some(session) = state.sessions.get(session_id) {
        let event = proto::Event {
            kind: Some(Event::EphemeralMessage(proto::EphemeralMessage {
                channel_id: command.channel_id,
                nonce: nonce.to_owned(),
                content,
                state: command_state as i32,
            })),
        };
        session.push_event(&event, Instant::now());
    }
}

/// Posts the thinking line. Its stored text reads sensibly in older apps.
async fn post(state: &Arc<AppState>, command: &StartThinking) -> Result<(), ApiError> {
    let content = format!("{} is thinking…", command.plugin_name);
    let thinking = proto::CommandState::Thinking;
    match &command.place {
        Place::Channel {
            message_id,
            author_id,
        } => {
            let mut conn = state.db.acquire().await?;
            messages::insert_result(
                &mut conn,
                *message_id,
                command.channel_id,
                *author_id,
                &content,
                Some(messages::CommandRun {
                    name: &command.command,
                    user_id: command.member_id,
                    state: thinking as i32,
                }),
            )
            .await?;
            let message = the_message(command, *message_id, *author_id, content, thinking);
            state.broadcast(
                Event::MessageCreate(proto::MessageCreate {
                    message: Some(message),
                }),
                Audience::Channel(command.channel_id),
            );
        }
        Place::OnlyYou { session_id, nonce } => {
            push_only_you(state, command, session_id, nonce, content, thinking);
        }
    }
    Ok(())
}

async fn finish(
    state: &Arc<AppState>,
    command: &StartThinking,
    content: String,
    outcome: proto::CommandState,
) {
    match &command.place {
        Place::Channel {
            message_id,
            author_id,
        } => {
            if let Err(error) =
                finish_message(state, command, *message_id, *author_id, content, outcome).await
            {
                tracing::error!(%error, command = %command.command, "could not finish a thinking command");
            }
        }
        Place::OnlyYou { session_id, nonce } => {
            push_only_you(state, command, session_id, nonce, content, outcome);
        }
    }
}

async fn finish_message(
    state: &Arc<AppState>,
    command: &StartThinking,
    message_id: i64,
    author_id: i64,
    content: String,
    outcome: proto::CommandState,
) -> Result<(), sqlx::Error> {
    let mut conn = state.db.acquire().await?;
    if !messages::finish_thinking(&mut conn, message_id, &content, outcome as i32).await? {
        if let Some(handle) = state.plugins.get(&command.plugin_id) {
            handle.log().push(
                proto::PluginLogLevel::Warn,
                &format!(
                    "/{}'s message was deleted before the reply",
                    command.command
                ),
            );
        }
        return Ok(());
    }
    let message = the_message(command, message_id, author_id, content, outcome);
    state.broadcast(
        Event::MessageUpdate(proto::MessageUpdate {
            message: Some(message),
        }),
        Audience::Channel(command.channel_id),
    );
    Ok(())
}
