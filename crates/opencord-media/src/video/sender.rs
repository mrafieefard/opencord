//! A video track's send side (plan §7.8, §8, §9.2): camera or screen frames
//! become NV12 pictures, each layer's size, and H.264 from each layer's
//! encoder, as the transport asks (which layers, at what bitrate, keyframes
//! when someone lost a picture, and for a screen how much of its frame rate
//! and pixels to keep). One thread per track; the newest frame wins when
//! the encoders fall behind. A screen that has not changed is sent again
//! every second, so the stream stays alive.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::transport::{Layer, VideoFrame};
use crate::video::camera::RawFrame;
use crate::video::codec::convert;
use crate::video::codec::encoder::{Backend, Encoder, EncoderConfig};
use crate::video::codec::raw::RawDecoder;
use crate::video::codec::scale::Scaler;
use crate::video::layers::{ScreenShape, ScreenSizes, screen_sizes};
use crate::video::picture::{Picture, PixelFormat};

/// How long the thread waits for a frame before it looks at its commands.
const WAIT: Duration = Duration::from_millis(100);
/// A bitrate this much away from an encoder's reopens it.
const BITRATE_CHANGE: f64 = 0.2;
/// A screen that has not changed is sent again this often (plan §9.2).
const KEEPALIVE: Duration = Duration::from_secs(1);

enum Command {
    Encode {
        rids: Vec<String>,
        bitrates: Vec<u32>,
        fps_scale: f32,
        size_scale: f32,
    },
    Keyframe(u8),
}

/// A track's encoding thread; stops when dropped or when its frames end.
pub struct VideoSender {
    commands: mpsc::Sender<Command>,
    thread: Option<JoinHandle<()>>,
}

impl VideoSender {
    /// Encodes a camera's frames into `layers`, handing each picture to
    /// `send` and each camera frame to `preview` first.
    pub fn camera(
        track_id: String,
        layers: Vec<Layer>,
        frames: Receiver<RawFrame>,
        send: impl FnMut(VideoFrame) + Send + 'static,
        preview: impl FnMut(&Picture) + Send + 'static,
    ) -> Self {
        Self::start(Sending::new(track_id, layers, None, send, preview), frames)
    }

    /// Encodes a screen's frames into `layers` (see `screen_layers`, from
    /// the screen's first size), resizing them as the screen or window
    /// changes size.
    pub fn screen(
        track_id: String,
        layers: Vec<Layer>,
        shape: ScreenShape,
        frames: Receiver<RawFrame>,
        send: impl FnMut(VideoFrame) + Send + 'static,
        preview: impl FnMut(&Picture) + Send + 'static,
    ) -> Self {
        Self::start(
            Sending::new(track_id, layers, Some(shape), send, preview),
            frames,
        )
    }

    fn start<S, P>(mut sending: Sending<S, P>, frames: Receiver<RawFrame>) -> Self
    where
        S: FnMut(VideoFrame) + Send + 'static,
        P: FnMut(&Picture) + Send + 'static,
    {
        let (commands, received) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("opencord-video-send".to_owned())
            .spawn(move || sending.run(&frames, &received))
            .ok();
        Self { commands, thread }
    }

    /// What to encode: the layers named, at these bitrates, and of a
    /// screen's main layer this share of its frame rate and of its pixels
    /// (the transport's `VoiceEvent::Encode`).
    pub fn encode(&self, rids: &[String], bitrates: &[u32], fps_scale: f32, size_scale: f32) {
        let _ = self.commands.send(Command::Encode {
            rids: rids.to_vec(),
            bitrates: bitrates.to_vec(),
            fps_scale,
            size_scale,
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

impl Drop for VideoSender {
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
    /// A screen's quality, and the planner's share of its main layer's
    /// frame rate and pixels; `None` for a camera.
    screen: Option<ScreenShape>,
    scales: (f32, f32),
    /// The newest picture and when it came, sent again while a screen
    /// does not change.
    last: Option<(Picture, Instant)>,
    decoder: RawDecoder,
    scaler: Scaler,
    send: S,
    preview: P,
}

impl<S: FnMut(VideoFrame), P: FnMut(&Picture)> Sending<S, P> {
    fn new(
        track_id: String,
        layers: Vec<Layer>,
        screen: Option<ScreenShape>,
        send: S,
        preview: P,
    ) -> Self {
        Self {
            screen,
            scales: (1.0, 1.0),
            last: None,
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
            let repeat = frame.is_none() && self.screen.is_some();
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
            } else if repeat {
                self.repeat();
            }
        }
    }

    /// Sends a screen's last picture again once it has not changed for
    /// [`KEEPALIVE`].
    fn repeat(&mut self) {
        let now = Instant::now();
        let Some((mut picture, at)) = self.last.take() else {
            return;
        };
        if now.saturating_duration_since(at) < KEEPALIVE {
            self.last = Some((picture, at));
            return;
        }
        picture.captured = now;
        self.encode_picture(picture);
    }

    fn apply(&mut self, command: Command) {
        match command {
            Command::Encode {
                rids,
                bitrates,
                fps_scale,
                size_scale,
            } => {
                self.scales = (fps_scale, size_scale);
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
                tracing::debug!(%error, "a frame could not be read");
                return;
            }
        };
        (self.preview)(&picture);
        self.encode_picture(picture);
    }

    /// A screen's layers at the sizes and frame rates it should have now:
    /// a layer that changes gets a new encoder, starting with a keyframe.
    fn fit_screen(&mut self, picture: &Picture) {
        let Some(shape) = self.screen else {
            return;
        };
        let (fps_scale, size_scale) = self.scales;
        let sizes = match screen_sizes(picture.width, picture.height, shape, fps_scale, size_scale)
        {
            ScreenSizes::None => return,
            ScreenSizes::Main(main) => vec![main],
            ScreenSizes::Both { low, main } => vec![low, main],
        };
        // The main layer is the last; a low one, if published, the first.
        let offset = self.layers.len().saturating_sub(sizes.len());
        for (state, (width, height, fps)) in self.layers.iter_mut().skip(offset).zip(sizes) {
            let layer = &mut state.layer;
            if (layer.width, layer.height, layer.fps) != (width, height, fps) {
                (layer.width, layer.height, layer.fps) = (width, height, fps);
                state.encoder = None;
                state.keyframe_due = true;
            }
        }
    }

    fn encode_picture(&mut self, picture: Picture) {
        self.fit_screen(&picture);
        let inputs = self.inputs(&picture);
        for ((index, state), input) in self.layers.iter_mut().enumerate().zip(&inputs) {
            let input = match input {
                Input::Skip => continue,
                Input::Camera => &picture,
                Input::Made(made) => made,
            };
            let layer = &state.layer;
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
        if self.screen.is_some() {
            self.last = Some((picture, Instant::now()));
        }
    }
}

/// What a layer encodes from a camera picture.
enum Input {
    /// Nothing: it is off, or not due at its frame rate.
    Skip,
    /// The camera's picture, at the layer's size already.
    Camera,
    Made(Picture),
}

impl Input {
    fn made(&self) -> Option<&Picture> {
        match self {
            Self::Made(picture) => Some(picture),
            Self::Skip | Self::Camera => None,
        }
    }
}

impl<S: FnMut(VideoFrame), P: FnMut(&Picture)> Sending<S, P> {
    /// Each layer's picture, made from the largest layer down: a layer half
    /// the size of a larger one's picture (or a quarter) is its mean,
    /// others are scaled from the camera's.
    fn inputs(&mut self, picture: &Picture) -> Vec<Input> {
        let mut inputs: Vec<Input> = self
            .layers
            .iter_mut()
            .map(|state| {
                if state.active && due(state, picture.captured) {
                    Input::Camera
                } else {
                    Input::Skip
                }
            })
            .collect();
        for index in (0..inputs.len()).rev() {
            if matches!(inputs[index], Input::Skip) {
                continue;
            }
            let layer = &self.layers[index].layer;
            let size = (layer.width, layer.height);
            if (picture.width, picture.height) == size {
                continue;
            }
            // Smallest first: fewer halvings.
            let halved = inputs[index + 1..]
                .iter()
                .filter_map(Input::made)
                .chain([picture])
                .find_map(|source| halved(source, size));
            inputs[index] = match halved {
                Some(made) => Input::Made(made),
                None => match self
                    .scaler
                    .picture(picture, size.0, size.1, PixelFormat::Nv12)
                {
                    Ok(scaled) => Input::Made(scaled),
                    Err(error) => {
                        tracing::debug!(%error, "a layer could not be scaled");
                        Input::Skip
                    }
                },
            };
        }
        inputs
    }
}

/// `source` halved once or twice to `size`, when that is how to get there.
fn halved(source: &Picture, size: (u32, u32)) -> Option<Picture> {
    let halves_to = |times: u32| (source.width >> times, source.height >> times) == size;
    if halves_to(1) {
        convert::halve_nv12(source)
    } else if halves_to(2) {
        convert::halve_nv12(&convert::halve_nv12(source)?)
    } else {
        None
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

    use super::*;
    use crate::video::camera::{RawFormat, RawFrame};
    use crate::video::codec::decoder::Decoder;
    use crate::video::layers::{camera_layers, screen_layers};
    use crate::video::pattern::scene;

    /// Feeds `count` frames of the moving scene at 30 fps, in real time.
    fn feed(frames: &mpsc::SyncSender<RawFrame>, from: u64, count: u64) {
        feed_sized(frames, from, count, (1280, 720));
    }

    fn feed_sized(
        frames: &mpsc::SyncSender<RawFrame>,
        from: u64,
        count: u64,
        (width, height): (u32, u32),
    ) {
        let start = Instant::now();
        for number in from..from + count {
            let picture = scene(width, height, number, Instant::now());
            let frame = RawFrame {
                format: RawFormat::Nv12,
                width,
                height,
                data: picture.data,
                captured: picture.captured,
            };
            let _ = frames.send(frame);
            let due = start + Duration::from_millis(33 * (number - from + 1));
            std::thread::sleep(due.saturating_duration_since(Instant::now()));
        }
    }

    fn started() -> (
        VideoSender,
        mpsc::SyncSender<RawFrame>,
        mpsc::Receiver<VideoFrame>,
    ) {
        let (frames, captured) = mpsc::sync_channel(4);
        let (sent, received) = mpsc::channel();
        let sender = VideoSender::camera(
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

        sender.encode(
            &["l".to_owned(), "m".to_owned()],
            &[150_000, 500_000],
            1.0,
            1.0,
        );
        feed(&frames, 10, 10);
        let without_top = drain(&received);
        sender.encode(
            &["l".to_owned(), "m".to_owned(), "h".to_owned()],
            &[150_000, 500_000, 1_500_000],
            1.0,
            1.0,
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
            1.0,
            1.0,
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

    const P720_30: ScreenShape = ScreenShape {
        max_pixels: 1280 * 720,
        fps: 30,
    };

    /// A screen sender for a 1280×720 screen at 720p30: a 640×360 low
    /// layer and the main one.
    fn started_screen() -> (
        VideoSender,
        mpsc::SyncSender<RawFrame>,
        mpsc::Receiver<VideoFrame>,
    ) {
        let (frames, captured) = mpsc::sync_channel(4);
        let (sent, received) = mpsc::channel();
        let sender = VideoSender::screen(
            "screen".to_owned(),
            screen_layers(1280, 720, P720_30),
            P720_30,
            captured,
            move |frame| {
                let _ = sent.send(frame);
            },
            |_| {},
        );
        (sender, frames, received)
    }

    fn sizes(sent: &[VideoFrame], layer: u8) -> Vec<(u16, u16, bool)> {
        sent.iter()
            .filter(|frame| frame.layer == layer)
            .map(|frame| (frame.width, frame.height, frame.keyframe))
            .collect()
    }

    #[test]
    fn a_screen_sends_its_main_and_low_layers_and_follows_a_resized_window() {
        let (_sender, frames, received) = started_screen();

        feed_sized(&frames, 0, 10, (1280, 720));
        let before = drain(&received);
        feed_sized(&frames, 10, 10, (1024, 768));
        let after = drain(&received);

        assert!(
            sizes(&before, 1)
                .iter()
                .all(|size| size.0 == 1280 && size.1 == 720)
        );
        assert!(
            sizes(&before, 0)
                .iter()
                .all(|size| size.0 == 640 && size.1 == 360)
        );
        let main = sizes(&after, 1);
        assert_eq!(
            main[0],
            (1024, 768, true),
            "a new size starts with a keyframe"
        );
        assert_eq!(sizes(&after, 0)[0], (554, 414, true));
    }

    #[test]
    fn a_screen_that_does_not_change_is_sent_again_every_second() {
        let (_sender, frames, received) = started_screen();
        feed_sized(&frames, 0, 3, (1280, 720));
        drain(&received);

        std::thread::sleep(Duration::from_millis(2_600));
        let repeated = sizes(&received.try_iter().collect::<Vec<_>>(), 1);

        assert!(
            (2..=3).contains(&repeated.len()),
            "{} pictures in 2.6 s",
            repeated.len()
        );
    }

    #[test]
    fn the_planners_cuts_shrink_and_slow_a_screens_main_layer() {
        let (sender, frames, received) = started_screen();
        feed_sized(&frames, 0, 5, (1280, 720));
        drain(&received);

        sender.encode(
            &["l".to_owned(), "h".to_owned()],
            &[300_000, 250_000],
            0.5,
            0.25,
        );
        feed_sized(&frames, 5, 30, (1280, 720));
        let sent = drain(&received);

        let main = sizes(&sent, 1);
        assert!(
            main.iter().all(|size| (size.0, size.1) == (640, 360)),
            "{main:?}"
        );
        assert!(
            (12..=17).contains(&main.len()),
            "{} pictures in 1 s",
            main.len()
        );
    }
}
