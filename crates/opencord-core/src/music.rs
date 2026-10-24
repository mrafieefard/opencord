//! Music (Phase 3 plan §15–16): its settings, members blocked from it,
//! moving players off a Lavalink node, and artwork through the server.

use std::collections::HashMap;

use opencord_proto::music_sync;
use opencord_proto::v1 as proto;
use proto::request::Kind as Request;
use proto::response::Result as Response;

use crate::api::types::{
    CoreError, CoreEventPayload, LavalinkNodes, LocalSong, MusicAction, MusicBlock, MusicNotice,
    MusicNowPlaying, MusicPlacement, MusicPlayState, MusicPlaylist, MusicQueueItem, MusicRepeat,
    MusicSearchResult, MusicSearchResults, MusicSession, MusicSettings, MusicSettingsChanges,
    MusicTrack,
};
use crate::client::{Client, unexpected};

/// How long the music takes from the voice node to the speakers: the
/// network and the jitter buffer. Progress shows what is heard.
const PLAYOUT_DELAY_MS: i64 = 80;

impl Client {
    /// The channel's player, and its changes from now on even when not
    /// connected to the channel. Needs only to see the channel.
    pub async fn music_subscribe(
        &self,
        key: &str,
        channel_id: i64,
        subscribe: bool,
    ) -> Result<MusicSession, CoreError> {
        let request = Request::MusicSubscribe(proto::MusicSubscribe {
            channel_id,
            subscribe,
        });
        self.music_request(key, request).await
    }

    /// Adds search results, or tracks from the channel's history by their
    /// item ids: at the end, before an item, next or now.
    pub async fn music_enqueue(
        &self,
        key: &str,
        channel_id: i64,
        result_ids: Vec<String>,
        history_item_ids: Vec<String>,
        before_item_id: Option<String>,
        placement: MusicPlacement,
    ) -> Result<MusicSession, CoreError> {
        let placement = match placement {
            MusicPlacement::End => proto::MusicPlacement::End,
            MusicPlacement::Next => proto::MusicPlacement::Next,
            MusicPlacement::Now => proto::MusicPlacement::Now,
        };
        let request = Request::MusicEnqueue(proto::MusicEnqueue {
            channel_id,
            result_ids,
            before_item_id,
            placement: placement as i32,
            history_item_ids,
        });
        self.music_request(key, request).await
    }

    /// With `expected_item_id`, nothing happens when another item plays
    /// by now (an error with code `MusicStale`).
    pub async fn music_playback(
        &self,
        key: &str,
        channel_id: i64,
        action: MusicAction,
        position_ms: Option<u64>,
        expected_item_id: Option<String>,
    ) -> Result<MusicSession, CoreError> {
        let action = match action {
            MusicAction::Pause => proto::MusicAction::Pause,
            MusicAction::Resume => proto::MusicAction::Resume,
            MusicAction::Next => proto::MusicAction::Next,
            MusicAction::Previous => proto::MusicAction::Previous,
            MusicAction::Seek => proto::MusicAction::Seek,
            MusicAction::Stop => proto::MusicAction::Stop,
        };
        let request = Request::MusicPlayback(proto::MusicPlayback {
            channel_id,
            action: action as i32,
            position_ms,
            expected_item_id,
        });
        self.music_request(key, request).await
    }

    pub async fn music_set_mode(
        &self,
        key: &str,
        channel_id: i64,
        repeat: Option<MusicRepeat>,
        shuffle: Option<bool>,
    ) -> Result<MusicSession, CoreError> {
        let request = Request::MusicSetMode(proto::MusicSetMode {
            channel_id,
            repeat: repeat.map(|repeat| repeat_to_proto(repeat) as i32),
            shuffle,
        });
        self.music_request(key, request).await
    }

    /// Puts an item before another, or at the end.
    pub async fn music_queue_move(
        &self,
        key: &str,
        channel_id: i64,
        item_id: String,
        before_item_id: Option<String>,
    ) -> Result<MusicSession, CoreError> {
        let request = Request::MusicQueueMove(proto::MusicQueueMove {
            channel_id,
            item_id,
            before_item_id,
        });
        self.music_request(key, request).await
    }

    pub async fn music_queue_remove(
        &self,
        key: &str,
        channel_id: i64,
        item_ids: Vec<String>,
    ) -> Result<MusicSession, CoreError> {
        let request = Request::MusicQueueRemove(proto::MusicQueueRemove {
            channel_id,
            item_ids,
        });
        self.music_request(key, request).await
    }

    pub async fn music_queue_clear(
        &self,
        key: &str,
        channel_id: i64,
    ) -> Result<MusicSession, CoreError> {
        let request = Request::MusicQueueClear(proto::MusicQueueClear { channel_id });
        self.music_request(key, request).await
    }

    pub async fn music_vote_skip(
        &self,
        key: &str,
        channel_id: i64,
        expected_item_id: String,
    ) -> Result<MusicSession, CoreError> {
        let request = Request::MusicVoteSkip(proto::MusicVoteSkip {
            channel_id,
            expected_item_id,
        });
        self.music_request(key, request).await
    }

    async fn music_request(&self, key: &str, request: Request) -> Result<MusicSession, CoreError> {
        match self.request(key, request).await? {
            Response::MusicSession(session) => {
                Ok(session_from(*session, self.server_clock_offset(key)))
            }
            other => Err(unexpected(&other)),
        }
    }

    /// Queues the song at `path`, uploading it first when the server
    /// doesn't have it (design 2026-10-24); `MusicUploadProgress` events
    /// follow, under `upload_id`. `play_now` plays it at once.
    pub async fn music_upload_song(
        &self,
        key: &str,
        channel_id: i64,
        upload_id: String,
        path: String,
        play_now: bool,
    ) -> Result<(), CoreError> {
        crate::attachments::song::upload_song(self, key, channel_id, upload_id, path, play_now)
            .await
    }

    /// Stops a song's upload; it returns `Cancelled`.
    pub fn music_cancel_song(&self, key: &str, upload_id: &str) {
        crate::attachments::song::cancel_song(self, key, upload_id);
    }

    /// An owner stops owning the song, which goes when nobody owns it; an
    /// administrator removes it for everyone.
    pub async fn remove_local_song(&self, key: &str, song_id: i64) -> Result<(), CoreError> {
        let request = Request::RemoveLocalSong(proto::RemoveLocalSong { song_id });
        match self.request(key, request).await? {
            Response::Ack(_) => Ok(()),
            other => Err(unexpected(&other)),
        }
    }

    /// Every uploaded song, newest first, 50 at a time; administrators only.
    pub async fn fetch_local_songs(
        &self,
        key: &str,
        query: String,
        before_id: Option<i64>,
    ) -> Result<Vec<LocalSong>, CoreError> {
        let request = Request::FetchLocalSongs(proto::FetchLocalSongs { query, before_id });
        match self.request(key, request).await? {
            Response::LocalSongs(songs) => Ok(songs
                .songs
                .into_iter()
                .map(|song| LocalSong {
                    id: song.id,
                    title: song.title,
                    artist: song.artist,
                    duration_ms: song.duration_ms,
                    size_bytes: song.size_bytes,
                    filename: song.filename,
                    uploader_id: song.uploader_id,
                    created_at_ms: song.created_at_ms,
                })
                .collect()),
            other => Err(unexpected(&other)),
        }
    }

    /// Searches, or loads a link, for the voice channel; `source` empty for
    /// the server's default. Needs Add music.
    pub async fn music_search(
        &self,
        key: &str,
        channel_id: i64,
        query: String,
        source: String,
    ) -> Result<MusicSearchResults, CoreError> {
        let request = Request::MusicSearch(proto::MusicSearch {
            channel_id,
            query,
            source,
        });
        match self.request(key, request).await? {
            Response::MusicSearchResults(found) => Ok(MusicSearchResults {
                results: found
                    .results
                    .into_iter()
                    .map(|result| MusicSearchResult {
                        result_id: result.result_id,
                        track: track_from(result.track.unwrap_or_default()),
                    })
                    .collect(),
                playlist: found.playlist.map(|playlist| MusicPlaylist {
                    name: playlist.name,
                    result_ids: playlist.result_ids,
                }),
            }),
            other => Err(unexpected(&other)),
        }
    }

    /// A track's artwork from the server's proxy, at 128 or 512 px: the
    /// path of a file kept on this device.
    pub async fn music_artwork(
        &self,
        key: &str,
        artwork_id: &str,
        size: u32,
    ) -> Result<String, CoreError> {
        self.artwork().get(self, key, artwork_id, size).await
    }

    pub async fn music_settings(&self, key: &str) -> Result<MusicSettings, CoreError> {
        let request = Request::FetchMusicSettings(proto::FetchMusicSettings {});
        settings(self.request(key, request).await?)
    }

    /// Needs Manage music.
    pub async fn update_music_settings(
        &self,
        key: &str,
        changes: MusicSettingsChanges,
    ) -> Result<MusicSettings, CoreError> {
        let request = Request::UpdateMusicSettings(proto::UpdateMusicSettings {
            enabled: changes.enabled,
            enabled_sources: changes
                .enabled_sources
                .map(|values| proto::StringList { values }),
            default_source: changes.default_source,
            allow_links: changes.allow_links,
            allow_streams: changes.allow_streams,
            max_track_ms: changes.max_track_ms,
            max_tracks_per_member: changes.max_tracks_per_member,
            max_playlist_tracks: changes.max_playlist_tracks,
            queue_limit: changes.queue_limit,
            vote_skip: changes.vote_skip,
            vote_skip_percent: changes.vote_skip_percent,
            stop_after_empty_s: changes.stop_after_empty_s,
            disconnect_after_idle_s: changes.disconnect_after_idle_s,
            bridge_domain: changes.bridge_domain,
        });
        settings(self.request(key, request).await?)
    }

    /// Needs Manage music.
    pub async fn music_blocks(&self, key: &str) -> Result<Vec<MusicBlock>, CoreError> {
        let request = Request::FetchMusicBlocks(proto::FetchMusicBlocks {});
        blocks(self.request(key, request).await?)
    }

    /// Blocks a member from music, or unblocks them. Needs Manage music.
    pub async fn block_from_music(
        &self,
        key: &str,
        user_id: i64,
        blocked: bool,
        reason: String,
    ) -> Result<Vec<MusicBlock>, CoreError> {
        let request = Request::MusicBlockMember(proto::MusicBlockMember {
            user_id,
            blocked,
            reason,
        });
        blocks(self.request(key, request).await?)
    }

    /// Moves the channels on a node now: to `to_node_id`, or each to the
    /// best other node. Needs Manage server.
    pub async fn move_lavalink_players(
        &self,
        key: &str,
        from_node_id: i64,
        to_node_id: Option<i64>,
    ) -> Result<LavalinkNodes, CoreError> {
        let request = Request::MoveLavalinkPlayers(proto::MoveLavalinkPlayers {
            from_node_id,
            to_node_id,
        });
        match self.request(key, request).await? {
            Response::LavalinkNodes(nodes) => Ok(crate::lavalink::nodes_from(*nodes)),
            other => Err(unexpected(&other)),
        }
    }
}

fn settings(response: Response) -> Result<MusicSettings, CoreError> {
    match response {
        Response::MusicSettings(settings) => Ok(settings_from(*settings)),
        other => Err(unexpected(&other)),
    }
}

fn blocks(response: Response) -> Result<Vec<MusicBlock>, CoreError> {
    match response {
        Response::MusicBlocks(blocks) => Ok(blocks
            .blocks
            .into_iter()
            .map(|block| MusicBlock {
                user_id: block.user_id,
                reason: block.reason,
                blocked_by: block.blocked_by,
                created_at_ms: block.created_at_ms,
            })
            .collect()),
        other => Err(unexpected(&other)),
    }
}

pub(crate) fn settings_from(settings: proto::MusicSettings) -> MusicSettings {
    MusicSettings {
        enabled: settings.enabled,
        enabled_sources: settings.enabled_sources,
        default_source: settings.default_source,
        allow_links: settings.allow_links,
        allow_streams: settings.allow_streams,
        max_track_ms: settings.max_track_ms,
        max_tracks_per_member: settings.max_tracks_per_member,
        max_playlist_tracks: settings.max_playlist_tracks,
        queue_limit: settings.queue_limit,
        vote_skip: settings.vote_skip,
        vote_skip_percent: settings.vote_skip_percent,
        stop_after_empty_s: settings.stop_after_empty_s,
        disconnect_after_idle_s: settings.disconnect_after_idle_s,
        bridge_domain: settings.bridge_domain,
        available_sources: settings.available_sources,
        supported: settings.supported,
        configured: settings.configured,
        has_nodes: settings.has_nodes,
    }
}

pub(crate) fn track_from(track: proto::MusicTrack) -> MusicTrack {
    MusicTrack {
        title: track.title,
        artist: track.artist,
        duration_ms: track.duration_ms,
        is_stream: track.is_stream,
        is_seekable: track.is_seekable,
        source: track.source,
        source_display_name: track.source_display_name,
        uri: track.uri,
        artwork_id: track.artwork_id,
        local_song_id: track.local_song_id,
        removable: track.removable,
    }
}

fn repeat_to_proto(repeat: MusicRepeat) -> proto::MusicRepeat {
    match repeat {
        MusicRepeat::Off => proto::MusicRepeat::Off,
        MusicRepeat::Queue => proto::MusicRepeat::Queue,
        MusicRepeat::Track => proto::MusicRepeat::Track,
    }
}

fn item_from(item: proto::MusicQueueItem) -> MusicQueueItem {
    MusicQueueItem {
        item_id: item.item_id,
        track: track_from(item.track.unwrap_or_default()),
        requester_id: item.requester_id,
        added_at_ms: item.added_at_ms,
    }
}

/// A session as the app sees it: its anchor on this device's clock, when
/// the server's is `offset_ms` ahead, and later by the playout delay.
pub(crate) fn session_from(session: proto::MusicSession, offset_ms: i64) -> MusicSession {
    let state = match proto::MusicPlayState::try_from(session.state).unwrap_or_default() {
        proto::MusicPlayState::Loading => MusicPlayState::Loading,
        proto::MusicPlayState::Playing => MusicPlayState::Playing,
        proto::MusicPlayState::Paused => MusicPlayState::Paused,
        proto::MusicPlayState::Idle | proto::MusicPlayState::Unspecified => MusicPlayState::Idle,
    };
    let repeat = match proto::MusicRepeat::try_from(session.repeat).unwrap_or_default() {
        proto::MusicRepeat::Queue => MusicRepeat::Queue,
        proto::MusicRepeat::Track => MusicRepeat::Track,
        proto::MusicRepeat::Off | proto::MusicRepeat::Unspecified => MusicRepeat::Off,
    };
    MusicSession {
        channel_id: session.channel_id,
        version: session.version,
        state,
        now_playing: session.now_playing.map(item_from),
        queue: session.queue.into_iter().map(item_from).collect(),
        history: session.history.into_iter().map(item_from).collect(),
        position_ms: session.position_ms,
        anchor_ms: session.server_time_ms - offset_ms + PLAYOUT_DELAY_MS,
        repeat,
        shuffle: session.shuffle,
        waiting_for_node: session.waiting_for_node,
        paused_empty: session.paused_empty,
        skip_votes: session.skip_votes,
        skip_votes_needed: session.skip_votes_needed,
    }
}

/// The now-playing card and notices. Sessions follow the connection's
/// [`MusicSessions`].
pub(crate) fn event(kind: proto::event::Kind) -> Option<CoreEventPayload> {
    use proto::event::Kind;
    Some(match kind {
        Kind::MusicNowPlaying(card) => CoreEventPayload::MusicNowPlaying(MusicNowPlaying {
            channel_id: card.channel_id,
            title: card.title,
            artist: card.artist,
            artwork_id: card.artwork_id,
            playing: card.playing,
            ended: card.ended,
            source_display_name: card.source_display_name,
        }),
        Kind::MusicNotice(notice) => CoreEventPayload::MusicNotice(MusicNotice {
            channel_id: notice.channel_id,
            text: notice.text,
            at_ms: notice.at_ms,
        }),
        _ => return None,
    })
}

/// The music sessions a connection follows, as last heard (§14.3).
/// Updates apply to them; one that does not follow asks for the session
/// again.
#[derive(Debug, Default)]
pub(crate) struct MusicSessions {
    held: HashMap<i64, proto::MusicSession>,
    /// Channels whose session is to be asked for again.
    missed: Vec<i64>,
}

impl MusicSessions {
    /// A whole session, from a snapshot or an answer, kept unless an even
    /// newer one is at hand. Returns whether it was.
    pub fn keep(&mut self, session: &proto::MusicSession) -> bool {
        let newer = self
            .held
            .get(&session.channel_id)
            .is_none_or(|held| held.version <= session.version);
        if newer {
            self.held.insert(session.channel_id, session.clone());
            self.missed
                .retain(|channel_id| *channel_id != session.channel_id);
        }
        newer
    }

    /// The session after `update`, when it applied.
    pub fn update(&mut self, update: &proto::MusicUpdate) -> Option<proto::MusicSession> {
        let applied = self
            .held
            .get_mut(&update.channel_id)
            .map(|held| music_sync::apply(held, update));
        match applied {
            Some(Ok(true)) => self.held.get(&update.channel_id).cloned(),
            Some(Ok(false)) => None,
            Some(Err(music_sync::Missed)) | None => {
                if !self.missed.contains(&update.channel_id) {
                    self.missed.push(update.channel_id);
                }
                None
            }
        }
    }

    /// The channels to ask for again, once each.
    pub fn take_missed(&mut self) -> Vec<i64> {
        std::mem::take(&mut self.missed)
    }

    /// A fresh session starts from snapshots again.
    pub fn clear(&mut self) {
        self.held.clear();
        self.missed.clear();
    }
}

#[cfg(test)]
mod sessions_tests {
    use super::*;

    fn session(version: u64, queue: &[&str]) -> proto::MusicSession {
        proto::MusicSession {
            channel_id: 9,
            version,
            queue: queue
                .iter()
                .map(|id| proto::MusicQueueItem {
                    item_id: (*id).to_owned(),
                    ..proto::MusicQueueItem::default()
                })
                .collect(),
            ..proto::MusicSession::default()
        }
    }

    #[test]
    fn updates_apply_to_what_is_held_and_a_gap_asks_once() {
        let mut sessions = MusicSessions::default();
        let (v1, v2, v3) = (
            session(1, &["a"]),
            session(2, &["a", "b"]),
            session(3, &["b"]),
        );

        // Nothing held yet: asked for.
        assert_eq!(sessions.update(&music_sync::diff(&v1, &v2)), None);
        assert_eq!(sessions.take_missed(), [9]);

        assert!(sessions.keep(&v1));
        assert_eq!(
            sessions.update(&music_sync::diff(&v1, &v2)),
            Some(v2.clone())
        );
        // Already applied: nothing.
        assert_eq!(sessions.update(&music_sync::diff(&v1, &v2)), None);
        assert!(sessions.take_missed().is_empty());
        // An older answer does not replace a newer session.
        assert!(!sessions.keep(&v1));

        let v4 = session(4, &[]);
        assert_eq!(sessions.update(&music_sync::diff(&v3, &v4)), None);
        assert_eq!(sessions.update(&music_sync::diff(&v3, &v4)), None);
        assert_eq!(sessions.take_missed(), [9], "once");
        assert!(sessions.keep(&v4));
        assert!(sessions.take_missed().is_empty());
    }
}
