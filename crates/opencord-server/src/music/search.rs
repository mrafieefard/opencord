//! Search and links (Phase 3 plan §15.1–15.2). Results are kept on the
//! server for the user who searched, for 15 minutes, behind opaque ids:
//! clients never send encoded tracks or Lavalink identifiers.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use opencord_proto::v1 as proto;
use serde_json::Value;

use super::settings::MusicSettings;
use super::tracks::{SEARCH_SOURCES, Track, search_source, source_display_name};
use crate::error::ApiError;
use crate::lavalink::lock;
use crate::state::AppState;

/// Results are good this long.
const KEEP: Duration = Duration::from_secs(15 * 60);
/// Search results shown (§15.1).
pub const RESULTS_MAX: usize = 15;
/// Queries are at most this long.
pub const QUERY_MAX: usize = 200;
/// Results kept for everyone at once; the oldest go first.
const KEPT_MAX: usize = 20_000;

#[derive(Debug)]
struct Kept {
    user_id: i64,
    track: Track,
    expires: Instant,
}

/// Results handed out, by their id.
#[derive(Debug, Default)]
pub struct Results {
    kept: Mutex<HashMap<String, Kept>>,
}

impl Results {
    /// Keeps tracks for `user_id`; returns their ids, in order.
    pub fn keep(&self, user_id: i64, tracks: Vec<Track>) -> Vec<String> {
        let now = Instant::now();
        let mut kept = lock(&self.kept);
        kept.retain(|_, result| result.expires > now);
        if kept.len() + tracks.len() > KEPT_MAX {
            let mut oldest: Vec<(Instant, String)> = kept
                .iter()
                .map(|(id, result)| (result.expires, id.clone()))
                .collect();
            oldest.sort();
            let over = kept.len() + tracks.len() - KEPT_MAX;
            for (_, id) in oldest.into_iter().take(over) {
                kept.remove(&id);
            }
        }
        tracks
            .into_iter()
            .map(|track| {
                let id = crate::random::code(22);
                kept.insert(
                    id.clone(),
                    Kept {
                        user_id,
                        track,
                        expires: now + KEEP,
                    },
                );
                id
            })
            .collect()
    }

    /// The tracks behind `ids`, which `user_id` was given.
    pub fn tracks(&self, user_id: i64, ids: &[String]) -> Result<Vec<Track>, ApiError> {
        let now = Instant::now();
        let kept = lock(&self.kept);
        ids.iter()
            .map(|id| {
                kept.get(id)
                    .filter(|result| result.user_id == user_id && result.expires > now)
                    .map(|result| result.track.clone())
                    .ok_or_else(|| {
                        ApiError::not_found("search result (it may have expired: search again)")
                    })
            })
            .collect()
    }
}

/// What a query asks Lavalink for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lookup {
    /// Lavalink's identifier: a prefixed search or a link.
    pub identifier: String,
    /// The source manager a node needs.
    pub manager: String,
}

/// Hosts links may point at, and the source manager that plays them.
const LINK_HOSTS: [(&str, &str); 16] = [
    ("youtube.com", "youtube"),
    ("www.youtube.com", "youtube"),
    ("m.youtube.com", "youtube"),
    ("music.youtube.com", "youtube"),
    ("youtu.be", "youtube"),
    ("soundcloud.com", "soundcloud"),
    ("m.soundcloud.com", "soundcloud"),
    ("on.soundcloud.com", "soundcloud"),
    ("open.spotify.com", "spotify"),
    ("music.apple.com", "applemusic"),
    ("deezer.com", "deezer"),
    ("www.deezer.com", "deezer"),
    ("twitch.tv", "twitch"),
    ("www.twitch.tv", "twitch"),
    ("vimeo.com", "vimeo"),
    ("bandcamp.com", "bandcamp"),
];

/// The search sources a manager stands for, to apply the admin's choice.
fn sources_of(manager: &str) -> Vec<&'static str> {
    SEARCH_SOURCES
        .iter()
        .filter(|(_, _, of)| *of == manager)
        .map(|(key, _, _)| *key)
        .collect()
}

/// What to ask Lavalink for `query` from `source` (empty: the default).
pub fn lookup(
    settings: &MusicSettings,
    query: &str,
    source: &str,
    allow_unsafe: bool,
) -> Result<Lookup, ApiError> {
    let query = query.trim();
    if query.is_empty() || query.chars().count() > QUERY_MAX {
        return Err(ApiError::invalid_argument(
            "a search is 1 to 200 characters",
        ));
    }
    if query.contains("://") {
        return link(settings, query, allow_unsafe);
    }
    if allow_unsafe && query.starts_with('/') {
        // A file on the Lavalink's machine, for its `local` source.
        return Ok(Lookup {
            identifier: query.to_owned(),
            manager: "local".to_owned(),
        });
    }
    let source = if source.is_empty() {
        settings.default_source.as_str()
    } else {
        source
    };
    let not_allowed = || {
        ApiError::new(
            proto::ErrorCode::MusicSourceNotAllowed,
            format!("{source} is not a search source on this server"),
        )
    };
    let (prefix, manager) = search_source(source).ok_or_else(not_allowed)?;
    if !settings.allows_source(source) {
        return Err(not_allowed());
    }
    Ok(Lookup {
        identifier: format!("{prefix}{query}"),
        manager: manager.to_owned(),
    })
}

/// A link (§15.2): `https`, to a host of an enabled source; never an IP
/// address, `localhost` or a private name. With unsafe sources allowed
/// (`[music] allow_unsafe_sources`), any `http` or `https` link, and above
/// any file path.
fn link(settings: &MusicSettings, text: &str, allow_unsafe: bool) -> Result<Lookup, ApiError> {
    let refused = |why: &str| {
        Err(ApiError::new(
            proto::ErrorCode::MusicLinkNotAllowed,
            why.to_owned(),
        ))
    };
    if !settings.allow_links {
        return refused("Links are turned off on this server: search instead");
    }
    let Ok(url) = reqwest::Url::parse(text) else {
        return refused("That link can't be read");
    };
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if allow_unsafe && matches!(url.scheme(), "http" | "https") && !host.is_empty() {
        return Ok(Lookup {
            identifier: url.to_string(),
            manager: "http".to_owned(),
        });
    }
    if url.scheme() != "https" {
        return refused("Only https links can be played");
    }
    if !url.username().is_empty() || url.password().is_some() {
        return refused("Links with a user name can't be played");
    }
    let private = host.trim_matches(['[', ']']).parse::<IpAddr>().is_ok()
        || host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".internal")
        || host.ends_with(".lan")
        || !host.contains('.');
    if private {
        return refused("Links to addresses or private hosts can't be played");
    }
    let manager = LINK_HOSTS
        .iter()
        .find(|(known, _)| *known == host)
        .map(|(_, manager)| *manager)
        .or_else(|| host.ends_with(".bandcamp.com").then_some("bandcamp"));
    let Some(manager) = manager else {
        return refused("Links from that site can't be played here");
    };
    let sources = sources_of(manager);
    if !sources.is_empty() && !sources.iter().any(|source| settings.allows_source(source)) {
        return Err(ApiError::new(
            proto::ErrorCode::MusicSourceNotAllowed,
            format!(
                "{} is not enabled on this server",
                source_display_name(manager)
            ),
        ));
    }
    Ok(Lookup {
        identifier: url.to_string(),
        manager: manager.to_owned(),
    })
}

/// What Lavalink loaded, as tracks and maybe a playlist.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Loaded {
    pub tracks: Vec<Track>,
    pub playlist: Option<String>,
}

/// Reads `/v4/loadtracks`' answer; at most `RESULTS_MAX` results, or
/// `playlist_max` tracks of a playlist. Refuses Lavalink's errors.
pub fn read_loaded(value: &Value, playlist_max: usize) -> Result<Loaded, ApiError> {
    let tracks = |list: &Value, max: usize| -> Vec<Track> {
        list.as_array()
            .map(|list| {
                list.iter()
                    .filter_map(Track::from_lavalink)
                    .take(max)
                    .collect()
            })
            .unwrap_or_default()
    };
    let data = &value["data"];
    match value["loadType"].as_str() {
        Some("track") => Ok(Loaded {
            tracks: Track::from_lavalink(data).into_iter().collect(),
            playlist: None,
        }),
        Some("search") => Ok(Loaded {
            tracks: tracks(data, RESULTS_MAX),
            playlist: None,
        }),
        Some("playlist") => Ok(Loaded {
            tracks: tracks(&data["tracks"], playlist_max),
            playlist: Some(
                data["info"]["name"]
                    .as_str()
                    .unwrap_or("Playlist")
                    .chars()
                    .take(200)
                    .collect(),
            ),
        }),
        Some("error") => Err(ApiError::invalid_argument(format!(
            "The music server couldn't load that: {}",
            data["message"].as_str().unwrap_or("no reason given")
        ))),
        _ => Ok(Loaded::default()),
    }
}

/// Searches, or loads a link, on the best node with its source; once more
/// on the next best when that fails (§12.6).
pub async fn search(
    state: &AppState,
    user_id: i64,
    request: &proto::MusicSearch,
) -> Result<proto::MusicSearchResults, ApiError> {
    let settings = state.music.settings();
    let allow_unsafe = state.config.music.allow_unsafe_sources;
    let lookup = lookup(&settings, &request.query, &request.source, allow_unsafe)?;
    let playlist_max = usize::try_from(settings.max_playlist_tracks).unwrap_or(usize::MAX);
    let mut tried = Vec::new();
    let mut last_failure = None;
    for _ in 0..2 {
        let Some(node) = state.lavalink.best_node(Some(&lookup.manager), &tried) else {
            break;
        };
        tried.push(node.id);
        let Some((rest, _)) = node.rest(state) else {
            continue;
        };
        match rest.load_tracks(&lookup.identifier).await {
            Ok(value) => {
                let loaded = read_loaded(&value, playlist_max)?;
                return Ok(results(state, user_id, loaded, allow_unsafe));
            }
            Err(failure) => {
                tracing::info!(
                    node_id = node.id,
                    reason = failure.text(),
                    "a music search failed on a Lavalink node"
                );
                last_failure = Some(failure.text());
            }
        }
    }
    Err(ApiError::new(
        proto::ErrorCode::MusicNodeUnavailable,
        match last_failure {
            Some(_) => "The music servers couldn't search right now: try again".to_owned(),
            None => format!(
                "No music server offers {} right now",
                source_display_name(&lookup.manager)
            ),
        },
    ))
}

fn results(
    state: &AppState,
    user_id: i64,
    loaded: Loaded,
    allow_unsafe: bool,
) -> proto::MusicSearchResults {
    let tracks: Vec<Track> = loaded
        .tracks
        .into_iter()
        .filter(|track| allow_unsafe || !track.is_unsafe_source())
        .collect();
    let shown: Vec<proto::MusicTrack> = tracks.iter().map(Track::to_proto).collect();
    let ids = state.music.results.keep(user_id, tracks);
    let playlist = loaded.playlist.map(|name| proto::MusicPlaylist {
        name,
        result_ids: ids.clone(),
    });
    let results = ids
        .into_iter()
        .zip(shown)
        .take(if playlist.is_some() {
            RESULTS_MAX
        } else {
            usize::MAX
        })
        .map(|(result_id, track)| proto::MusicSearchResult {
            result_id,
            track: Some(track),
        })
        .collect();
    proto::MusicSearchResults { results, playlist }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn settings() -> MusicSettings {
        MusicSettings::default()
    }

    #[test]
    fn words_search_the_chosen_or_default_source() {
        let found = lookup(&settings(), " never gonna ", "", false).unwrap();
        assert_eq!(found.identifier, "ytmsearch:never gonna");
        assert_eq!(found.manager, "youtube");
        let found = lookup(&settings(), "lofi", "soundcloud", false).unwrap();
        assert_eq!(found.identifier, "scsearch:lofi");
        let error = lookup(&settings(), "x", "myspace", false).unwrap_err();
        assert_eq!(error.code, proto::ErrorCode::MusicSourceNotAllowed);
        let only_soundcloud = MusicSettings {
            enabled_sources: vec!["soundcloud".to_owned()],
            default_source: "soundcloud".to_owned(),
            ..settings()
        };
        let error = lookup(&only_soundcloud, "x", "youtube", false).unwrap_err();
        assert_eq!(error.code, proto::ErrorCode::MusicSourceNotAllowed);
        assert!(lookup(&settings(), "", "", false).is_err());
        assert!(lookup(&settings(), &"a".repeat(201), "", false).is_err());
    }

    #[test]
    fn links_need_https_and_a_known_public_host() {
        let ok = lookup(
            &settings(),
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "",
            false,
        )
        .unwrap();
        assert_eq!(ok.manager, "youtube");
        assert_eq!(
            lookup(
                &settings(),
                "https://artist.bandcamp.com/track/x",
                "",
                false
            )
            .unwrap()
            .manager,
            "bandcamp"
        );
        for refused in [
            "http://www.youtube.com/watch?v=x",
            "https://127.0.0.1/a.mp3",
            "https://[::1]/a.mp3",
            "https://localhost/a.mp3",
            "https://nas.local/a.mp3",
            "https://intranet/a.mp3",
            "https://user:pw@youtube.com/watch?v=x",
            "https://example.com/a.mp3",
            "file:///etc/passwd",
        ] {
            let error = lookup(&settings(), refused, "", false).unwrap_err();
            assert_eq!(
                error.code,
                proto::ErrorCode::MusicLinkNotAllowed,
                "{refused}"
            );
        }
        let no_links = MusicSettings {
            allow_links: false,
            ..settings()
        };
        let error = lookup(&no_links, "https://youtu.be/x", "", false).unwrap_err();
        assert_eq!(error.code, proto::ErrorCode::MusicLinkNotAllowed);
        let unsafe_ok = lookup(&settings(), "http://127.0.0.1:8080/a.mp3", "", true).unwrap();
        assert_eq!(unsafe_ok.manager, "http");
        assert_eq!(
            lookup(&settings(), "/srv/music/a.ogg", "", true)
                .unwrap()
                .manager,
            "local"
        );
        assert_eq!(
            lookup(&settings(), "/srv/music/a.ogg", "", false)
                .unwrap()
                .identifier,
            "ytmsearch:/srv/music/a.ogg",
            "only a search without unsafe sources"
        );
    }

    #[test]
    fn results_are_kept_for_their_user_only() {
        let results = Results::default();
        let track =
            Track::from_lavalink(&json!({ "encoded": "e", "info": { "title": "t" } })).unwrap();
        let ids = results.keep(7, vec![track.clone()]);
        assert_eq!(results.tracks(7, &ids).unwrap(), vec![track]);
        assert!(results.tracks(8, &ids).is_err());
        assert!(results.tracks(7, &["made-up".to_owned()]).is_err());
    }

    #[test]
    fn lavalink_answers_are_read() {
        let track = json!({ "encoded": "e1", "info": { "title": "One", "sourceName": "youtube" } });
        let search = read_loaded(
            &json!({ "loadType": "search", "data": vec![track.clone(); 20] }),
            100,
        )
        .unwrap();
        assert_eq!(search.tracks.len(), RESULTS_MAX);
        let playlist = read_loaded(
            &json!({ "loadType": "playlist", "data": {
                "info": { "name": "Mix", "selectedTrack": -1 },
                "tracks": vec![track.clone(); 8] } }),
            5,
        )
        .unwrap();
        assert_eq!(playlist.playlist.as_deref(), Some("Mix"));
        assert_eq!(playlist.tracks.len(), 5);
        assert!(
            read_loaded(&json!({ "loadType": "empty", "data": {} }), 5)
                .unwrap()
                .tracks
                .is_empty()
        );
        assert!(
            read_loaded(
                &json!({ "loadType": "error", "data": { "message": "x", "severity": "common" } }),
                5
            )
            .is_err()
        );
    }
}
