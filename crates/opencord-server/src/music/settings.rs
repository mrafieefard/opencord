//! Server settings → Music (Phase 3 plan §16.2), kept in `server_meta`.

use opencord_proto::v1 as proto;
use serde::{Deserialize, Serialize};
use sqlx::SqliteConnection;

use super::tracks::SEARCH_SOURCES;
use crate::db;
use crate::error::ApiError;

const KEY: &str = "music_settings";
/// The queue holds at most this many (§14.1).
pub const QUEUE_MAX: u32 = 500;
const TIMEOUT_MAX_S: u32 = 3600;
/// A track may be up to a day long, when limited at all.
const TRACK_MAX_MS: u64 = 24 * 3600 * 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MusicSettings {
    pub enabled: bool,
    /// Empty: every available one.
    pub enabled_sources: Vec<String>,
    pub default_source: String,
    pub allow_links: bool,
    pub allow_streams: bool,
    /// 0 for no limit.
    pub max_track_ms: u64,
    pub max_tracks_per_member: u32,
    pub max_playlist_tracks: u32,
    pub queue_limit: u32,
    pub vote_skip: bool,
    pub vote_skip_percent: u32,
    pub stop_after_empty_s: u32,
    pub disconnect_after_idle_s: u32,
    pub bridge_domain: String,
}

impl Default for MusicSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            enabled_sources: Vec::new(),
            default_source: "youtube_music".to_owned(),
            allow_links: true,
            allow_streams: true,
            max_track_ms: 0,
            max_tracks_per_member: 20,
            max_playlist_tracks: 100,
            queue_limit: QUEUE_MAX,
            vote_skip: false,
            vote_skip_percent: 50,
            stop_after_empty_s: 120,
            disconnect_after_idle_s: 300,
            bridge_domain: "primary".to_owned(),
        }
    }
}

impl MusicSettings {
    /// The saved settings; the defaults, with `opencord.toml`'s bridge
    /// domain, before any were saved.
    pub async fn load(
        conn: &mut SqliteConnection,
        bridge_domain: &str,
    ) -> Result<Self, sqlx::Error> {
        let saved = db::meta::load_prefixed(conn, KEY).await?;
        Ok(saved
            .get(KEY)
            .and_then(|json| serde_json::from_str(json).ok())
            .unwrap_or_else(|| Self {
                bridge_domain: bridge_domain.to_owned(),
                ..Self::default()
            }))
    }

    pub async fn save(&self, conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
        let json = serde_json::to_string(self).expect("music settings serialize");
        db::meta::put(conn, KEY, Some(&json)).await
    }

    /// Whether a search source may be used.
    pub fn allows_source(&self, source: &str) -> bool {
        self.enabled_sources.is_empty() || self.enabled_sources.iter().any(|known| known == source)
    }

    /// These settings with a request's changes, checked. `domain_ok` says
    /// whether a hostname is an active domain with a trusted certificate.
    pub fn apply(
        &self,
        request: &proto::UpdateMusicSettings,
        domain_ok: impl Fn(&str) -> bool,
    ) -> Result<Self, ApiError> {
        let invalid = |message: &str| Err(ApiError::invalid_argument(message.to_owned()));
        let mut next = self.clone();
        let known = |source: &str| SEARCH_SOURCES.iter().any(|(key, _, _)| *key == source);
        if let Some(enabled) = request.enabled {
            next.enabled = enabled;
        }
        if let Some(sources) = &request.enabled_sources {
            if let Some(unknown) = sources.values.iter().find(|source| !known(source)) {
                return invalid(&format!("{unknown} is not a search source"));
            }
            next.enabled_sources = sources.values.clone();
        }
        if let Some(source) = &request.default_source {
            if !known(source) {
                return invalid(&format!("{source} is not a search source"));
            }
            next.default_source = source.clone();
        }
        if let Some(allow) = request.allow_links {
            next.allow_links = allow;
        }
        if let Some(allow) = request.allow_streams {
            next.allow_streams = allow;
        }
        if let Some(max) = request.max_track_ms {
            if max > TRACK_MAX_MS {
                return invalid("the longest track limit is 24 hours");
            }
            next.max_track_ms = max;
        }
        let in_range = |value: u32, max: u32, what: &str| {
            if (1..=max).contains(&value) {
                Ok(value)
            } else {
                Err(ApiError::invalid_argument(format!(
                    "{what} is from 1 to {max}"
                )))
            }
        };
        if let Some(max) = request.max_tracks_per_member {
            next.max_tracks_per_member = in_range(max, QUEUE_MAX, "tracks per member")?;
        }
        if let Some(max) = request.max_playlist_tracks {
            next.max_playlist_tracks = in_range(max, QUEUE_MAX, "tracks from a playlist")?;
        }
        if let Some(max) = request.queue_limit {
            next.queue_limit = in_range(max, QUEUE_MAX, "the queue limit")?;
        }
        if let Some(on) = request.vote_skip {
            next.vote_skip = on;
        }
        if let Some(percent) = request.vote_skip_percent {
            next.vote_skip_percent = in_range(percent, 100, "the vote skip threshold")?;
        }
        if let Some(seconds) = request.stop_after_empty_s {
            if seconds > TIMEOUT_MAX_S {
                return invalid("stop after everyone leaves is at most an hour");
            }
            next.stop_after_empty_s = seconds;
        }
        if let Some(seconds) = request.disconnect_after_idle_s {
            if seconds > TIMEOUT_MAX_S {
                return invalid("disconnect after the queue ends is at most an hour");
            }
            next.disconnect_after_idle_s = seconds;
        }
        if let Some(domain) = &request.bridge_domain {
            let domain = domain.trim().to_ascii_lowercase();
            if domain != "primary" && !domain_ok(&domain) {
                return invalid(
                    "the bridge domain is the primary one, or an active domain with a trusted \
                     certificate",
                );
            }
            next.bridge_domain = domain;
        }
        if !next.allows_source(&next.default_source) {
            return invalid("the default source must be one of the enabled sources");
        }
        Ok(next)
    }

    pub fn to_proto(
        &self,
        available_sources: Vec<String>,
        supported: bool,
    ) -> proto::MusicSettings {
        proto::MusicSettings {
            enabled: self.enabled,
            enabled_sources: self.enabled_sources.clone(),
            default_source: self.default_source.clone(),
            allow_links: self.allow_links,
            allow_streams: self.allow_streams,
            max_track_ms: self.max_track_ms,
            max_tracks_per_member: self.max_tracks_per_member,
            max_playlist_tracks: self.max_playlist_tracks,
            queue_limit: self.queue_limit,
            vote_skip: self.vote_skip,
            vote_skip_percent: self.vote_skip_percent,
            stop_after_empty_s: self.stop_after_empty_s,
            disconnect_after_idle_s: self.disconnect_after_idle_s,
            bridge_domain: self.bridge_domain.clone(),
            available_sources,
            supported,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_are_checked() {
        let settings = MusicSettings::default();
        let request = |change: fn(&mut proto::UpdateMusicSettings)| {
            let mut request = proto::UpdateMusicSettings::default();
            change(&mut request);
            request
        };
        let changed = settings
            .apply(
                &request(|r| {
                    r.queue_limit = Some(100);
                    r.max_track_ms = Some(600_000);
                    r.enabled_sources = Some(proto::StringList {
                        values: vec!["soundcloud".to_owned(), "youtube_music".to_owned()],
                    });
                }),
                |_| false,
            )
            .unwrap();
        assert_eq!(changed.queue_limit, 100);
        assert!(changed.allows_source("soundcloud"));
        assert!(!changed.allows_source("youtube"));
        for bad in [
            request(|r| r.queue_limit = Some(501)),
            request(|r| r.queue_limit = Some(0)),
            request(|r| r.vote_skip_percent = Some(101)),
            request(|r| r.default_source = Some("myspace".to_owned())),
            request(|r| r.bridge_domain = Some("elsewhere.example".to_owned())),
            request(|r| r.stop_after_empty_s = Some(7200)),
            request(|r| {
                r.enabled_sources = Some(proto::StringList {
                    values: vec!["soundcloud".to_owned()],
                });
            }),
        ] {
            assert!(settings.apply(&bad, |_| false).is_err(), "{bad:?}");
        }
        let domain = request(|r| r.bridge_domain = Some("Music.Example.com".to_owned()));
        assert_eq!(
            settings
                .apply(&domain, |host| host == "music.example.com")
                .unwrap()
                .bridge_domain,
            "music.example.com"
        );
    }
}
