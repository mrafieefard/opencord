//! Whether a participant may publish a video track as asked (plan §6, §8,
//! §9.2, §14): permissions, the server's limits and the bitrate ceilings.

use opencord_common::permissions::Permissions;
use opencord_common::video::{LAYER_RIDS, VideoKind, layer_ceiling, layer_index};
use opencord_proto::v1 as proto;
use opencord_proto::voice::v1 as voice;

/// Track ids are short visible ASCII.
pub const MAX_TRACK_ID: usize = 64;
/// The largest camera layer (plan §8).
const CAMERA_PIXELS: u32 = 1280 * 720;
const CAMERA_FPS: u32 = 30;
/// "Source" is the screen's own size, up to 4K's pixel count.
const SOURCE_PIXELS: u32 = 3840 * 2160;
const MAX_FPS: u32 = 60;
/// Width and height a layer may have.
const SIDES: std::ops::RangeInclusive<u32> = 2..=8192;
/// The server's screen share frame rate when it gives none.
const DEFAULT_SCREEN_FPS: u32 = 30;

/// Why a track was refused, for `TrackRejected`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub code: proto::ErrorCode,
    pub message: String,
}

impl Refusal {
    fn new(code: proto::ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn invalid(message: impl Into<String>) -> Self {
        Self::new(proto::ErrorCode::InvalidArgument, message)
    }
}

/// A track that may be published, its layers lowest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    pub kind: VideoKind,
    pub layers: Vec<voice::Layer>,
}

pub fn valid_track_id(track_id: &str) -> bool {
    (1..=MAX_TRACK_ID).contains(&track_id.len())
        && track_id.bytes().all(|byte| byte.is_ascii_graphic())
}

/// Whether `publish` is allowed for someone with `permissions` under
/// `limits`.
pub fn check(
    publish: &voice::PublishTrack,
    permissions: u64,
    limits: &voice::Limits,
) -> Result<Checked, Refusal> {
    if !valid_track_id(&publish.track_id) {
        return Err(Refusal::invalid(
            "a track id is 1 to 64 visible ASCII characters",
        ));
    }
    let kind = match voice::TrackKind::try_from(publish.kind) {
        Ok(voice::TrackKind::Camera) => VideoKind::Camera,
        Ok(voice::TrackKind::Screen) => VideoKind::Screen,
        _ => return Err(Refusal::invalid("only camera and screen video tracks")),
    };
    if publish.codec != voice::Codec::H264 as i32 {
        return Err(Refusal::invalid("only H.264 video is supported"));
    }
    let permissions = Permissions::from_bits_truncate(permissions);
    let (needed, name) = match kind {
        VideoKind::Camera => (Permissions::VIDEO, "Video"),
        VideoKind::Screen => (Permissions::SCREENSHARE, "Screen Share"),
    };
    if !permissions.contains(needed) {
        return Err(Refusal::new(
            proto::ErrorCode::Forbidden,
            format!("missing the {name} permission"),
        ));
    }
    if kind == VideoKind::Camera && !limits.camera_allowed {
        return Err(Refusal::new(
            proto::ErrorCode::Forbidden,
            "cameras are turned off on this server",
        ));
    }
    let layers = ordered_layers(&publish.layers)?;
    for layer in &layers {
        within_limits(kind, layer, limits)?;
    }
    Ok(Checked { kind, layers })
}

/// The layers sorted lowest first, each named once, each taller than the
/// one below.
fn ordered_layers(layers: &[voice::Layer]) -> Result<Vec<voice::Layer>, Refusal> {
    if layers.is_empty() || layers.len() > LAYER_RIDS.len() {
        return Err(Refusal::invalid("a track has one to three layers"));
    }
    let mut sorted: Vec<(u8, voice::Layer)> = Vec::with_capacity(layers.len());
    for layer in layers {
        let index = layer_index(&layer.rid)
            .ok_or_else(|| Refusal::invalid("layers are named l, m and h"))?;
        if sorted.iter().any(|(other, _)| *other == index) {
            return Err(Refusal::invalid("each layer is named once"));
        }
        let sane = SIDES.contains(&layer.width)
            && SIDES.contains(&layer.height)
            && (1..=MAX_FPS).contains(&layer.fps)
            && layer.max_bitrate > 0;
        if !sane {
            return Err(Refusal::invalid(
                "a layer has a size, 1 to 60 fps and a bitrate",
            ));
        }
        sorted.push((index, layer.clone()));
    }
    sorted.sort_by_key(|(index, _)| *index);
    let taller = sorted
        .windows(2)
        .all(|pair| pair[1].1.height > pair[0].1.height);
    if !taller {
        return Err(Refusal::invalid("each layer is taller than the one below"));
    }
    Ok(sorted.into_iter().map(|(_, layer)| layer).collect())
}

fn within_limits(
    kind: VideoKind,
    layer: &voice::Layer,
    limits: &voice::Limits,
) -> Result<(), Refusal> {
    let pixels = layer.width.saturating_mul(layer.height);
    let (most_pixels, most_fps, what) = match kind {
        VideoKind::Camera => (
            CAMERA_PIXELS,
            CAMERA_FPS,
            "cameras send at most 720p at 30 fps",
        ),
        VideoKind::Screen => (
            screen_pixels(limits.screen_share_max_resolution),
            match limits.screen_share_max_fps {
                0 => DEFAULT_SCREEN_FPS,
                fps => fps,
            },
            "the screen share is above this server's maximum",
        ),
    };
    if pixels > most_pixels || layer.fps > most_fps {
        return Err(Refusal::new(proto::ErrorCode::QualityLimit, what));
    }
    let ceiling = layer_ceiling(kind, layer.width, layer.height, layer.fps);
    if layer.max_bitrate > ceiling {
        return Err(Refusal::new(
            proto::ErrorCode::QualityLimit,
            format!("the {} layer asks for more than {ceiling} bit/s", layer.rid),
        ));
    }
    Ok(())
}

/// The pixel count a screen share preset allows (plan §9.2).
fn screen_pixels(resolution: i32) -> u32 {
    use proto::ScreenShareResolution as Preset;
    match Preset::try_from(resolution) {
        Ok(Preset::ScreenShareResolution480p) => 854 * 480,
        Ok(Preset::ScreenShareResolution1080p) => 1920 * 1080,
        Ok(Preset::ScreenShareResolution1440p) => 2560 * 1440,
        Ok(Preset::Source) => SOURCE_PIXELS,
        // 720p is the default (plan §5.2).
        _ => 1280 * 720,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(rid: &str, width: u32, height: u32, fps: u32, max_bitrate: u32) -> voice::Layer {
        voice::Layer {
            rid: rid.to_owned(),
            width,
            height,
            fps,
            max_bitrate,
        }
    }

    fn camera() -> voice::PublishTrack {
        voice::PublishTrack {
            track_id: "c0ffee".to_owned(),
            kind: voice::TrackKind::Camera as i32,
            codec: voice::Codec::H264 as i32,
            layers: vec![
                layer("h", 1280, 720, 30, 2_000_000),
                layer("l", 320, 180, 15, 150_000),
                layer("m", 640, 360, 30, 500_000),
            ],
        }
    }

    fn screen(width: u32, height: u32, fps: u32, max_bitrate: u32) -> voice::PublishTrack {
        voice::PublishTrack {
            track_id: "5c4ee7".to_owned(),
            kind: voice::TrackKind::Screen as i32,
            codec: voice::Codec::H264 as i32,
            layers: vec![layer("h", width, height, fps, max_bitrate)],
        }
    }

    fn limits() -> voice::Limits {
        voice::Limits {
            screen_share_max_resolution: proto::ScreenShareResolution::ScreenShareResolution720p
                as i32,
            screen_share_max_fps: 30,
            voice_bitrate: 64_000,
            camera_allowed: true,
            max_camera_participants: 25,
            max_stream_viewers: 50,
        }
    }

    const EVERYTHING: u64 = Permissions::VIDEO.bits() | Permissions::SCREENSHARE.bits();

    fn refused(
        publish: &voice::PublishTrack,
        permissions: u64,
        limits: &voice::Limits,
    ) -> proto::ErrorCode {
        check(publish, permissions, limits).unwrap_err().code
    }

    #[test]
    fn a_camera_with_three_layers_is_accepted_lowest_first() {
        let checked = check(&camera(), EVERYTHING, &limits()).unwrap();

        assert_eq!(checked.kind, VideoKind::Camera);
        let rids: Vec<&str> = checked.layers.iter().map(|l| l.rid.as_str()).collect();
        assert_eq!(rids, ["l", "m", "h"]);
    }

    #[test]
    fn a_screen_share_within_the_server_maximum_is_accepted() {
        let checked = check(&screen(1280, 720, 30, 2_000_000), EVERYTHING, &limits()).unwrap();

        assert_eq!(checked.kind, VideoKind::Screen);
        // An ultrawide window under 720p's pixel count is fine too.
        assert!(check(&screen(1500, 600, 30, 2_000_000), EVERYTHING, &limits()).is_ok());
    }

    #[test]
    fn permissions_are_needed() {
        assert_eq!(
            refused(&camera(), Permissions::SCREENSHARE.bits(), &limits()),
            proto::ErrorCode::Forbidden
        );
        assert_eq!(
            refused(
                &screen(1280, 720, 30, 1_000_000),
                Permissions::VIDEO.bits(),
                &limits()
            ),
            proto::ErrorCode::Forbidden
        );
    }

    #[test]
    fn a_server_without_cameras_refuses_them() {
        let limits = voice::Limits {
            camera_allowed: false,
            ..limits()
        };

        assert_eq!(
            refused(&camera(), EVERYTHING, &limits),
            proto::ErrorCode::Forbidden
        );
    }

    #[test]
    fn video_above_the_limits_is_a_quality_limit() {
        let mut big_camera = camera();
        big_camera.layers[0] = layer("h", 1920, 1080, 30, 2_000_000);
        let mut fast_camera = camera();
        fast_camera.layers[0] = layer("h", 1280, 720, 60, 2_000_000);
        let mut greedy_camera = camera();
        greedy_camera.layers[0] = layer("h", 1280, 720, 30, 3_000_000);

        for publish in [
            big_camera,
            fast_camera,
            greedy_camera,
            screen(1920, 1080, 30, 2_000_000),
            screen(1280, 720, 60, 2_000_000),
            screen(1280, 720, 30, 2_500_000),
        ] {
            assert_eq!(
                refused(&publish, EVERYTHING, &limits()),
                proto::ErrorCode::QualityLimit,
                "{publish:?}"
            );
        }
    }

    #[test]
    fn source_means_up_to_4k() {
        let limits = voice::Limits {
            screen_share_max_resolution: proto::ScreenShareResolution::Source as i32,
            screen_share_max_fps: 60,
            ..limits()
        };

        assert!(check(&screen(3840, 2160, 60, 16_000_000), EVERYTHING, &limits).is_ok());
        assert_eq!(
            refused(&screen(5120, 2880, 60, 16_000_000), EVERYTHING, &limits),
            proto::ErrorCode::QualityLimit
        );
    }

    #[test]
    fn malformed_tracks_are_refused() {
        let mut no_layers = camera();
        no_layers.layers.clear();
        let mut four_layers = camera();
        four_layers.layers.push(layer("l", 160, 90, 15, 100_000));
        let mut unknown_rid = camera();
        unknown_rid.layers[0].rid = "x".to_owned();
        let mut shrinking = camera();
        shrinking.layers[2].height = 100;
        let mut zero_bitrate = camera();
        zero_bitrate.layers[1].max_bitrate = 0;
        let mut no_frames = camera();
        no_frames.layers[1].fps = 0;
        let mut av1 = camera();
        av1.codec = voice::Codec::Av1 as i32;
        let mut long_id = camera();
        long_id.track_id = "x".repeat(MAX_TRACK_ID + 1);
        let mut empty_id = camera();
        empty_id.track_id.clear();
        let mut spaced_id = camera();
        spaced_id.track_id = "my camera".to_owned();
        let mut screen_audio = camera();
        screen_audio.kind = voice::TrackKind::ScreenAudio as i32;

        for publish in [
            no_layers,
            four_layers,
            unknown_rid,
            shrinking,
            zero_bitrate,
            no_frames,
            av1,
            long_id,
            empty_id,
            spaced_id,
            screen_audio,
        ] {
            assert_eq!(
                refused(&publish, EVERYTHING, &limits()),
                proto::ErrorCode::InvalidArgument,
                "{publish:?}"
            );
        }
    }
}
