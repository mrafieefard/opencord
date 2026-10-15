//! Music (Phase 3 plan §15–16): its settings, members blocked from it,
//! moving players off a Lavalink node, and artwork through the server.

use opencord_proto::v1 as proto;
use proto::request::Kind as Request;
use proto::response::Result as Response;

use crate::api::types::{
    CoreError, LavalinkNodes, MusicBlock, MusicPlaylist, MusicSearchResult, MusicSearchResults,
    MusicSettings, MusicSettingsChanges, MusicTrack,
};
use crate::client::{Client, unexpected};

impl Client {
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
    }
}
