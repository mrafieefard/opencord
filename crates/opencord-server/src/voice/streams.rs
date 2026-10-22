//! Screen shares in progress (Phase 2 plan §9): at most one per user, in
//! the voice channel they are in. Kept in memory with the voice states;
//! watching is opt-in, up to the server's viewer limit, and the voice node
//! forwards a screen only to its viewers (§6).

use std::collections::{BTreeSet, HashMap};

use opencord_common::limits::SCREEN_SHARE_FPS_CHOICES;
use opencord_proto::v1 as proto;

use opencord_common::permissions::Permissions;
use opencord_voice::node::NodeCommand;
use proto::event::Kind;

use super::settings::{Resolution, VoiceSettings};
use super::states::VoiceState;
use crate::error::ApiError;
use crate::guild::Guild;
use crate::permissions::require_channel;
use crate::state::{AppState, Audience};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stream {
    pub channel_id: i64,
    pub user_id: i64,
    pub source_kind: proto::StreamSourceKind,
    pub resolution: Resolution,
    pub fps: u32,
    pub has_audio: bool,
    pub viewers: BTreeSet<i64>,
}

impl Stream {
    pub fn key(&self) -> String {
        key(self.channel_id, self.user_id)
    }

    pub fn to_proto(&self) -> proto::Stream {
        proto::Stream {
            stream_key: self.key(),
            channel_id: self.channel_id,
            user_id: self.user_id,
            source_kind: self.source_kind as i32,
            resolution: self.resolution.to_proto() as i32,
            fps: self.fps,
            has_audio: self.has_audio,
            viewer_count: u32::try_from(self.viewers.len()).unwrap_or(u32::MAX),
        }
    }
}

/// "stream:<channel_id>:<user_id>" (plan §3).
pub fn key(channel_id: i64, user_id: i64) -> String {
    format!("stream:{channel_id}:{user_id}")
}

/// The channel and user of a stream key.
pub fn parse_key(key: &str) -> Option<(i64, i64)> {
    let rest = key.strip_prefix("stream:")?;
    let (channel_id, user_id) = rest.split_once(':')?;
    Some((channel_id.parse().ok()?, user_id.parse().ok()?))
}

#[derive(Debug, Default)]
pub struct Streams {
    by_user: HashMap<i64, Stream>,
}

impl Streams {
    pub fn of(&self, user_id: i64) -> Option<&Stream> {
        self.by_user.get(&user_id)
    }

    /// The stream a key names, if it is still live.
    pub fn by_key(&self, key: &str) -> Option<&Stream> {
        let (channel_id, user_id) = parse_key(key)?;
        self.of(user_id)
            .filter(|stream| stream.channel_id == channel_id)
    }

    pub fn all(&self) -> impl Iterator<Item = &Stream> {
        self.by_user.values()
    }

    /// Every stream in the voice channels `user_id` can view.
    pub fn visible_to(&self, guild: &Guild, user_id: i64) -> Vec<proto::Stream> {
        self.all()
            .filter(|stream| guild.can_view(user_id, stream.channel_id))
            .map(Stream::to_proto)
            .collect()
    }

    /// Stores the stream, replacing the user's previous one.
    pub fn put(&mut self, stream: Stream) {
        self.by_user.insert(stream.user_id, stream);
    }

    pub fn remove(&mut self, user_id: i64) -> Option<Stream> {
        self.by_user.remove(&user_id)
    }

    /// Adds or removes a viewer; the stream as it is now, if it changed.
    pub fn set_viewer(&mut self, streamer: i64, viewer: i64, watching: bool) -> Option<Stream> {
        let stream = self.by_user.get_mut(&streamer)?;
        let changed = match watching {
            true => stream.viewers.insert(viewer),
            false => stream.viewers.remove(&viewer),
        };
        changed.then(|| stream.clone())
    }

    /// Streams in `channel_id` that `viewer` was watching; it no longer
    /// is. Returns them as they are now.
    pub fn stop_watching_in(&mut self, channel_id: i64, viewer: i64) -> Vec<Stream> {
        self.by_user
            .values_mut()
            .filter_map(|stream| {
                let was = stream.channel_id == channel_id && stream.viewers.remove(&viewer);
                was.then(|| stream.clone())
            })
            .collect()
    }
}

/// Whether a resolution and frame rate are within the server's maximum
/// (plan §9.2).
pub fn check_quality(
    settings: &VoiceSettings,
    resolution: i32,
    fps: u32,
) -> Result<Resolution, ApiError> {
    let resolution = Resolution::from_proto(resolution)
        .ok_or_else(|| ApiError::invalid_argument("choose a resolution"))?;
    if !SCREEN_SHARE_FPS_CHOICES.contains(&fps) {
        return Err(ApiError::invalid_argument(
            "the frame rate is 15, 30 or 60 fps",
        ));
    }
    if resolution > settings.screen_share_max_resolution || fps > settings.screen_share_max_fps {
        return Err(ApiError::quality_limit(format!(
            "this server shares screens at up to {} and {} fps",
            settings.screen_share_max_resolution.as_str(),
            settings.screen_share_max_fps
        )));
    }
    Ok(resolution)
}

/// A screen or a window; nothing else can be shared.
pub fn check_source(source_kind: i32) -> Result<proto::StreamSourceKind, ApiError> {
    match proto::StreamSourceKind::try_from(source_kind) {
        Ok(kind @ (proto::StreamSourceKind::Screen | proto::StreamSourceKind::Window)) => Ok(kind),
        _ => Err(ApiError::invalid_argument("share a screen or a window")),
    }
}

/// Announces a stream's viewers: the count to everyone who can see its
/// channel, the list to the streamer, and who may receive it to the voice
/// node.
pub fn viewers_changed(state: &AppState, stream: &Stream) {
    state.broadcast(
        Kind::StreamUpdate(proto::StreamUpdate {
            stream: Some(stream.to_proto()),
        }),
        Audience::Channel(stream.channel_id),
    );
    state.broadcast(
        Kind::StreamViewersUpdate(proto::StreamViewersUpdate {
            stream_key: stream.key(),
            viewer_ids: stream.viewers.iter().copied().collect(),
        }),
        Audience::User(stream.user_id),
    );
    tell_node(state, stream.channel_id, stream.user_id, &stream.viewers);
}

fn tell_node(state: &AppState, channel_id: i64, user_id: i64, viewers: &BTreeSet<i64>) {
    state.voice_nodes.send(
        channel_id,
        NodeCommand::StreamViewers {
            channel_id,
            user_id,
            viewers: viewers.iter().copied().collect(),
        },
    );
}

/// Ends `user_id`'s stream, if there is one: everyone who can see its
/// channel learns, and the voice node forwards the screen to nobody. The
/// caller holds the write lock and sees to the voice state's `self_stream`.
pub fn end(state: &AppState, user_id: i64) -> Option<Stream> {
    let removed = state.voice().streams.remove(user_id)?;
    state.broadcast(
        Kind::StreamDelete(proto::StreamDelete {
            stream_key: removed.key(),
            channel_id: removed.channel_id,
        }),
        Audience::Channel(removed.channel_id),
    );
    tell_node(state, removed.channel_id, user_id, &BTreeSet::new());
    Some(removed)
}

/// `user_id` left `channel_id`'s voice: their stream there ends, and they
/// stop watching the others.
pub fn left_channel(state: &AppState, user_id: i64, channel_id: i64) {
    let streaming_there = state
        .voice()
        .streams
        .of(user_id)
        .is_some_and(|stream| stream.channel_id == channel_id);
    if streaming_there {
        end(state, user_id);
    }
    let watched = state.voice().streams.stop_watching_in(channel_id, user_id);
    for stream in watched {
        viewers_changed(state, &stream);
    }
}

/// After permissions or settings change: ends streams whose owner may no
/// longer share there, or whose quality is now above the server's maximum
/// (the voice node stops their tracks too).
pub fn reconcile(state: &AppState) {
    let ended: Vec<i64> = {
        let guild = state.guild();
        let voice = state.voice();
        let settings = &guild.voice_settings;
        voice
            .streams
            .all()
            .filter(|stream| {
                let allowed = require_channel(
                    &guild,
                    stream.user_id,
                    stream.channel_id,
                    Permissions::SCREENSHARE,
                )
                .is_ok();
                let within = stream.resolution <= settings.screen_share_max_resolution
                    && stream.fps <= settings.screen_share_max_fps;
                let in_voice = voice
                    .get(stream.user_id)
                    .is_some_and(|vs| vs.channel_id == stream.channel_id);
                !(allowed && within && in_voice)
            })
            .map(|stream| stream.user_id)
            .collect()
    };
    for user_id in ended {
        end(state, user_id);
        set_streaming(state, user_id, false);
    }
}

/// Sets the voice state's `self_stream` and announces it, if it changed.
pub fn set_streaming(state: &AppState, user_id: i64, streaming: bool) {
    let shown = {
        let mut voice = state.voice();
        let Some(current) = voice.get(user_id).cloned() else {
            return;
        };
        if current.self_stream == streaming {
            return;
        }
        let updated = VoiceState {
            self_stream: streaming,
            ..current
        };
        let shown = voice.to_proto(&updated);
        voice.put(updated);
        shown
    };
    let channel_id = shown.channel_id.unwrap_or_default();
    super::announce(state, shown, &[channel_id]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(resolution: Resolution, fps: u32) -> VoiceSettings {
        VoiceSettings {
            screen_share_max_resolution: resolution,
            screen_share_max_fps: fps,
            ..VoiceSettings::default()
        }
    }

    fn stream(user_id: i64, channel_id: i64) -> Stream {
        Stream {
            channel_id,
            user_id,
            source_kind: proto::StreamSourceKind::Screen,
            resolution: Resolution::P720,
            fps: 30,
            has_audio: true,
            viewers: BTreeSet::new(),
        }
    }

    #[test]
    fn stream_keys_name_a_channel_and_a_user() {
        assert_eq!(key(77, 2), "stream:77:2");
        assert_eq!(parse_key("stream:77:2"), Some((77, 2)));
        assert_eq!(parse_key("stream:77"), None);
        assert_eq!(parse_key("camera:77:2"), None);
        assert_eq!(parse_key("stream:x:2"), None);
    }

    #[test]
    fn quality_above_the_server_maximum_is_refused() {
        let capped = settings(Resolution::P720, 30);
        let p720 = proto::ScreenShareResolution::ScreenShareResolution720p as i32;
        let p1080 = proto::ScreenShareResolution::ScreenShareResolution1080p as i32;
        let p480 = proto::ScreenShareResolution::ScreenShareResolution480p as i32;

        assert_eq!(check_quality(&capped, p720, 30).unwrap(), Resolution::P720);
        assert_eq!(check_quality(&capped, p480, 15).unwrap(), Resolution::P480);
        let over = check_quality(&capped, p1080, 30).unwrap_err();
        assert_eq!(over.code, proto::ErrorCode::QualityLimit);
        let faster = check_quality(&capped, p720, 60).unwrap_err();
        assert_eq!(faster.code, proto::ErrorCode::QualityLimit);
        assert_eq!(
            check_quality(&capped, p720, 24).unwrap_err().code,
            proto::ErrorCode::InvalidArgument
        );
        assert_eq!(
            check_quality(&capped, 0, 30).unwrap_err().code,
            proto::ErrorCode::InvalidArgument
        );
    }

    #[test]
    fn a_key_names_only_a_live_stream_in_its_channel() {
        let mut streams = Streams::default();
        streams.put(stream(2, 77));

        assert!(streams.by_key("stream:77:2").is_some());
        assert!(streams.by_key("stream:78:2").is_none());
        assert!(streams.by_key("stream:77:3").is_none());
    }

    #[test]
    fn viewers_come_and_go() {
        let mut streams = Streams::default();
        streams.put(stream(2, 77));
        streams.put(stream(3, 77));
        streams.put(stream(4, 78));

        assert_eq!(streams.set_viewer(2, 9, true).unwrap().viewers.len(), 1);
        assert!(streams.set_viewer(2, 9, true).is_none(), "already watching");
        streams.set_viewer(3, 9, true);
        streams.set_viewer(4, 9, true);

        let left = streams.stop_watching_in(77, 9);
        let mut left: Vec<i64> = left.iter().map(|stream| stream.user_id).collect();
        left.sort_unstable();
        assert_eq!(left, [2, 3]);
        assert!(streams.of(4).unwrap().viewers.contains(&9));
    }
}
