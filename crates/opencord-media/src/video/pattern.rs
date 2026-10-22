//! A synthetic three-layer video source (plan §15) for tests, the voicebot
//! and self-hosters checking their setup. Frames are shaped like H.264
//! access units (SPS, PPS and an IDR slice on keyframes, one slice
//! otherwise), sized to each layer's bitrate, and carry their layer, number
//! and a checksum, so a receiver can tell every frame arrived intact. Nothing
//! decodes them: the voice node never reads payloads, and the transport only
//! cuts and joins NAL units.

use std::time::{Duration, Instant};

use crate::transport::{Layer, TrackKind, TrackRequest, VideoFrame};
use crate::video::picture::{Picture, PixelFormat};

const MAGIC: &[u8; 4] = b"OCTP";
/// After a slice's NAL header: magic, layer, flags and number.
const HEADER: usize = 4 + 1 + 1 + 8;
/// FNV-1a of everything after the NAL header, at the end.
const CHECKSUM: usize = 4;
const SPS: [u8; 9] = [0x67, 0x42, 0xe0, 0x1f, 0xda, 0x01, 0x40, 0x16, 0xe8];
const PPS: [u8; 4] = [0x68, 0xce, 0x3c, 0x80];

/// A moving scene for real encoders (NV12): a luma ramp that drifts, a
/// bright square crossing the picture, and colour bands. Like a camera, it
/// changes a little from one frame to the next.
pub fn scene(width: u32, height: u32, number: u64, captured: Instant) -> Picture {
    let (w, h) = (width as usize, height as usize);
    let mut picture = Picture::black(PixelFormat::Nv12, width, height, captured);
    let side = (h / 4).max(2);
    let travel = (w - side.min(w)).max(1);
    let square_x = (number as usize * 6) % travel;
    let square_y = h / 2 - side / 2;
    let drift = number as usize * 2;
    let (luma, chroma) = picture.data.split_at_mut(w * h);
    for y in 0..h {
        for x in 0..w {
            let inside = (square_x..square_x + side).contains(&x)
                && (square_y..square_y + side).contains(&y);
            luma[y * w + x] = if inside {
                235
            } else {
                16 + ((x + y / 2 + drift) % 200) as u8
            };
        }
    }
    for y in 0..h / 2 {
        for x in 0..w / 2 {
            let band = (x * 8 / (w / 2).max(1)) as u8;
            chroma[y * w + 2 * x] = 96 + band * 8;
            chroma[y * w + 2 * x + 1] = 160 - band * 8;
        }
    }
    picture
}

/// What a received picture says about itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Checked {
    pub layer: u8,
    pub number: u64,
    pub keyframe: bool,
}

/// The NAL units of picture `number` of `layer`, about `size` bytes.
pub fn picture(layer: u8, number: u64, keyframe: bool, size: usize) -> Vec<Vec<u8>> {
    let mut units = Vec::new();
    let mut remaining = size;
    if keyframe {
        units.push(SPS.to_vec());
        units.push(PPS.to_vec());
        remaining = remaining.saturating_sub(SPS.len() + PPS.len());
    }
    let body = remaining.saturating_sub(1).max(HEADER + CHECKSUM);
    let filler = body - HEADER - CHECKSUM;
    let mut slice = Vec::with_capacity(1 + body);
    slice.push(if keyframe { 0x65 } else { 0x41 });
    slice.extend_from_slice(MAGIC);
    slice.push(layer);
    slice.push(u8::from(keyframe));
    slice.extend_from_slice(&number.to_be_bytes());
    slice.extend((0..filler).map(|index| filler_byte(number, index)));
    let sum = fnv1a(&slice[1..]);
    slice.extend_from_slice(&sum.to_be_bytes());
    units.push(slice);
    units
}

/// The picture's layer, number and kind, if it arrived intact.
pub fn check(nal_units: &[Vec<u8>]) -> Option<Checked> {
    let slice = nal_units.last()?;
    if slice.len() < 1 + HEADER + CHECKSUM || &slice[1..5] != MAGIC {
        return None;
    }
    let (content, sum) = slice[1..].split_at(slice.len() - 1 - CHECKSUM);
    if fnv1a(content).to_be_bytes() != sum {
        return None;
    }
    let number = u64::from_be_bytes(content[6..14].try_into().ok()?);
    Some(Checked {
        layer: content[4],
        number,
        keyframe: content[5] == 1,
    })
}

/// Never zero, so the bytes never look like a start code.
fn filler_byte(number: u64, index: usize) -> u8 {
    let mixed = (number as usize).wrapping_mul(131).wrapping_add(index * 7);
    (mixed % 251 + 1) as u8
}

fn fnv1a(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c_9dc5, |hash, byte| {
        (hash ^ u32::from(*byte)).wrapping_mul(0x0100_0193)
    })
}

/// Pictures for every layer the node wants, each at its layer's frame rate
/// and bitrate, with keyframes on request.
pub struct TestPattern {
    track_id: String,
    layers: Vec<PatternLayer>,
    /// The top layer's share of its frame rate and of its pixels.
    fps_scale: f32,
    size_scale: f32,
}

struct PatternLayer {
    layer: Layer,
    active: bool,
    next_at: Instant,
    number: u64,
    keyframe_due: bool,
    bitrate: u32,
}

impl TestPattern {
    /// Every layer starts on, each with a keyframe.
    pub fn new(track_id: &str, layers: Vec<Layer>, now: Instant) -> Self {
        Self {
            track_id: track_id.to_owned(),
            layers: layers
                .into_iter()
                .map(|layer| PatternLayer {
                    bitrate: layer.max_bitrate,
                    layer,
                    active: true,
                    next_at: now,
                    number: 0,
                    keyframe_due: true,
                })
                .collect(),
            fps_scale: 1.0,
            size_scale: 1.0,
        }
    }

    pub fn request(&self, kind: TrackKind) -> TrackRequest {
        TrackRequest {
            track_id: self.track_id.clone(),
            kind,
            layers: self
                .layers
                .iter()
                .map(|layer| layer.layer.clone())
                .collect(),
        }
    }

    /// Only the layers named are sent; one that comes back starts with a
    /// keyframe.
    pub fn wants(&mut self, rids: &[String]) {
        for layer in &mut self.layers {
            let wanted = rids.contains(&layer.layer.rid);
            if wanted && !layer.active {
                layer.keyframe_due = true;
            }
            layer.active = wanted;
        }
    }

    /// Does what the transport's `VoiceEvent::Encode` says: only these
    /// layers, the top one slowed down and shrunk by these shares.
    pub fn encode(&mut self, rids: &[String], fps_scale: f32, size_scale: f32) {
        self.wants(rids);
        self.fps_scale = fps_scale.clamp(0.0, 1.0);
        self.size_scale = size_scale.clamp(0.0, 1.0);
    }

    /// The next picture of `layer` (0 the lowest) is a keyframe.
    pub fn keyframe(&mut self, layer: u8) {
        if let Some(layer) = self.layers.get_mut(usize::from(layer)) {
            layer.keyframe_due = true;
        }
    }

    /// Sends `layer` at this many bits per second instead of its maximum.
    pub fn set_bitrate(&mut self, layer: u8, bits_per_second: u32) {
        if let Some(layer) = self.layers.get_mut(usize::from(layer)) {
            layer.bitrate = bits_per_second;
        }
    }

    /// The layers being sent, by rid.
    pub fn active(&self) -> Vec<String> {
        self.layers
            .iter()
            .filter(|layer| layer.active)
            .map(|layer| layer.layer.rid.clone())
            .collect()
    }

    /// The pictures due by `now`; a layer that fell behind skips ahead.
    pub fn frames(&mut self, now: Instant) -> Vec<VideoFrame> {
        let mut frames = Vec::new();
        let top = self.layers.len().saturating_sub(1);
        for (index, layer) in self.layers.iter_mut().enumerate() {
            if !layer.active || layer.next_at > now {
                continue;
            }
            let (fps_scale, size_scale) = if index == top {
                (self.fps_scale, self.size_scale)
            } else {
                (1.0, 1.0)
            };
            let fps = ((layer.layer.fps as f32 * fps_scale).round() as u32).max(1);
            let bitrate = f64::from(layer.bitrate) * f64::from(fps_scale) * f64::from(size_scale);
            let side = |pixels: u32| {
                let scaled = (f64::from(pixels) * f64::from(size_scale).sqrt()).round();
                u16::try_from(scaled as u32).unwrap_or(u16::MAX)
            };
            let interval = Duration::from_secs(1) / fps;
            layer.next_at = (layer.next_at + interval).max(now);
            let keyframe = std::mem::take(&mut layer.keyframe_due);
            let size = (bitrate / f64::from(fps) / 8.0) as usize;
            frames.push(VideoFrame {
                track_id: self.track_id.clone(),
                layer: index as u8,
                keyframe,
                width: side(layer.layer.width),
                height: side(layer.layer.height),
                captured: now,
                nal_units: picture(index as u8, layer.number, keyframe, size),
            });
            layer.number += 1;
        }
        frames
    }

    /// When the next picture is due.
    pub fn next_due(&self) -> Option<Instant> {
        self.layers
            .iter()
            .filter(|layer| layer.active)
            .map(|layer| layer.next_at)
            .min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::Layer;

    fn layers() -> Vec<Layer> {
        let layer = |rid: &str, width, height, fps, max_bitrate| Layer {
            rid: rid.to_owned(),
            width,
            height,
            fps,
            max_bitrate,
        };
        vec![
            layer("l", 320, 180, 15, 150_000),
            layer("m", 640, 360, 30, 500_000),
            layer("h", 1280, 720, 30, 1_500_000),
        ]
    }

    #[test]
    fn the_scene_moves_and_stays_in_video_range() {
        let start = Instant::now();
        let first = scene(320, 180, 0, start);
        let second = scene(320, 180, 1, start);

        assert_eq!(first.format, PixelFormat::Nv12);
        assert_eq!(first.data.len(), Picture::bytes(320, 180));
        assert_ne!(first.data, second.data);
        assert!(first.y().iter().all(|&y| (16..=235).contains(&y)));
    }

    #[test]
    fn a_keyframe_has_parameter_sets_and_checks_out() {
        let units = picture(2, 5, true, 3000);

        assert_eq!(units.len(), 3);
        assert_eq!(units[0][0] & 0x1f, 7);
        assert_eq!(units[1][0] & 0x1f, 8);
        assert_eq!(units[2][0] & 0x1f, 5);
        let total: usize = units.iter().map(Vec::len).sum();
        assert!((2900..=3100).contains(&total), "{total} bytes");
        assert_eq!(
            check(&units),
            Some(Checked {
                layer: 2,
                number: 5,
                keyframe: true
            })
        );
    }

    #[test]
    fn a_delta_frame_is_one_slice() {
        let units = picture(0, 9, false, 500);

        assert_eq!(units.len(), 1);
        assert_eq!(units[0][0] & 0x1f, 1);
        assert_eq!(check(&units).map(|checked| checked.number), Some(9));
    }

    #[test]
    fn a_damaged_frame_does_not_check_out() {
        let mut units = picture(1, 3, false, 800);
        units[0][100] ^= 0x01;

        assert_eq!(check(&units), None);
        assert_eq!(check(&[]), None);
        assert_eq!(check(&[vec![0x41, 1, 2]]), None);
    }

    #[test]
    fn layers_send_at_their_frame_rates_starting_with_keyframes() {
        let start = Instant::now();
        let mut pattern = TestPattern::new("cam", layers(), start);

        let mut frames = Vec::new();
        let mut now = start;
        while now < start + Duration::from_millis(1000) {
            frames.extend(pattern.frames(now));
            now += Duration::from_millis(5);
        }

        let count = |layer| frames.iter().filter(|f| f.layer == layer).count();
        assert!((14..=16).contains(&count(0)), "{}", count(0));
        assert!((29..=31).contains(&count(2)), "{}", count(2));
        for layer in 0..3 {
            let first = frames.iter().find(|f| f.layer == layer).unwrap();
            assert!(first.keyframe);
            let keyframes = frames
                .iter()
                .filter(|f| f.layer == layer && f.keyframe)
                .count();
            assert_eq!(keyframes, 1, "layer {layer}");
        }
        // A frame of the 720p layer at 1.5 Mbit/s and 30 fps: 6250 bytes.
        let big = frames.iter().find(|f| f.layer == 2 && !f.keyframe).unwrap();
        let bytes: usize = big.nal_units.iter().map(Vec::len).sum();
        assert!((6000..=6500).contains(&bytes), "{bytes}");
        assert_eq!((big.width, big.height), (1280, 720));
    }

    #[test]
    fn a_keyframe_request_makes_the_next_frame_a_keyframe() {
        let start = Instant::now();
        let mut pattern = TestPattern::new("cam", layers(), start);
        pattern.frames(start);

        pattern.keyframe(1);
        let next = pattern.frames(start + Duration::from_millis(40));

        let mid = next.iter().find(|f| f.layer == 1).unwrap();
        assert!(mid.keyframe);
        assert!(next.iter().filter(|f| f.layer != 1).all(|f| !f.keyframe));
    }

    #[test]
    fn only_the_wanted_layers_are_sent_and_a_returning_layer_starts_with_a_keyframe() {
        let start = Instant::now();
        let mut pattern = TestPattern::new("cam", layers(), start);
        pattern.frames(start);

        pattern.wants(&["l".to_owned()]);
        let wanted = pattern.frames(start + Duration::from_millis(100));
        pattern.wants(&["l".to_owned(), "h".to_owned()]);
        let back = pattern.frames(start + Duration::from_millis(200));

        assert!(wanted.iter().all(|f| f.layer == 0));
        let high = back.iter().find(|f| f.layer == 2).unwrap();
        assert!(high.keyframe);
        assert!(back.iter().all(|f| f.layer != 1));
    }

    #[test]
    fn the_top_layer_slows_down_and_shrinks_as_told() {
        let start = Instant::now();
        let screen = vec![
            Layer {
                rid: "l".to_owned(),
                width: 640,
                height: 360,
                fps: 15,
                max_bitrate: 300_000,
            },
            Layer {
                rid: "h".to_owned(),
                width: 1920,
                height: 1080,
                fps: 30,
                max_bitrate: 2_000_000,
            },
        ];
        let mut pattern = TestPattern::new("screen", screen, start);
        pattern.frames(start);

        pattern.encode(&["h".to_owned()], 0.5, 0.25);
        let mut frames = Vec::new();
        let mut now = start + Duration::from_millis(5);
        while now < start + Duration::from_millis(1005) {
            frames.extend(pattern.frames(now));
            now += Duration::from_millis(5);
        }

        assert!(frames.iter().all(|frame| frame.layer == 1));
        assert!((14..=16).contains(&frames.len()), "{}", frames.len());
        let frame = &frames[1];
        assert_eq!((frame.width, frame.height), (960, 540));
        // 2 Mbit/s, half the frames, a quarter of the pixels: 250 kbit/s
        // at 15 fps.
        let bytes: usize = frame.nal_units.iter().map(Vec::len).sum();
        assert!((2000..=2200).contains(&bytes), "{bytes}");
    }

    #[test]
    fn a_layer_can_send_more_than_its_share() {
        let start = Instant::now();
        let mut pattern = TestPattern::new("cam", layers(), start);
        pattern.frames(start);

        pattern.set_bitrate(0, 600_000);
        let frames = pattern.frames(start + Duration::from_millis(70));

        let low = frames.iter().find(|f| f.layer == 0).unwrap();
        let bytes: usize = low.nal_units.iter().map(Vec::len).sum();
        assert!((4800..=5200).contains(&bytes), "{bytes}");
    }
}
