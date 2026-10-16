//! One voice channel's music (Phase 3 plan §14), run as a single actor so
//! what members do, Lavalink's events and timers apply in order. It keeps
//! the queue (§14.1), applies the playback rules (§14.2), drives the
//! channel's Lavalink player, sends snapshots and notices to subscribers
//! (§14.3–14.4) and saves itself at most every 2 s.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use opencord_common::permissions::Permissions;
use opencord_proto::v1 as proto;
use proto::event::Kind as Event;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

use super::player::Player;
use super::queue::{Ended, Item, Placement, Previous, Queue, Repeat};
use super::tracks::Track;
use crate::db;
use crate::error::ApiError;
use crate::lavalink::Node;
use crate::permissions::missing;
use crate::state::{AppState, Audience, now_ms};

/// Changes are saved this long after the first one.
const SAVE_AFTER: Duration = Duration::from_secs(2);
/// The queue is kept this long after everyone left (§14.2).
const KEEP_EMPTY: Duration = Duration::from_secs(30 * 60);
/// Playback stops after this many failures in a row (§14.2).
const FAILURES_MAX: u32 = 3;
/// Waiting for a node: tried again this often, and whenever one comes up.
const RETRY_EVERY: Duration = Duration::from_secs(5);
/// After a restart, the node music was on gets it back if it is up within
/// this long (§12.7: restarts keep music on the same nodes).
const PREFER_FOR: Duration = Duration::from_secs(15);
/// After a restart, members have this long to reconnect before the
/// channel counts as empty.
const RECONNECT_GRACE: Duration = Duration::from_secs(60);
/// Lavalink's position may differ this much from ours before subscribers
/// hear of it.
const DRIFT_MS: u64 = 1_000;
/// Lavalink's position reports are ignored this long after a track starts
/// or seeks: the first ones still give the position from before.
const SETTLE: Duration = Duration::from_millis(1_500);
/// Voice connections Lavalink may lose in [`VOICE_RETRY_WINDOW`] before
/// it is left alone.
const VOICE_RETRIES: usize = 3;
const VOICE_RETRY_WINDOW: Duration = Duration::from_secs(60);
/// Discord close codes the bridge itself ends Lavalink's voice
/// connection with: a newer voice state, or the player ended.
const OWN_CLOSES: [i64; 2] = [4006, 4014];

mod watchdog;

pub type Reply = oneshot::Sender<Result<proto::MusicSession, ApiError>>;

/// Who asks, and what they may do in the channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caller {
    pub user_id: i64,
    pub permissions: Permissions,
}

impl Caller {
    fn may(&self, needed: Permissions) -> Result<(), ApiError> {
        if self.permissions.contains(needed) {
            Ok(())
        } else {
            Err(missing(needed - self.permissions))
        }
    }
}

#[derive(Debug)]
pub enum Command {
    Enqueue {
        caller: Caller,
        items: Vec<Item>,
        placement: Placement,
        reply: Reply,
    },
    Playback {
        caller: Caller,
        action: proto::MusicAction,
        position_ms: Option<u64>,
        expected_item_id: Option<String>,
        reply: Reply,
    },
    SetMode {
        repeat: Option<Repeat>,
        shuffle: Option<bool>,
        reply: Reply,
    },
    Move {
        item_id: String,
        before_item_id: Option<String>,
        reply: Reply,
    },
    Remove {
        caller: Caller,
        item_ids: Vec<String>,
        reply: Reply,
    },
    Clear {
        reply: Reply,
    },
    Snapshot {
        reply: oneshot::Sender<proto::MusicSession>,
    },
    /// The tracks of history items, to add again; unknown ids are left
    /// out.
    History {
        item_ids: Vec<String>,
        reply: oneshot::Sender<Vec<Track>>,
    },
    /// Someone joined or left the channel.
    Listeners,
    /// An event or position from the node `node_id`.
    Lavalink {
        node_id: i64,
        value: Value,
    },
    /// Plays on another node now: `to`, or the best one but `from`. Says
    /// where it went, if it was on `from` and moved.
    Relocate {
        from: i64,
        to: Option<i64>,
        done: oneshot::Sender<Option<i64>>,
    },
    /// A node is up: a channel waiting for one tries again.
    NodeUp,
    /// Music was turned off, or the channel deleted: nothing plays, the
    /// queue is gone.
    End,
    /// Saves now: the server stops.
    Save {
        done: oneshot::Sender<()>,
    },
    /// What the channel's bridge saw.
    Bridge(opencord_voice::bridge::BridgeEvent),
}

/// What survives a restart.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Saved {
    pub queue: Queue,
    pub paused: bool,
    pub paused_empty: bool,
    pub position_ms: u64,
    pub node_id: Option<i64>,
    pub version: u64,
}

/// Starts a channel's session, fresh or as it was saved.
pub fn spawn(
    state: &Arc<AppState>,
    channel_id: i64,
    saved: Option<Saved>,
) -> mpsc::UnboundedSender<Command> {
    let (sender, receiver) = mpsc::unbounded_channel();
    let session = Session::new(state, channel_id, saved);
    tokio::spawn(session.run(receiver));
    sender
}

struct Session {
    state: Arc<AppState>,
    channel_id: i64,
    queue: Queue,
    /// A member paused.
    paused: bool,
    /// Everyone left (§14.2).
    paused_empty: bool,
    /// Lavalink said the current item started.
    started: bool,
    /// The position was this at `anchor` (`anchor_ms` on the clock).
    position_ms: u64,
    anchor: Instant,
    anchor_ms: i64,
    /// Lavalink's position reports count again from then on.
    settled_at: Instant,
    version: u64,
    player: Option<Player>,
    waiting_for_node: bool,
    retry_at: Option<Instant>,
    /// After a restart: the node the music was on, until then.
    prefer: Option<(i64, Instant)>,
    /// After a restart: Lavalink may still play the current item.
    restored: bool,
    failures: u32,
    empty_since: Option<Instant>,
    idle_since: Option<Instant>,
    /// The channel counts as empty only from then on.
    grace_until: Option<Instant>,
    save_at: Option<Instant>,
    voice_retries: VecDeque<Instant>,
    /// The audio watchdog's view of the bridge.
    watch: watchdog::Watch,
    /// The now-playing card as last sent: item and whether it plays.
    card: Option<(String, bool)>,
    /// The item "Now playing" was said for.
    announced: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlayState {
    Idle,
    Loading,
    Playing,
    Paused,
}

impl Session {
    fn new(state: &Arc<AppState>, channel_id: i64, saved: Option<Saved>) -> Self {
        let now = Instant::now();
        let restored = saved.is_some();
        let saved = saved.unwrap_or_default();
        state.music.artwork.remember_queue(&saved.queue);
        let wants_to_play = saved.queue.now.is_some() && !saved.paused && !saved.paused_empty;
        Self {
            state: Arc::clone(state),
            channel_id,
            paused: saved.paused,
            paused_empty: saved.paused_empty,
            started: false,
            position_ms: saved.position_ms,
            anchor: now,
            anchor_ms: now_ms(),
            settled_at: now,
            version: saved.version,
            player: None,
            waiting_for_node: wants_to_play,
            retry_at: wants_to_play.then_some(now),
            prefer: saved.node_id.map(|node_id| (node_id, now + PREFER_FOR)),
            restored: restored && saved.queue.now.is_some(),
            failures: 0,
            empty_since: None,
            idle_since: (restored && saved.queue.now.is_none()).then_some(now),
            grace_until: restored.then_some(now + RECONNECT_GRACE),
            save_at: None,
            voice_retries: VecDeque::new(),
            watch: watchdog::Watch::default(),
            card: None,
            announced: saved.queue.now.as_ref().map(|item| item.id.clone()),
            queue: saved.queue,
        }
    }

    async fn run(mut self, mut commands: mpsc::UnboundedReceiver<Command>) {
        if self.grace_until.is_none() {
            self.listeners().await;
        }
        loop {
            let wake = self
                .next_wake()
                .unwrap_or_else(|| Instant::now() + Duration::from_secs(3600));
            tokio::select! {
                command = commands.recv() => match command {
                    Some(command) => self.handle(command).await,
                    None => return,
                },
                () = tokio::time::sleep_until(wake.into()) => self.timers().await,
                // Saved just before (`Music::save_all`); players stay, and
                // Lavalink resumes them (§12.2).
                () = self.state.shutdown.cancelled() => return,
            }
            if self.finished() && self.state.music.end(self.channel_id, &commands) {
                self.save().await;
                return;
            }
        }
    }

    async fn handle(&mut self, command: Command) {
        match command {
            Command::Enqueue {
                caller,
                items,
                placement,
                reply,
            } => {
                let result = self.enqueue(caller, items, &placement).await;
                self.reply(reply, result);
            }
            Command::Playback {
                caller,
                action,
                position_ms,
                expected_item_id,
                reply,
            } => {
                let result = self
                    .playback(caller, action, position_ms, expected_item_id)
                    .await;
                self.reply(reply, result);
            }
            Command::SetMode {
                repeat,
                shuffle,
                reply,
            } => {
                if let Some(repeat) = repeat {
                    self.queue.repeat = repeat;
                }
                if let Some(shuffle) = shuffle {
                    self.queue.set_shuffle(shuffle, &mut crate::random::below);
                }
                self.reply(reply, Ok(()));
            }
            Command::Move {
                item_id,
                before_item_id,
                reply,
            } => {
                let result = if self.queue.move_item(&item_id, before_item_id.as_deref()) {
                    Ok(())
                } else {
                    Err(ApiError::not_found("queue item"))
                };
                self.reply(reply, result);
            }
            Command::Remove {
                caller,
                item_ids,
                reply,
            } => {
                let result = self.remove(caller, &item_ids);
                self.reply(reply, result);
            }
            Command::Clear { reply } => {
                self.queue.clear();
                self.reply(reply, Ok(()));
            }
            Command::Snapshot { reply } => {
                let _ = reply.send(self.to_proto());
            }
            Command::History { item_ids, reply } => {
                let tracks = item_ids
                    .iter()
                    .filter_map(|id| self.queue.history.iter().find(|item| &item.id == id))
                    .map(|item| item.track.clone())
                    .collect();
                let _ = reply.send(tracks);
            }
            Command::Listeners => self.listeners().await,
            Command::Lavalink { node_id, value } => self.lavalink(node_id, &value).await,
            Command::Relocate { from, to, done } => {
                let moved = self.relocate(from, to).await;
                let _ = done.send(moved);
            }
            Command::NodeUp => {
                if self.waiting_for_node {
                    self.retry().await;
                }
            }
            Command::End => {
                self.end();
                self.changed();
            }
            Command::Save { done } => {
                self.save().await;
                let _ = done.send(());
            }
            Command::Bridge(event) => self.bridge(&event),
        }
    }

    /// Answers a request; a change was made when it succeeded.
    fn reply(&mut self, reply: Reply, result: Result<(), ApiError>) {
        if result.is_ok() {
            self.changed();
        }
        let _ = reply.send(result.map(|()| self.to_proto()));
    }

    // Requests.

    async fn enqueue(
        &mut self,
        caller: Caller,
        items: Vec<Item>,
        placement: &Placement,
    ) -> Result<(), ApiError> {
        let settings = self.state.music.settings();
        let limit = usize::try_from(settings.queue_limit).unwrap_or(usize::MAX);
        if self.queue.len() + items.len() > limit {
            return Err(ApiError::new(
                proto::ErrorCode::MusicQueueFull,
                format!("The queue holds at most {limit} tracks"),
            ));
        }
        if !caller.permissions.contains(Permissions::MUSIC_MANAGE) {
            let max = usize::try_from(settings.max_tracks_per_member).unwrap_or(usize::MAX);
            if self.queue.count_by(caller.user_id) + items.len() > max {
                return Err(ApiError::new(
                    proto::ErrorCode::MusicMemberLimit,
                    format!("You can have at most {max} tracks in the queue"),
                ));
            }
        }
        let Some(first) = items.first() else {
            return Err(ApiError::invalid_argument("no tracks to add"));
        };
        let name = super::display_name(&self.state, caller.user_id);
        let text = match items.len() {
            1 => format!("{name} added '{}'", first.track.info.title),
            count => format!("{name} added {count} tracks"),
        };
        if self.queue.now.is_none() || *placement == Placement::Now {
            self.paused = false;
            self.failures = 0;
            self.idle_since = None;
            // Added from outside an empty channel: it waits for listeners.
            self.paused_empty = self.empty_since.is_some();
        }
        let started = self.queue.add(items, placement);
        self.notice(text);
        if started {
            self.start_current(0, &[]).await;
        }
        Ok(())
    }

    async fn playback(
        &mut self,
        caller: Caller,
        action: proto::MusicAction,
        position_ms: Option<u64>,
        expected_item_id: Option<String>,
    ) -> Result<(), ApiError> {
        use proto::MusicAction as Action;
        let current = self.queue.now.clone();
        let stale = expected_item_id
            .is_some_and(|expected| current.as_ref().is_none_or(|item| item.id != expected));
        if stale && matches!(action, Action::Next | Action::Previous | Action::Seek) {
            return Err(ApiError::new(
                proto::ErrorCode::MusicStale,
                "Another track plays now",
            ));
        }
        let own_track = current
            .as_ref()
            .is_some_and(|item| item.requester_id == caller.user_id);
        match action {
            Action::Unspecified => return Err(ApiError::invalid_argument("an action is needed")),
            Action::Next if own_track => caller.may(Permissions::MUSIC_ADD)?,
            Action::Stop => caller.may(Permissions::MUSIC_MANAGE)?,
            _ => caller.may(Permissions::MUSIC_CONTROL)?,
        }
        let nothing = || ApiError::invalid_argument("nothing is playing");
        match action {
            Action::Pause => {
                current.ok_or_else(nothing)?;
                if !self.paused {
                    let position = self.position();
                    self.paused = true;
                    self.set_position(position);
                    self.tell_paused().await;
                }
            }
            Action::Resume => {
                current.ok_or_else(nothing)?;
                if self.paused {
                    self.paused = false;
                    self.resume().await;
                }
            }
            Action::Next => {
                current.ok_or_else(nothing)?;
                self.advance(Ended::Skipped).await;
            }
            Action::Previous => match self.queue.previous(self.position()) {
                Previous::Restart => {
                    current.ok_or_else(nothing)?;
                    self.seek(0).await;
                }
                Previous::Back => {
                    self.paused = false;
                    self.start_current(0, &[]).await;
                }
            },
            Action::Seek => {
                let item = current.ok_or_else(nothing)?;
                let info = &item.track.info;
                if info.is_stream || !info.is_seekable {
                    return Err(ApiError::invalid_argument("this track can't be sought"));
                }
                let position = position_ms
                    .ok_or_else(|| ApiError::invalid_argument("a position is needed"))?;
                self.seek(position.min(info.length_ms)).await;
            }
            Action::Stop => {
                self.end();
                let name = super::display_name(&self.state, caller.user_id);
                self.notice(format!("{name} stopped the music"));
            }
            Action::Unspecified => {}
        }
        Ok(())
    }

    fn remove(&mut self, caller: Caller, item_ids: &[String]) -> Result<(), ApiError> {
        let others = item_ids
            .iter()
            .filter_map(|id| self.queue.upcoming_item(id))
            .any(|item| item.requester_id != caller.user_id);
        caller.may(if others {
            Permissions::MUSIC_MANAGE_QUEUE
        } else {
            Permissions::MUSIC_ADD
        })?;
        if self.queue.remove(item_ids).is_empty() {
            return Err(ApiError::not_found("queue item"));
        }
        Ok(())
    }

    // Playback.

    /// Whether the music moves on now.
    fn playing(&self) -> bool {
        self.queue.now.is_some()
            && self.started
            && !self.paused
            && !self.paused_empty
            && !self.waiting_for_node
    }

    fn play_state(&self) -> PlayState {
        if self.queue.now.is_none() {
            PlayState::Idle
        } else if self.paused || self.paused_empty {
            PlayState::Paused
        } else if self.started && !self.waiting_for_node {
            PlayState::Playing
        } else {
            PlayState::Loading
        }
    }

    fn position(&self) -> u64 {
        let mut position = self.position_ms;
        if self.playing() {
            position += u64::try_from(self.anchor.elapsed().as_millis()).unwrap_or(u64::MAX);
        }
        match &self.queue.now {
            Some(item) if !item.track.info.is_stream => position.min(item.track.info.length_ms),
            _ => position,
        }
    }

    fn set_position(&mut self, position_ms: u64) {
        self.position_ms = position_ms;
        self.anchor = Instant::now();
        self.anchor_ms = now_ms();
    }

    /// Plays the current item from `position_ms`, on the node it is on or
    /// the best one but those in `except`; waits for a node when none can.
    async fn start_current(&mut self, position_ms: u64, except: &[i64]) {
        self.started = false;
        self.set_position(position_ms);
        self.settled_at = Instant::now() + SETTLE;
        let Some(item) = self.queue.now.clone() else {
            return;
        };
        let mut except = except.to_vec();
        for _ in 0..2 {
            if !self.ensure_player(&item.track.info.source_name, &except) {
                break;
            }
            if self.restored && self.reattach(&item).await {
                return;
            }
            let paused = self.paused || self.paused_empty;
            let channel_id = self.channel_id;
            let Some(player) = self.player.as_mut() else {
                break;
            };
            match player
                .play(&self.state, channel_id, &item, position_ms, paused)
                .await
            {
                Ok(()) => {
                    self.waiting_for_node = false;
                    self.retry_at = None;
                    return;
                }
                Err(failure) => {
                    tracing::warn!(
                        channel_id,
                        node_id = player.node_id,
                        reason = failure.text(),
                        "a track could not be started on a Lavalink node"
                    );
                    except.push(player.node_id);
                    self.drop_player();
                }
            }
        }
        self.wait_for_node();
    }

    /// After a restart: Lavalink kept the player and still plays the
    /// current item, so it only needs a new voice state.
    async fn reattach(&mut self, item: &Item) -> bool {
        self.restored = false;
        let channel_id = self.channel_id;
        let Some(player) = self.player.as_mut() else {
            return false;
        };
        let Some(playing) = player.playing(channel_id).await else {
            return false;
        };
        if playing.item_id != item.id
            || player
                .reconnect_voice(&self.state, channel_id)
                .await
                .is_err()
        {
            return false;
        }
        tracing::info!(
            channel_id,
            node_id = player.node_id,
            "music carried on through a restart"
        );
        let paused = self.paused || self.paused_empty;
        if playing.paused != paused {
            let _ = player.set_paused(&self.state, channel_id, paused).await;
        }
        self.started = true;
        self.waiting_for_node = false;
        self.retry_at = None;
        self.set_position(playing.position_ms);
        true
    }

    /// Keeps the player, or opens one on the best node for `source` but
    /// those in `except`. False when no node can play it.
    fn ensure_player(&mut self, source: &str, except: &[i64]) -> bool {
        if let Some(player) = &self.player {
            if !except.contains(&player.node_id) && player.usable(&self.state) {
                return true;
            }
            self.drop_player();
        }
        let Some(node) = self.choose_node(source, except) else {
            return false;
        };
        let Some(player) = Player::open(&self.state, &node) else {
            return false;
        };
        self.state
            .lavalink
            .assign(&self.state, self.channel_id, node.id);
        self.state.music.forget_preferred(self.channel_id);
        self.prefer = None;
        self.player = Some(player);
        true
    }

    fn choose_node(&mut self, source: &str, except: &[i64]) -> Option<Arc<Node>> {
        if let Some((node_id, until)) = self.prefer {
            if Instant::now() < until {
                // Its old node, once it is back; nothing else meanwhile.
                return self
                    .state
                    .lavalink
                    .node(node_id)
                    .filter(|node| node.health().is_up() && !except.contains(&node.id));
            }
            self.prefer = None;
            self.state.music.forget_preferred(self.channel_id);
        }
        self.state.lavalink.best_node(Some(source), except)
    }

    /// Lets go of the player: its node is gone, or the channel moves.
    fn drop_player(&mut self) {
        if let Some(player) = self.player.take() {
            player.abandon(self.channel_id);
            self.state.lavalink.release(&self.state, self.channel_id);
        }
    }

    /// Ends the player for good: nothing plays for the channel any more.
    fn destroy_player(&mut self) {
        if let Some(player) = self.player.take() {
            player.destroy(&self.state, self.channel_id);
            self.state.lavalink.release(&self.state, self.channel_id);
        }
        self.started = false;
    }

    fn wait_for_node(&mut self) {
        let quiet = self.prefer.is_some();
        if !self.waiting_for_node && !quiet {
            self.notice("Music server unavailable — retrying".to_owned());
        }
        self.waiting_for_node = true;
        self.retry_at = Some(Instant::now() + RETRY_EVERY);
    }

    async fn retry(&mut self) {
        if self.queue.now.is_none() {
            self.waiting_for_node = false;
            self.retry_at = None;
            return;
        }
        let position = self.position_ms;
        let was_waiting = self.waiting_for_node;
        self.start_current(position, &[]).await;
        if was_waiting && !self.waiting_for_node {
            self.changed();
        }
    }

    /// Tells the player whether it is paused.
    async fn tell_paused(&mut self) {
        let paused = self.paused || self.paused_empty;
        let channel_id = self.channel_id;
        if let Some(player) = self.player.as_mut()
            && player
                .set_paused(&self.state, channel_id, paused)
                .await
                .is_err()
        {
            self.drop_player();
            if !paused {
                self.start_current(self.position_ms, &[]).await;
            }
        }
    }

    /// Plays on from the position kept.
    async fn resume(&mut self) {
        let position = self.position_ms;
        self.set_position(position);
        let usable = self
            .player
            .as_ref()
            .is_some_and(|player| player.usable(&self.state));
        if usable && self.started && !self.waiting_for_node {
            self.tell_paused().await;
            self.expect_audio();
        } else {
            self.start_current(position, &[]).await;
        }
    }

    async fn seek(&mut self, position_ms: u64) {
        self.set_position(position_ms);
        self.settled_at = Instant::now() + SETTLE;
        let channel_id = self.channel_id;
        let sought = match self.player.as_mut() {
            Some(player) if self.started => player
                .seek(&self.state, channel_id, position_ms)
                .await
                .is_ok(),
            _ => false,
        };
        if !sought && !self.waiting_for_node {
            self.start_current(position_ms, &[]).await;
        }
    }

    /// The current item ends; the next one plays, if there is one.
    async fn advance(&mut self, ended: Ended) {
        match ended {
            Ended::Failed => self.failures += 1,
            Ended::Finished => self.failures = 0,
            Ended::Skipped => {}
        }
        if !self.queue.advance(ended) {
            self.queue_ended().await;
            return;
        }
        if self.failures >= FAILURES_MAX {
            self.failures = 0;
            self.paused = true;
            self.started = false;
            self.set_position(0);
            self.notice("Music stopped: 3 tracks in a row couldn't be played".to_owned());
            let channel_id = self.channel_id;
            if let Some(player) = self.player.as_mut() {
                let _ = player.stop(&self.state, channel_id).await;
            }
            return;
        }
        let except = self.move_between_tracks();
        self.start_current(0, &except).await;
    }

    /// Between tracks, a channel on a struggling or drained node moves to
    /// a healthy one (§12.6). Returns the nodes not to use.
    fn move_between_tracks(&mut self) -> Vec<i64> {
        let Some(player) = &self.player else {
            return Vec::new();
        };
        let Some(item) = &self.queue.now else {
            return Vec::new();
        };
        if !player.should_move(&self.state) {
            return Vec::new();
        }
        let from = player.node_id;
        let better = self
            .state
            .lavalink
            .best_node(Some(&item.track.info.source_name), &[from])
            .filter(|node| node.health() == crate::lavalink::health::Health::Healthy);
        let Some(better) = better else {
            return Vec::new();
        };
        self.drop_player();
        self.state.lavalink.moved(&self.state, from, better.id, 1);
        vec![from]
    }

    async fn queue_ended(&mut self) {
        self.started = false;
        self.set_position(0);
        self.idle_since = Some(Instant::now());
        self.waiting_for_node = false;
        self.retry_at = None;
        let channel_id = self.channel_id;
        if let Some(player) = self.player.as_mut()
            && player.stop(&self.state, channel_id).await.is_err()
        {
            self.drop_player();
        }
    }

    /// Nothing plays and nothing is queued; the player goes.
    fn end(&mut self) {
        self.queue.clear();
        self.queue.advance(Ended::Failed);
        self.destroy_player();
        self.paused = false;
        self.paused_empty = false;
        self.waiting_for_node = false;
        self.retry_at = None;
        self.set_position(0);
        self.idle_since = Some(Instant::now());
    }

    /// Plays elsewhere: its node is gone or being emptied (§12.7).
    async fn relocate(&mut self, from: i64, to: Option<i64>) -> Option<i64> {
        if self.player.as_ref().map(|player| player.node_id) != Some(from) {
            return None;
        }
        let position = self.position();
        self.drop_player();
        // Nothing plays: no player is needed anywhere.
        self.queue.now.as_ref()?;
        if let Some(node) = to
            .and_then(|to| self.state.lavalink.node(to))
            .filter(|node| node.health().is_up())
            && let Some(player) = Player::open(&self.state, &node)
        {
            self.state
                .lavalink
                .assign(&self.state, self.channel_id, node.id);
            self.player = Some(player);
        }
        self.start_current(position, &[from]).await;
        let to = self
            .player
            .as_ref()
            .map(|player| player.node_id)
            .filter(|_| !self.waiting_for_node)?;
        self.notice("Switched to another music server".to_owned());
        self.changed();
        Some(to)
    }

    // Lavalink.

    async fn lavalink(&mut self, node_id: i64, value: &Value) {
        if self.player.as_ref().map(|player| player.node_id) != Some(node_id) {
            return;
        }
        let current = self.queue.now.as_ref().map(|item| item.id.clone());
        let item_id = value["track"]["userData"]["itemId"].as_str();
        let ours = item_id.is_some() && item_id == current.as_deref();
        match (value["op"].as_str(), value["type"].as_str()) {
            (Some("playerUpdate"), _) => self.player_update(value),
            (_, Some("TrackStartEvent")) if ours => {
                self.started = true;
                self.failures = 0;
                self.set_position(self.position_ms);
                self.expect_audio();
                if self.announced != current {
                    self.announced.clone_from(&current);
                    if let Some(item) = &self.queue.now {
                        let text = format!("Now playing: {}", item.track.info.title);
                        self.notice(text);
                    }
                }
                self.changed();
            }
            (_, Some("TrackEndEvent")) if ours => match value["reason"].as_str() {
                Some("finished") => {
                    self.advance(Ended::Finished).await;
                    self.changed();
                }
                Some("loadFailed") => self.failed(None).await,
                Some("cleanup") => {
                    // Lavalink let the player go; this channel still wants it.
                    self.player = None;
                    self.state.lavalink.release(&self.state, self.channel_id);
                    if !self.paused && !self.paused_empty {
                        self.start_current(self.position_ms, &[]).await;
                    }
                }
                _ => {}
            },
            (_, Some("TrackExceptionEvent")) if ours => {
                let reason = value["exception"]["message"].as_str().map(str::to_owned);
                self.failed(reason).await;
            }
            (_, Some("TrackStuckEvent")) if ours => {
                self.failed(Some("it stopped sending audio".to_owned()))
                    .await;
            }
            (_, Some("WebSocketClosedEvent")) => self.voice_closed(value).await,
            _ => {}
        }
    }

    fn player_update(&mut self, value: &Value) {
        if !self.playing() || Instant::now() < self.settled_at {
            return;
        }
        let Some(position) = value["state"]["position"].as_u64() else {
            return;
        };
        let drift = position.abs_diff(self.position());
        self.set_position(position);
        if drift > DRIFT_MS {
            self.changed();
        }
    }

    /// The current item could not be played: say so and play the next.
    async fn failed(&mut self, reason: Option<String>) {
        let Some(item) = &self.queue.now else {
            return;
        };
        let title = item.track.info.title.clone();
        if let Some(node) = self
            .player
            .as_ref()
            .and_then(|player| self.state.lavalink.node(player.node_id))
        {
            node.load_failed();
        }
        tracing::info!(
            channel_id = self.channel_id,
            reason = reason.as_deref().unwrap_or("load failed"),
            "a track could not be played"
        );
        self.notice(format!("Couldn't play '{title}', skipped"));
        self.advance(Ended::Failed).await;
        self.changed();
    }

    /// Lavalink lost its voice connection to the bridge.
    async fn voice_closed(&mut self, value: &Value) {
        let code = value["code"].as_i64().unwrap_or(0);
        if OWN_CLOSES.contains(&code) || self.queue.now.is_none() {
            return;
        }
        let now = Instant::now();
        while self
            .voice_retries
            .front()
            .is_some_and(|at| now.duration_since(*at) > VOICE_RETRY_WINDOW)
        {
            self.voice_retries.pop_front();
        }
        if self.voice_retries.len() >= VOICE_RETRIES {
            tracing::warn!(
                channel_id = self.channel_id,
                code,
                "Lavalink keeps losing its voice connection; left alone for now"
            );
            return;
        }
        self.voice_retries.push_back(now);
        let channel_id = self.channel_id;
        if let Some(player) = self.player.as_mut()
            && player
                .reconnect_voice(&self.state, channel_id)
                .await
                .is_err()
        {
            self.drop_player();
            self.start_current(self.position(), &[]).await;
        }
    }

    // Listeners and timers.

    async fn listeners(&mut self) {
        let count = self.state.voice().in_channel(self.channel_id).count();
        let now = Instant::now();
        if self.grace_until.is_some_and(|until| now < until) {
            if count > 0 {
                self.grace_until = None;
            } else {
                return;
            }
        }
        self.grace_until = None;
        if count == 0 {
            if self.empty_since.is_none() {
                self.empty_since = Some(now);
                if self.queue.now.is_some() && !self.paused && !self.paused_empty {
                    let position = self.position();
                    self.paused_empty = true;
                    self.set_position(position);
                    self.tell_paused().await;
                    self.changed();
                }
            }
            return;
        }
        self.empty_since = None;
        // Back on another voice node: the bridge there takes the music.
        let channel_id = self.channel_id;
        if let Some(player) = self.player.as_mut()
            && player.bridge_moved(&self.state, channel_id)
            && player
                .reconnect_voice(&self.state, channel_id)
                .await
                .is_err()
        {
            self.drop_player();
        }
        if self.paused_empty {
            self.paused_empty = false;
            if !self.paused {
                self.resume().await;
            }
            self.changed();
        } else {
            // Whoever joined gets the snapshot.
            self.publish();
        }
    }

    fn next_wake(&self) -> Option<Instant> {
        let settings = self.state.music.settings();
        let has_player = self.player.is_some();
        let seconds = |s: u32| Duration::from_secs(u64::from(s));
        [
            self.save_at,
            self.watchdog_due(),
            self.retry_at.filter(|_| self.waiting_for_node),
            self.grace_until,
            self.empty_since
                .filter(|_| has_player)
                .map(|since| since + seconds(settings.stop_after_empty_s)),
            self.empty_since.map(|since| since + KEEP_EMPTY),
            self.idle_since
                .filter(|_| has_player && self.queue.now.is_none())
                .map(|since| since + seconds(settings.disconnect_after_idle_s)),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    async fn timers(&mut self) {
        let now = Instant::now();
        let settings = self.state.music.settings();
        let seconds = |s: u32| Duration::from_secs(u64::from(s));
        if self.save_at.is_some_and(|at| at <= now) {
            self.save().await;
        }
        self.watchdog(now).await;
        if self.grace_until.is_some_and(|until| until <= now) {
            self.grace_until = None;
            self.listeners().await;
        }
        if self.waiting_for_node && self.retry_at.is_some_and(|at| at <= now) {
            self.retry().await;
        }
        if let Some(since) = self.empty_since {
            if self.player.is_some() && since + seconds(settings.stop_after_empty_s) <= now {
                self.destroy_player();
            }
            if since + KEEP_EMPTY <= now {
                self.end();
                self.changed();
            }
        }
        if let Some(since) = self.idle_since
            && self.queue.now.is_none()
            && self.player.is_some()
            && since + seconds(settings.disconnect_after_idle_s) <= now
        {
            self.destroy_player();
        }
    }

    /// Nothing is left to keep: no music, no player, nobody listening.
    fn finished(&self) -> bool {
        self.queue.is_empty()
            && self.player.is_none()
            && self.save_at.is_none()
            && self
                .state
                .voice()
                .in_channel(self.channel_id)
                .next()
                .is_none()
    }

    // Sync.

    /// Something changed: a new version, saved soon, sent now.
    fn changed(&mut self) {
        self.version += 1;
        if self.save_at.is_none() {
            self.save_at = Some(Instant::now() + SAVE_AFTER);
        }
        self.publish();
        self.card();
    }

    fn publish(&self) {
        let snapshot = proto::MusicSnapshot {
            session: Some(self.to_proto()),
        };
        self.state.music.publish(
            &self.state,
            self.channel_id,
            Event::MusicSnapshot(Box::new(snapshot)),
        );
    }

    fn notice(&self, text: String) {
        self.state.music.publish(
            &self.state,
            self.channel_id,
            Event::MusicNotice(proto::MusicNotice {
                channel_id: self.channel_id,
                text,
                at_ms: now_ms(),
            }),
        );
    }

    /// The now-playing card, to everyone who can see the channel, when
    /// what plays or whether it plays changed.
    fn card(&mut self) {
        let playing = matches!(self.play_state(), PlayState::Playing | PlayState::Loading);
        let card = self
            .queue
            .now
            .as_ref()
            .map(|item| (item.id.clone(), playing));
        if card == self.card {
            return;
        }
        self.card.clone_from(&card);
        let event = match &self.queue.now {
            Some(item) => proto::MusicNowPlaying {
                channel_id: self.channel_id,
                title: item.track.info.title.clone(),
                artist: item.track.info.author.clone(),
                artwork_id: super::artwork::id_of(&item.track.info.artwork_url),
                playing,
                ended: false,
                source_display_name: super::tracks::source_display_name(
                    &item.track.info.source_name,
                ),
            },
            None => proto::MusicNowPlaying {
                channel_id: self.channel_id,
                ended: true,
                ..Default::default()
            },
        };
        self.state.broadcast(
            Event::MusicNowPlaying(event),
            Audience::Channel(self.channel_id),
        );
    }

    fn to_proto(&self) -> proto::MusicSession {
        let state = match self.play_state() {
            PlayState::Idle => proto::MusicPlayState::Idle,
            PlayState::Loading => proto::MusicPlayState::Loading,
            PlayState::Playing => proto::MusicPlayState::Playing,
            PlayState::Paused => proto::MusicPlayState::Paused,
        };
        proto::MusicSession {
            channel_id: self.channel_id,
            version: self.version,
            state: state as i32,
            now_playing: self.queue.now.as_ref().map(Item::to_proto),
            queue: self.queue.upcoming.iter().map(Item::to_proto).collect(),
            history: self.queue.history.iter().map(Item::to_proto).collect(),
            position_ms: self.position_ms,
            server_time_ms: self.anchor_ms,
            repeat: self.queue.repeat.to_proto() as i32,
            shuffle: self.queue.shuffle,
            waiting_for_node: self.waiting_for_node,
            paused_empty: self.paused_empty,
        }
    }

    /// Saves what survives a restart, or forgets it when nothing does.
    async fn save(&mut self) {
        self.save_at = None;
        let saved = Saved {
            queue: self.queue.clone(),
            paused: self.paused,
            paused_empty: self.paused_empty,
            position_ms: self.position(),
            node_id: self.player.as_ref().map(|player| player.node_id),
            version: self.version,
        };
        let channel_id = self.channel_id;
        let result = async {
            let mut conn = self.state.db.acquire().await?;
            if saved.queue.is_empty() {
                db::music::delete_session(&mut conn, channel_id).await
            } else {
                let state_json = serde_json::to_string(&saved).expect("a session serializes");
                db::music::put_session(
                    &mut conn,
                    &db::music::SessionRow {
                        channel_id,
                        state_json,
                        updated_at: now_ms(),
                    },
                )
                .await
            }
        }
        .await;
        if let Err(error) = result {
            tracing::warn!(channel_id, %error, "a music session could not be saved");
        }
    }
}
