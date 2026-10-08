//! Video signalling on the voice gateway (plan §4.3): publishing and
//! unpublishing tracks, what receivers want, and what the SFU reports back.

use std::time::Instant;

use opencord_common::permissions::Permissions;
use opencord_proto::v1 as proto;
use opencord_proto::voice::v1 as voice;
use voice::envelope::Payload;

use super::Runtime;
use crate::node::tracks::{self, Refusal};
use crate::sfu::{LayerSetup, PeerId, TrackSetup, VideoKind, Want};

impl Runtime {
    pub(super) fn publish(&mut self, now: Instant, session_id: &str, publish: voice::PublishTrack) {
        let Some(session) = self.sessions.get(session_id) else {
            return;
        };
        let (peer, user_id, channel_id, permissions) = (
            session.peer,
            session.user_id,
            session.channel_id,
            session.permissions,
        );
        let track_id = publish.track_id.clone();
        // A client that resumed may ask again; it gets the same answer.
        let own = self.sfu.tracks(peer);
        if let Some(existing) = own.iter().find(|track| track.track_id == track_id) {
            self.send_published(now, session_id, existing);
            return;
        }
        let limits = self.channel_limits(channel_id);
        let checked = match tracks::check(&publish, permissions, &limits) {
            Ok(checked) => checked,
            Err(refusal) => return self.refuse(now, session_id, &track_id, refusal),
        };
        if own.iter().any(|track| track.kind == checked.kind) {
            let refusal = invalid("a participant publishes one track of each kind");
            return self.refuse(now, session_id, &track_id, refusal);
        }
        let others = self.others_in_channel(channel_id, session_id);
        let in_use = others.iter().any(|(other, _)| {
            self.sfu
                .tracks(*other)
                .iter()
                .any(|track| track.track_id == track_id)
        });
        if in_use {
            return self.refuse(
                now,
                session_id,
                &track_id,
                invalid("the track id is in use"),
            );
        }
        if checked.kind == VideoKind::Camera && limits.max_camera_participants > 0 {
            let cameras = others
                .iter()
                .filter(|(other, _)| {
                    self.sfu
                        .tracks(*other)
                        .iter()
                        .any(|track| track.kind == VideoKind::Camera)
                })
                .count();
            if cameras >= limits.max_camera_participants as usize {
                let refusal = Refusal {
                    code: proto::ErrorCode::CameraLimit,
                    message: "Camera limit reached in this channel".to_owned(),
                };
                return self.refuse(now, session_id, &track_id, refusal);
            }
        }
        let mut taken = Vec::new();
        let layers: Vec<LayerSetup> = checked
            .layers
            .iter()
            .map(|layer| LayerSetup {
                rid: layer.rid.clone(),
                ssrc: self.free_ssrc(channel_id, &mut taken),
                rtx_ssrc: self.free_ssrc(channel_id, &mut taken),
                width: layer.width,
                height: layer.height,
                fps: layer.fps,
                max_bitrate: layer.max_bitrate,
            })
            .collect();
        let setup = TrackSetup {
            track_id: track_id.clone(),
            kind: checked.kind,
            layers,
            ssrc: self.free_ssrc(channel_id, &mut taken),
            rtx_ssrc: self.free_ssrc(channel_id, &mut taken),
        };
        if let Err(error) = self.sfu.publish_track(now, peer, setup.clone()) {
            tracing::debug!(%error, "a track could not be published");
            let refusal = Refusal {
                code: proto::ErrorCode::Internal,
                message: "the track could not be published".to_owned(),
            };
            return self.refuse(now, session_id, &track_id, refusal);
        }
        self.send_published(now, session_id, &setup);
        let update = voice::TrackUpdate {
            user_id,
            track: Some(track_message(&setup, voice::TrackState::Active)),
            available_layers: Vec::new(),
        };
        for (_, other) in others {
            if let Some(session) = self.sessions.get_mut(&other) {
                session.send_sequenced(Payload::TrackUpdate(update.clone()), now);
            }
        }
    }

    pub(super) fn unpublish(&mut self, now: Instant, session_id: &str, track_id: &str) {
        let Some(peer) = self.sessions.get(session_id).map(|session| session.peer) else {
            return;
        };
        self.track_ended(now, peer, track_id, None);
    }

    /// Ends a track and tells everyone; `refusal` also tells its sender
    /// why, when it was not the sender's choice.
    pub(super) fn track_ended(
        &mut self,
        now: Instant,
        peer: PeerId,
        track_id: &str,
        refusal: Option<Refusal>,
    ) {
        let track = self
            .sfu
            .tracks(peer)
            .into_iter()
            .find(|track| track.track_id == track_id);
        self.sfu.unpublish_track(peer, track_id);
        // The SFU ends a track itself before saying so; otherwise there
        // has to have been a track.
        if track.is_none() && refusal.is_none() {
            return;
        }
        let Some(session) = self.session_by_peer(peer) else {
            return;
        };
        if let Some(refusal) = refusal {
            let rejected = voice::TrackRejected {
                track_id: track_id.to_owned(),
                reason: refusal.code as i32,
                message: refusal.message,
            };
            session.send_sequenced(Payload::TrackRejected(rejected), now);
        }
        let (user_id, channel_id, session_id) =
            (session.user_id, session.channel_id, session.id.clone());
        let removed = match &track {
            Some(track) => track_message(track, voice::TrackState::Removed),
            None => voice::Track {
                track_id: track_id.to_owned(),
                state: voice::TrackState::Removed as i32,
                ..voice::Track::default()
            },
        };
        let update = voice::TrackUpdate {
            user_id,
            track: Some(removed),
            available_layers: Vec::new(),
        };
        for (_, other) in self.others_in_channel(channel_id, &session_id) {
            if let Some(session) = self.sessions.get_mut(&other) {
                session.send_sequenced(Payload::TrackUpdate(update.clone()), now);
            }
        }
    }

    /// What a participant wants to watch; paused tracks are not wanted.
    pub(super) fn sink_wants(
        &mut self,
        now: Instant,
        session_id: &str,
        wants: &voice::MediaSinkWants,
    ) {
        let Some(peer) = self.sessions.get(session_id).map(|session| session.peer) else {
            return;
        };
        let wants: Vec<Want> = wants
            .wants
            .iter()
            .filter(|want| !want.paused)
            .map(|want| Want {
                track_id: want.track_id.clone(),
                max_height: want.max_height,
            })
            .collect();
        self.sfu.set_wants(now, peer, &wants);
    }

    /// Tells the channel which layers a sender now produces.
    pub(super) fn layers_available(&mut self, peer: PeerId, track_id: &str, rids: Vec<String>) {
        let Some(track) = self
            .sfu
            .tracks(peer)
            .into_iter()
            .find(|track| track.track_id == track_id)
        else {
            return;
        };
        let Some(session) = self.session_by_peer(peer) else {
            return;
        };
        let (user_id, channel_id, session_id) =
            (session.user_id, session.channel_id, session.id.clone());
        let update = voice::TrackUpdate {
            user_id,
            track: Some(track_message(&track, voice::TrackState::Active)),
            available_layers: rids,
        };
        let now = Instant::now();
        for (_, other) in self.others_in_channel(channel_id, &session_id) {
            if let Some(session) = self.sessions.get_mut(&other) {
                session.send_sequenced(Payload::TrackUpdate(update.clone()), now);
            }
        }
    }

    /// Stops tracks a permission or limit change no longer allows.
    pub(super) fn recheck_tracks(&mut self, now: Instant, session_id: &str) {
        let Some(session) = self.sessions.get(session_id) else {
            return;
        };
        let (peer, channel_id) = (session.peer, session.channel_id);
        let permissions = Permissions::from_bits_truncate(session.permissions);
        let limits = self.channel_limits(channel_id);
        let stopped: Vec<(String, &str)> = self
            .sfu
            .tracks(peer)
            .into_iter()
            .filter_map(|track| {
                let reason = match track.kind {
                    VideoKind::Camera if !permissions.contains(Permissions::VIDEO) => {
                        "the Video permission was taken away"
                    }
                    VideoKind::Camera if !limits.camera_allowed => {
                        "cameras were turned off on this server"
                    }
                    VideoKind::Screen if !permissions.contains(Permissions::SCREENSHARE) => {
                        "the Screen Share permission was taken away"
                    }
                    _ => return None,
                };
                Some((track.track_id, reason))
            })
            .collect();
        for (track_id, reason) in stopped {
            let refusal = Refusal {
                code: proto::ErrorCode::Forbidden,
                message: reason.to_owned(),
            };
            self.track_ended(now, peer, &track_id, Some(refusal));
        }
    }

    /// The limits that apply in a channel.
    pub(super) fn channel_limits(&self, channel_id: i64) -> voice::Limits {
        self.limits
            .get(&channel_id)
            .cloned()
            .unwrap_or_else(default_limits)
    }

    /// Everyone else in the channel: (peer, session id).
    fn others_in_channel(&self, channel_id: i64, session_id: &str) -> Vec<(PeerId, String)> {
        self.sessions
            .values()
            .filter(|session| session.channel_id == channel_id && session.id != session_id)
            .map(|session| (session.peer, session.id.clone()))
            .collect()
    }

    fn send_published(&mut self, now: Instant, session_id: &str, setup: &TrackSetup) {
        let Some(session) = self.sessions.get_mut(session_id) else {
            return;
        };
        let published = voice::TrackPublished {
            track_id: setup.track_id.clone(),
            layers: setup
                .layers
                .iter()
                .map(|layer| voice::LayerSsrc {
                    rid: layer.rid.clone(),
                    ssrc: layer.ssrc,
                    rtx_ssrc: layer.rtx_ssrc,
                })
                .collect(),
        };
        session.send(Payload::TrackPublished(published), now);
    }

    fn refuse(&mut self, now: Instant, session_id: &str, track_id: &str, refusal: Refusal) {
        let Some(session) = self.sessions.get_mut(session_id) else {
            return;
        };
        let rejected = voice::TrackRejected {
            track_id: track_id.to_owned(),
            reason: refusal.code as i32,
            message: refusal.message,
        };
        session.send(Payload::TrackRejected(rejected), now);
    }
}

fn invalid(message: &str) -> Refusal {
    Refusal {
        code: proto::ErrorCode::InvalidArgument,
        message: message.to_owned(),
    }
}

/// A track as receivers learn about it.
pub(super) fn track_message(setup: &TrackSetup, state: voice::TrackState) -> voice::Track {
    voice::Track {
        track_id: setup.track_id.clone(),
        kind: match setup.kind {
            VideoKind::Camera => voice::TrackKind::Camera,
            VideoKind::Screen => voice::TrackKind::Screen,
        } as i32,
        codec: voice::Codec::H264 as i32,
        layers: setup
            .layers
            .iter()
            .map(|layer| voice::Layer {
                rid: layer.rid.clone(),
                width: layer.width,
                height: layer.height,
                fps: layer.fps,
                max_bitrate: layer.max_bitrate,
            })
            .collect(),
        state: state as i32,
        ssrc: setup.ssrc,
        rtx_ssrc: setup.rtx_ssrc,
    }
}

/// When the main server gave none: plan §5.2's defaults.
fn default_limits() -> voice::Limits {
    voice::Limits {
        screen_share_max_resolution: proto::ScreenShareResolution::ScreenShareResolution720p as i32,
        screen_share_max_fps: 30,
        voice_bitrate: 64_000,
        camera_allowed: true,
        max_camera_participants: 25,
        max_stream_viewers: 50,
    }
}
