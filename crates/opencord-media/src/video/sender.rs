//! A camera's send side (plan §7.8, §8): its frames become NV12 pictures,
//! each layer's size, and H.264 from each layer's encoder, as the
//! transport asks (which layers, at what bitrate, keyframes when someone
//! lost a picture). One thread per track; the newest frame wins when the
//! encoders fall behind.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::transport::{Layer, VideoFrame};
use crate::video::camera::RawFrame;
use crate::video::codec::encoder::{Backend, Encoder, EncoderConfig};
use crate::video::codec::raw::RawDecoder;
use crate::video::codec::scale::Scaler;
use crate::video::picture::{Picture, PixelFormat};

/// The plan's camera layers (§8): name, height, frame rate and bitrate at
/// 16:9.
const LAYERS: [(&str, u32, u32, u32); 3] = [
    ("l", 180, 15, 150_000),
    ("m", 360, 30, 500_000),
    ("h", 720, 30, 1_500_000),
];
/// How long the thread waits for a frame before it looks at its commands.
const WAIT: Duration = Duration::from_millis(100);
/// A bitrate this much away from an encoder's reopens it.
const BITRATE_CHANGE: f64 = 0.2;

/// The layers for a camera of this size: 180, 360 and 720 lines (no more
/// than the camera has), its shape kept, bitrates by pixel count.
pub fn camera_layers(width: u32, height: u32) -> Vec<Layer> {
    let mut layers: Vec<Layer> = Vec::new();
    if width == 0 || height == 0 {
        return layers;
    }
    for (rid, target, fps, bitrate) in LAYERS {
        let layer_height = target.min(height) & !1;
        let layer_width =
            (u64::from(width) * u64::from(layer_height) / u64::from(height)) as u32 & !1;
        let taller = layers.last().is_none_or(|last| last.height < layer_height);
        if layer_height == 0 || layer_width == 0 || !taller {
            continue;
        }
        let reference = u64::from(target * 16 / 9) * u64::from(target);
        let pixels = u64::from(layer_width) * u64::from(layer_height);
        let max_bitrate = (u64::from(bitrate) * pixels / reference).min(u64::from(bitrate)) as u32;
        layers.push(Layer {
            rid: rid.to_owned(),
            width: layer_width,
            height: layer_height,
            fps,
            max_bitrate,
        });
    }
    layers
}

enum Command {
    Encode {
        rids: Vec<String>,
        bitrates: Vec<u32>,
    },
    Keyframe(u8),
}

/// A track's encoding thread; stops when dropped or when the camera's
/// frames end.
pub struct CameraSender {
    commands: mpsc::Sender<Command>,
    thread: Option<JoinHandle<()>>,
}

impl CameraSender {
    /// Encodes the frames from `frames` into `layers`, handing each picture
    /// to `send` and each camera frame to `preview` first.
    pub fn start(
        track_id: String,
        layers: Vec<Layer>,
        frames: Receiver<RawFrame>,
        send: impl FnMut(VideoFrame) + Send + 'static,
        preview: impl FnMut(&Picture) + Send + 'static,
    ) -> Self {
        let (commands, received) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("opencord-video-send".to_owned())
            .spawn(move || {
                let mut sending = Sending::new(track_id, layers, send, preview);
                sending.run(&frames, &received);
            })
            .ok();
        Self { commands, thread }
    }

    /// What to encode: the layers named, at these bitrates (the transport's
    /// `VoiceEvent::Encode`).
    pub fn encode(&self, rids: &[String], bitrates: &[u32]) {
        let _ = self.commands.send(Command::Encode {
            rids: rids.to_vec(),
            bitrates: bitrates.to_vec(),
        });
    }

    /// The next picture of `layer` is a keyframe.
    pub fn keyframe(&self, layer: u8) {
        let _ = self.commands.send(Command::Keyframe(layer));
    }

    /// Whether the thread has ended: the camera's frames stopped (it was
    /// unplugged, or failed).
    pub fn finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }
}

impl Drop for CameraSender {
    fn drop(&mut self) {
        // Closing the command channel ends the thread at its next look.
        let (closed, _) = mpsc::channel();
        drop(std::mem::replace(&mut self.commands, closed));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct LayerState {
    layer: Layer,
    active: bool,
    bitrate: u32,
    encoder: Option<Encoder>,
    keyframe_due: bool,
    next_at: Option<Instant>,
}

struct Sending<S, P> {
    track_id: String,
    layers: Vec<LayerState>,
    decoder: RawDecoder,
    scaler: Scaler,
    send: S,
    preview: P,
}

impl<S: FnMut(VideoFrame), P: FnMut(&Picture)> Sending<S, P> {
    fn new(track_id: String, layers: Vec<Layer>, send: S, preview: P) -> Self {
        Self {
            track_id,
            layers: layers
                .into_iter()
                .map(|layer| LayerState {
                    bitrate: layer.max_bitrate,
                    layer,
                    active: true,
                    encoder: None,
                    keyframe_due: true,
                    next_at: None,
                })
                .collect(),
            decoder: RawDecoder::new(),
            scaler: Scaler::new(),
            send,
            preview,
        }
    }

    fn run(&mut self, frames: &Receiver<RawFrame>, commands: &Receiver<Command>) {
        loop {
            let frame = match frames.recv_timeout(WAIT) {
                Ok(mut frame) => {
                    // The newest frame wins when encoding falls behind.
                    while let Ok(newer) = frames.try_recv() {
                        frame = newer;
                    }
                    Some(frame)
                }
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            };
            // Commands sent before the frame arrived apply to it.
            loop {
                match commands.try_recv() {
                    Ok(command) => self.apply(command),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => return,
                }
            }
            if let Some(frame) = frame {
                self.frame(&frame);
            }
        }
    }

    fn apply(&mut self, command: Command) {
        match command {
            Command::Encode { rids, bitrates } => {
                for state in &mut self.layers {
                    let position = rids.iter().position(|rid| *rid == state.layer.rid);
                    let active = position.is_some();
                    if active && !state.active {
                        state.keyframe_due = true;
                        state.next_at = None;
                    }
                    state.active = active;
                    let bitrate = position
                        .and_then(|position| bitrates.get(position).copied())
                        .unwrap_or(state.layer.max_bitrate)
                        .clamp(1, state.layer.max_bitrate.max(1));
                    let change = (f64::from(bitrate) - f64::from(state.bitrate)).abs()
                        / f64::from(state.bitrate.max(1));
                    if change > BITRATE_CHANGE {
                        // A new encoder starts with a keyframe.
                        state.encoder = None;
                        state.keyframe_due = true;
                    }
                    state.bitrate = bitrate;
                }
            }
            Command::Keyframe(layer) => {
                if let Some(state) = self.layers.get_mut(usize::from(layer)) {
                    state.keyframe_due = true;
                }
            }
        }
    }

    fn frame(&mut self, raw: &RawFrame) {
        let picture = match self.decoder.picture(raw) {
            Ok(picture) => picture,
            Err(error) => {
                tracing::debug!(%error, "a camera frame could not be read");
                return;
            }
        };
        (self.preview)(&picture);
        for (index, state) in self.layers.iter_mut().enumerate() {
            if !state.active || !due(state, picture.captured) {
                continue;
            }
            let layer = &state.layer;
            let input = if (picture.width, picture.height) == (layer.width, layer.height) {
                None
            } else {
                match self
                    .scaler
                    .picture(&picture, layer.width, layer.height, PixelFormat::Nv12)
                {
                    Ok(scaled) => Some(scaled),
                    Err(error) => {
                        tracing::debug!(%error, "a layer could not be scaled");
                        continue;
                    }
                }
            };
            let input = input.as_ref().unwrap_or(&picture);
            if state.encoder.is_none() {
                let config = EncoderConfig {
                    width: layer.width,
                    height: layer.height,
                    fps: layer.fps,
                    bitrate: state.bitrate,
                };
                match Encoder::open(config, &Backend::ALL) {
                    Ok(encoder) => {
                        tracing::debug!(layer = %layer.rid, backend = ?encoder.backend(), "encoding");
                        state.encoder = Some(encoder);
                        state.keyframe_due = true;
                    }
                    Err(error) => {
                        tracing::warn!(%error, layer = %layer.rid, "no encoder for a layer");
                        continue;
                    }
                }
            }
            let Some(encoder) = state.encoder.as_mut() else {
                continue;
            };
            let keyframe = std::mem::take(&mut state.keyframe_due);
            match encoder.encode(input, keyframe) {
                Ok(Some(encoded)) => (self.send)(VideoFrame {
                    track_id: self.track_id.clone(),
                    layer: index as u8,
                    keyframe: encoded.keyframe,
                    width: u16::try_from(layer.width).unwrap_or(u16::MAX),
                    height: u16::try_from(layer.height).unwrap_or(u16::MAX),
                    captured: picture.captured,
                    nal_units: encoded.nal_units,
                }),
                Ok(None) => state.keyframe_due |= keyframe,
                Err(error) => {
                    tracing::warn!(%error, layer = %layer.rid, "an encoder failed; opening another");
                    state.encoder = None;
                    state.keyframe_due = true;
                }
            }
        }
    }
}

/// Whether a layer takes a picture captured at `captured`, at its own
/// frame rate (a quarter of a frame's leeway for capture jitter).
fn due(state: &mut LayerState, captured: Instant) -> bool {
    let interval = Duration::from_secs(1) / state.layer.fps.max(1);
    if let Some(next) = state.next_at
        && captured + interval / 4 < next
    {
        return false;
    }
    // The slot this picture fills; the next is due a frame after it.
    let next = state.next_at.unwrap_or(captured) + interval;
    // Far behind (a pause in the camera): start again from now.
    state.next_at = Some(if next + interval < captured {
        captured + interval
    } else {
        next
    });
    true
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use opencord_common::video::{VideoKind, layer_ceiling};

    use super::*;
    use crate::video::camera::{RawFormat, RawFrame};
    use crate::video::codec::decoder::Decoder;
    use crate::video::pattern::scene;

    #[test]
    fn a_720p_camera_gets_the_plans_three_layers() {
        let layers = camera_layers(1280, 720);

        let shape: Vec<(&str, u32, u32, u32, u32)> = layers
            .iter()
            .map(|l| (l.rid.as_str(), l.width, l.height, l.fps, l.max_bitrate))
            .collect();
        assert_eq!(
            shape,
            vec![
                ("l", 320, 180, 15, 150_000),
                ("m", 640, 360, 30, 500_000),
                ("h", 1280, 720, 30, 1_500_000),
            ]
        );
    }

    #[test]
    fn layers_keep_the_cameras_shape_and_stay_under_their_ceilings() {
        for (width, height) in [
            (640, 480),
            (1920, 1080),
            (1280, 720),
            (320, 240),
            (1280, 960),
        ] {
            let layers = camera_layers(width, height);

            assert!(!layers.is_empty());
            let top = layers.last().unwrap();
            assert!(top.height <= 720 && top.height <= height);
            for layer in &layers {
                assert!(layer.width % 2 == 0 && layer.height % 2 == 0);
                let shape = f64::from(layer.width) / f64::from(layer.height);
                let camera = f64::from(width) / f64::from(height);
                assert!((shape - camera).abs() < 0.02, "{width}×{height}: {layer:?}");
                let ceiling =
                    layer_ceiling(VideoKind::Camera, layer.width, layer.height, layer.fps);
                assert!(layer.max_bitrate <= ceiling, "{layer:?} over {ceiling}");
            }
            let heights: Vec<u32> = layers.iter().map(|layer| layer.height).collect();
            assert!(
                heights.windows(2).all(|pair| pair[0] < pair[1]),
                "{heights:?}"
            );
        }
        assert_eq!(camera_layers(1920, 1080).last().unwrap().width, 1280);
        assert_eq!(camera_layers(640, 480).last().unwrap().height, 480);
    }

    /// Feeds `count` frames of the moving scene at 30 fps, in real time.
    fn feed(frames: &mpsc::SyncSender<RawFrame>, from: u64, count: u64) {
        let start = Instant::now();
        for number in from..from + count {
            let picture = scene(1280, 720, number, Instant::now());
            let frame = RawFrame {
                format: RawFormat::Nv12,
                width: 1280,
                height: 720,
                data: picture.data,
                captured: picture.captured,
            };
            let _ = frames.send(frame);
            let due = start + Duration::from_millis(33 * (number - from + 1));
            std::thread::sleep(due.saturating_duration_since(Instant::now()));
        }
    }

    fn started() -> (
        CameraSender,
        mpsc::SyncSender<RawFrame>,
        mpsc::Receiver<VideoFrame>,
    ) {
        let (frames, captured) = mpsc::sync_channel(4);
        let (sent, received) = mpsc::channel();
        let sender = CameraSender::start(
            "cam".to_owned(),
            camera_layers(1280, 720),
            captured,
            move |frame| {
                let _ = sent.send(frame);
            },
            |_| {},
        );
        (sender, frames, received)
    }

    fn drain(received: &mpsc::Receiver<VideoFrame>) -> Vec<VideoFrame> {
        std::thread::sleep(Duration::from_millis(200));
        received.try_iter().collect()
    }

    #[test]
    fn each_layer_takes_pictures_at_its_own_frame_rate() {
        let layers = camera_layers(1280, 720);
        let start = Instant::now();
        let mut taken = [0; 3];
        let mut states: Vec<LayerState> = layers
            .into_iter()
            .map(|layer| LayerState {
                bitrate: layer.max_bitrate,
                layer,
                active: true,
                encoder: None,
                keyframe_due: true,
                next_at: None,
            })
            .collect();

        // 30 frames at exactly 30 fps, 4 ms of jitter on every third.
        for number in 0..30u64 {
            let jitter = if number % 3 == 0 { 4_000_000 } else { 0 };
            let captured = start + Duration::from_nanos(33_333_333 * number + jitter);
            for (index, state) in states.iter_mut().enumerate() {
                taken[index] += usize::from(due(state, captured));
            }
        }

        assert_eq!(taken, [15, 30, 30]);
    }

    #[test]
    fn every_layer_comes_out_at_its_size_starting_with_a_keyframe() {
        let (_sender, frames, received) = started();

        feed(&frames, 0, 30);
        let sent = drain(&received);

        // Counts depend on how busy the machine is (the newest frame wins);
        // the rates themselves are tested above.
        for (layer, (width, height)) in [(0u8, (320, 180)), (1, (640, 360)), (2, (1280, 720))] {
            let of_layer: Vec<&VideoFrame> =
                sent.iter().filter(|frame| frame.layer == layer).collect();
            assert!(
                of_layer.len() >= 2,
                "layer {layer}: {} pictures",
                of_layer.len()
            );
            assert!(of_layer[0].keyframe, "layer {layer} starts with a keyframe");
            assert!(of_layer[1..].iter().all(|frame| !frame.keyframe));
            assert_eq!((of_layer[0].width, of_layer[0].height), (width, height));
            let mut decoder = Decoder::open(false).unwrap();
            let pictures = decoder
                .decode(&of_layer[0].nal_units, of_layer[0].captured)
                .unwrap();
            assert_eq!(
                (pictures[0].width, pictures[0].height),
                (width.into(), height.into())
            );
        }
    }

    #[test]
    fn only_the_layers_asked_for_are_encoded_and_a_returning_one_starts_with_a_keyframe() {
        let (sender, frames, received) = started();
        feed(&frames, 0, 10);
        drain(&received);

        sender.encode(&["l".to_owned(), "m".to_owned()], &[150_000, 500_000]);
        feed(&frames, 10, 10);
        let without_top = drain(&received);
        sender.encode(
            &["l".to_owned(), "m".to_owned(), "h".to_owned()],
            &[150_000, 500_000, 1_500_000],
        );
        feed(&frames, 20, 10);
        let with_top = drain(&received);

        assert!(without_top.iter().all(|frame| frame.layer != 2));
        let top: Vec<&VideoFrame> = with_top.iter().filter(|frame| frame.layer == 2).collect();
        assert!(top[0].keyframe);
    }

    #[test]
    fn a_keyframe_comes_when_asked_for() {
        let (sender, frames, received) = started();
        feed(&frames, 0, 10);
        drain(&received);

        sender.keyframe(1);
        feed(&frames, 10, 4);
        let sent = drain(&received);

        let middle: Vec<&VideoFrame> = sent.iter().filter(|frame| frame.layer == 1).collect();
        assert!(middle[0].keyframe);
        assert!(
            sent.iter()
                .filter(|frame| frame.layer == 2)
                .all(|frame| !frame.keyframe)
        );
    }

    #[test]
    fn a_new_bitrate_restarts_the_layer_with_a_keyframe() {
        let (sender, frames, received) = started();
        feed(&frames, 0, 10);
        drain(&received);

        sender.encode(
            &["l".to_owned(), "m".to_owned(), "h".to_owned()],
            &[150_000, 500_000, 600_000],
        );
        feed(&frames, 10, 30);
        let sent = drain(&received);

        let top: Vec<&VideoFrame> = sent.iter().filter(|frame| frame.layer == 2).collect();
        assert!(top[0].keyframe);
        let bytes: usize = top
            .iter()
            .flat_map(|frame| &frame.nal_units)
            .map(Vec::len)
            .sum();
        let bitrate = bytes * 8 * 30 / top.len();
        assert!(bitrate < 1_000_000, "{bitrate} bit/s");
    }

    #[test]
    fn the_sender_ends_when_the_camera_does() {
        let (sender, frames, _received) = started();
        feed(&frames, 0, 3);

        drop(frames);
        let ended = Instant::now();
        drop(sender);

        // At most the picture it was encoding when the camera stopped.
        assert!(ended.elapsed() < Duration::from_secs(3));
    }
}
