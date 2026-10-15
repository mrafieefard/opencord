//! Tracks as Lavalink describes them (Phase 3 plan §15.3), and as clients
//! see them: without the encoded track, which stays on the server.

use opencord_proto::v1 as proto;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A playable track: Lavalink's encoded form and what it says about it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub encoded: String,
    pub info: TrackInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackInfo {
    pub identifier: String,
    pub title: String,
    pub author: String,
    pub length_ms: u64,
    pub is_stream: bool,
    pub is_seekable: bool,
    pub uri: String,
    pub artwork_url: String,
    pub source_name: String,
}

impl Track {
    /// A track from Lavalink's JSON; `None` when it is not one.
    pub fn from_lavalink(value: &Value) -> Option<Self> {
        let encoded = value["encoded"].as_str()?.to_owned();
        let info = &value["info"];
        let text = |key: &str| info[key].as_str().unwrap_or_default().to_owned();
        Some(Self {
            encoded,
            info: TrackInfo {
                identifier: text("identifier"),
                title: clip(&text("title"), 200),
                author: clip(&text("author"), 200),
                length_ms: info["length"].as_u64().unwrap_or(0),
                is_stream: info["isStream"].as_bool().unwrap_or(false),
                is_seekable: info["isSeekable"].as_bool().unwrap_or(false),
                uri: text("uri"),
                artwork_url: text("artworkUrl"),
                source_name: text("sourceName"),
            },
        })
    }

    pub fn to_proto(&self) -> proto::MusicTrack {
        let info = &self.info;
        proto::MusicTrack {
            title: info.title.clone(),
            artist: info.author.clone(),
            duration_ms: if info.is_stream { 0 } else { info.length_ms },
            is_stream: info.is_stream,
            is_seekable: info.is_seekable && !info.is_stream,
            source: info.source_name.clone(),
            source_display_name: source_display_name(&info.source_name),
            uri: info.uri.clone(),
            artwork_id: String::new(),
        }
    }

    /// Sources that let users make Lavalink fetch any address or file.
    pub fn is_unsafe_source(&self) -> bool {
        matches!(self.info.source_name.as_str(), "http" | "local")
    }
}

fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// A source as a pill shows it (§15.3): known names, else capitalised.
pub fn source_display_name(source: &str) -> String {
    match source {
        "youtube" => "YouTube".to_owned(),
        "soundcloud" => "SoundCloud".to_owned(),
        "bandcamp" => "Bandcamp".to_owned(),
        "twitch" => "Twitch".to_owned(),
        "vimeo" => "Vimeo".to_owned(),
        "spotify" => "Spotify".to_owned(),
        "applemusic" => "Apple Music".to_owned(),
        "deezer" => "Deezer".to_owned(),
        "http" => "Link".to_owned(),
        "local" => "File".to_owned(),
        other => {
            let mut chars = other.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        }
    }
}

/// Search sources (§15.1): the name clients use, Lavalink's prefix, and
/// the source manager a node needs for it.
pub const SEARCH_SOURCES: [(&str, &str, &str); 7] = [
    ("youtube_music", "ytmsearch:", "youtube"),
    ("youtube", "ytsearch:", "youtube"),
    ("soundcloud", "scsearch:", "soundcloud"),
    ("bandcamp", "bcsearch:", "bandcamp"),
    ("spotify", "spsearch:", "spotify"),
    ("applemusic", "amsearch:", "applemusic"),
    ("deezer", "dzsearch:", "deezer"),
];

/// The prefix and source manager of a search source.
pub fn search_source(name: &str) -> Option<(&'static str, &'static str)> {
    SEARCH_SOURCES
        .iter()
        .find(|(key, _, _)| *key == name)
        .map(|(_, prefix, manager)| (*prefix, *manager))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn lavalink_tracks_are_read_and_shown_without_their_encoding() {
        let track = Track::from_lavalink(&json!({
            "encoded": "QAAA...",
            "info": {
                "identifier": "dQw4w9WgXcQ",
                "isSeekable": true,
                "author": "Rick Astley",
                "length": 212_000,
                "isStream": false,
                "position": 0,
                "title": "Never Gonna Give You Up",
                "uri": "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
                "artworkUrl": "https://i.ytimg.com/vi/dQw4w9WgXcQ/maxresdefault.jpg",
                "isrc": null,
                "sourceName": "youtube"
            },
            "pluginInfo": {},
            "userData": {}
        }))
        .unwrap();
        let shown = track.to_proto();
        assert_eq!(shown.title, "Never Gonna Give You Up");
        assert_eq!(shown.artist, "Rick Astley");
        assert_eq!(shown.duration_ms, 212_000);
        assert_eq!(shown.source_display_name, "YouTube");
        assert!(shown.is_seekable);
        assert!(!format!("{shown:?}").contains("QAAA"));
        assert!(!track.is_unsafe_source());
        assert!(Track::from_lavalink(&json!({ "info": {} })).is_none());
    }

    #[test]
    fn streams_have_no_length_and_cannot_seek() {
        let track = Track::from_lavalink(&json!({
            "encoded": "x",
            "info": { "length": 9_223_372_036_854_775_807u64, "isStream": true,
                      "isSeekable": true, "sourceName": "twitch" }
        }))
        .unwrap();
        let shown = track.to_proto();
        assert_eq!(shown.duration_ms, 0);
        assert!(!shown.is_seekable);
    }

    #[test]
    fn sources_are_named() {
        assert_eq!(source_display_name("applemusic"), "Apple Music");
        assert_eq!(source_display_name("nico"), "Nico");
        assert_eq!(
            search_source("youtube_music"),
            Some(("ytmsearch:", "youtube"))
        );
        assert_eq!(search_source("myspace"), None);
    }
}
