//! A remote track's receive side (plan §7.8, §7.11): its pictures decoded
//! on a thread of their own (hardware for layers over 360 lines, software
//! for small ones, so many tiles do not use up hardware decoders) and
//! handed on as RGBA at the size of the tile showing them, never larger
//! than the picture. A tile that is hidden stops its decoder.

use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::transport::ReceivedVideo;
use crate::video::codec::decoder::Decoder;
use crate::video::codec::scale::Scaler;
use crate::video::picture::Picture;
use crate::video::stats::ReceiveStats;

/// Layers taller than this decode in hardware where there is some.
const HARDWARE_ABOVE: u16 = 360;

/// A picture to show: RGBA rows.
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    /// When its encoded picture arrived.
    pub arrived: Instant,
}

/// Takes a hardware-decoded picture still on the GPU, to be drawn at the
/// size given; `false` when it cannot, and pictures come as RGBA from then
/// on (plan §7.11, zero-copy).
#[cfg(target_os = "linux")]
pub type GpuSink =
    Box<dyn FnMut(crate::video::codec::dmabuf::DmaBufPicture, (u32, u32)) -> bool + Send>;

enum Command {
    Video(ReceivedVideo),
    Size(u32, u32),
    Hold(Duration),
}

/// One remote track's decoding thread; stops when dropped.
pub struct VideoReceiver {
    commands: mpsc::Sender<Command>,
    thread: Option<JoinHandle<()>>,
}

impl VideoReceiver {
    /// Decodes what is pushed while the tile has a size, handing pictures
    /// to `show`; calls `lost` when a picture could not be decoded, so the
    /// sender can be asked for a keyframe.
    pub fn start(
        show: impl FnMut(Rgba) + Send + 'static,
        lost: impl FnMut() + Send + 'static,
    ) -> Self {
        Self::spawn(Receiving::new(show, lost))
    }

    /// Like [`Self::start`], with hardware-decoded pictures handed to `gpu`
    /// as they are, without copying them back from the GPU.
    #[cfg(target_os = "linux")]
    pub fn start_with_gpu(
        show: impl FnMut(Rgba) + Send + 'static,
        gpu: GpuSink,
        lost: impl FnMut() + Send + 'static,
    ) -> Self {
        let mut receiving = Receiving::new(show, lost);
        receiving.gpu = Some(gpu);
        Self::spawn(receiving)
    }

    fn spawn<S, L>(mut receiving: Receiving<S, L>) -> Self
    where
        S: FnMut(Rgba) + Send + 'static,
        L: FnMut() + Send + 'static,
    {
        let (commands, received) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("opencord-video-receive".to_owned())
            .spawn(move || receiving.run(&received))
            .ok();
        Self { commands, thread }
    }

    pub fn push(&self, video: ReceivedVideo) {
        let _ = self.commands.send(Command::Video(video));
    }

    /// The tile's size in pixels; 0 by 0 hides it.
    pub fn set_size(&self, width: u32, height: u32) {
        let _ = self.commands.send(Command::Size(width, height));
    }

    /// Shows each picture this long after it arrived, at the soonest: as
    /// long as its sound waits to play, so the two stay together (plan
    /// §10.5).
    pub fn set_hold(&self, hold: Duration) {
        let _ = self.commands.send(Command::Hold(hold.min(MAX_HOLD)));
    }
}

impl Drop for VideoReceiver {
    fn drop(&mut self) {
        let (closed, _) = mpsc::channel();
        drop(std::mem::replace(&mut self.commands, closed));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Receiving<S, L> {
    show: S,
    lost: L,
    size: (u32, u32),
    software: Option<Decoder>,
    hardware: Option<Decoder>,
    /// Height from the latest keyframe: which decoder to use.
    height: u16,
    awaiting_keyframe: bool,
    scaler: Scaler,
    hold: Duration,
    #[cfg(target_os = "linux")]
    gpu: Option<GpuSink>,
    /// For the session log (D82).
    stats: ReceiveStats,
}

/// The most a picture waits for its sound.
const MAX_HOLD: Duration = Duration::from_millis(250);

impl<S: FnMut(Rgba), L: FnMut()> Receiving<S, L> {
    fn new(show: S, lost: L) -> Self {
        Self {
            show,
            lost,
            size: (0, 0),
            software: None,
            hardware: None,
            height: 0,
            awaiting_keyframe: true,
            scaler: Scaler::new(),
            hold: Duration::ZERO,
            #[cfg(target_os = "linux")]
            gpu: None,
            stats: ReceiveStats::new(Instant::now()),
        }
    }

    fn run(&mut self, commands: &mpsc::Receiver<Command>) {
        tracing::info!("watching a video track");
        while let Ok(command) = commands.recv() {
            if let Some(line) = self.stats.due(Instant::now()) {
                tracing::info!("receiving: {line}");
            }
            match command {
                Command::Size(width, height) => {
                    let hidden = width == 0 || height == 0;
                    if hidden {
                        // Decoders hold memory and hardware sessions.
                        self.software = None;
                        self.hardware = None;
                        self.awaiting_keyframe = true;
                    }
                    self.size = (width, height);
                }
                Command::Video(video) => self.video(&video),
                Command::Hold(hold) => self.hold = hold,
            }
        }
    }

    fn video(&mut self, video: &ReceivedVideo) {
        self.stats.received();
        if self.size.0 == 0 || self.size.1 == 0 {
            return;
        }
        if video.keyframe {
            self.awaiting_keyframe = false;
            if let Some((_, height)) = video.size {
                self.height = height;
            }
        }
        if self.awaiting_keyframe {
            // What came before it was missed, or could not be decoded.
            self.stats.lost();
            (self.lost)();
            return;
        }
        let large = self.height > HARDWARE_ABOVE;
        let slot = if large {
            &mut self.hardware
        } else {
            &mut self.software
        };
        if slot.is_none() {
            match Decoder::open(large) {
                Ok(decoder) => {
                    tracing::info!(
                        wanted_hardware = large,
                        hardware = decoder.is_hardware(),
                        height = self.height,
                        "decoder opened"
                    );
                    *slot = Some(decoder);
                }
                Err(error) => {
                    tracing::warn!(%error, wanted_hardware = large, "no decoder");
                    return;
                }
            }
        }
        let Some(decoder) = slot.as_mut() else {
            return;
        };
        #[cfg(target_os = "linux")]
        if large && self.gpu.is_some() && decoder.is_hardware() {
            use crate::video::codec::decoder::Decoded;
            match decoder.decode_gpu(&video.nal_units, video.arrived) {
                Ok(decoded) => {
                    for decoded in decoded {
                        self.stats.decoded();
                        match decoded {
                            Decoded::Gpu(picture) => self.present_gpu(picture, video.arrived),
                            Decoded::Picture(picture) => self.present(&picture, video.arrived),
                        }
                    }
                }
                Err(error) => {
                    tracing::debug!(%error, "a picture could not be decoded");
                    self.stats.decode_error();
                    self.stats.lost();
                    *slot = None;
                    self.awaiting_keyframe = true;
                    (self.lost)();
                }
            }
            return;
        }
        match decoder.decode(&video.nal_units, video.arrived) {
            Ok(pictures) => {
                for picture in pictures {
                    self.stats.decoded();
                    self.present(&picture, video.arrived);
                }
            }
            Err(error) => {
                tracing::debug!(%error, "a picture could not be decoded");
                self.stats.decode_error();
                self.stats.lost();
                *slot = None;
                self.awaiting_keyframe = true;
                (self.lost)();
            }
        }
    }

    /// Hands a picture still on the GPU on; one the sink refuses turns the
    /// GPU path off.
    #[cfg(target_os = "linux")]
    fn present_gpu(
        &mut self,
        picture: crate::video::codec::dmabuf::DmaBufPicture,
        arrived: Instant,
    ) {
        let size = fit(picture.width, picture.height, self.size.0, self.size.1);
        self.wait_for_sound(arrived);
        let taken = self.gpu.as_mut().is_some_and(|gpu| gpu(picture, size));
        if !taken {
            tracing::info!("pictures come back from the GPU from now on");
            self.gpu = None;
        }
    }

    /// Its sound plays this long after arriving; a picture waits as long.
    fn wait_for_sound(&self, arrived: Instant) {
        let due = arrived + self.hold;
        let now = Instant::now();
        if due > now {
            std::thread::sleep(due - now);
        }
    }

    fn present(&mut self, picture: &Picture, arrived: Instant) {
        let (width, height) = fit(picture.width, picture.height, self.size.0, self.size.1);
        let data = self.scaler.rgba(picture, width, height);
        self.wait_for_sound(arrived);
        match data {
            Ok(data) => (self.show)(Rgba {
                width,
                height,
                data,
                arrived,
            }),
            Err(error) => tracing::debug!(%error, "a picture could not be converted"),
        }
    }
}

/// A picture's size fitted into a tile, its shape kept, never larger than
/// it is, even.
fn fit(width: u32, height: u32, tile_width: u32, tile_height: u32) -> (u32, u32) {
    let scale = (f64::from(tile_width) / f64::from(width))
        .min(f64::from(tile_height) / f64::from(height))
        .min(1.0);
    let even = |value: f64| ((value.round() as u32) & !1).max(2);
    (
        even(f64::from(width) * scale),
        even(f64::from(height) * scale),
    )
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::transport::ReceivedVideo;
    use crate::video::codec::encoder::{Backend, Encoder, EncoderConfig};
    use crate::video::pattern::scene;

    /// `count` encoded pictures of the moving scene at this size.
    fn stream(width: u32, height: u32, count: u64) -> Vec<ReceivedVideo> {
        let config = EncoderConfig {
            width,
            height,
            fps: 30,
            bitrate: 1_000_000,
        };
        let mut encoder = Encoder::open(config, &Backend::ALL).expect("an encoder");
        let start = Instant::now();
        (0..count)
            .filter_map(|number| {
                let picture = scene(width, height, number, start);
                let encoded = encoder.encode(&picture, number == 0).unwrap()?;
                Some(ReceivedVideo {
                    user_id: 7,
                    track_id: "cam".to_owned(),
                    layer: u8::from(height > 360),
                    keyframe: encoded.keyframe,
                    size: Some((width as u16, height as u16)),
                    timestamp: number as u32 * 3000,
                    nal_units: encoded.nal_units,
                    arrived: Instant::now(),
                })
            })
            .collect()
    }

    fn started(
        width: u32,
        height: u32,
    ) -> (VideoReceiver, mpsc::Receiver<Rgba>, mpsc::Receiver<()>) {
        let (shown, frames) = mpsc::channel();
        let (lost, losses) = mpsc::channel();
        let receiver = VideoReceiver::start(
            move |rgba| {
                let _ = shown.send(rgba);
            },
            move || {
                let _ = lost.send(());
            },
        );
        receiver.set_size(width, height);
        (receiver, frames, losses)
    }

    fn collect(frames: &mpsc::Receiver<Rgba>, count: usize) -> Vec<Rgba> {
        let mut got = Vec::new();
        while got.len() < count {
            match frames.recv_timeout(Duration::from_secs(3)) {
                Ok(rgba) => got.push(rgba),
                Err(_) => break,
            }
        }
        got
    }

    #[test]
    fn pictures_wait_as_long_as_their_sound_does() {
        crate::video::codec::needs_codec!();
        let (receiver, frames, _) = started(320, 180);
        receiver.set_hold(Duration::from_millis(120));

        for mut video in stream(640, 360, 5) {
            video.arrived = Instant::now();
            receiver.push(video);
        }
        let shown = collect(&frames, 5);

        assert_eq!(shown.len(), 5);
        for rgba in &shown {
            let waited = rgba.arrived.elapsed();
            assert!(waited >= Duration::from_millis(120), "{waited:?}");
        }
    }

    #[test]
    fn a_picture_is_fitted_into_its_tile() {
        crate::video::codec::needs_codec!();
        assert_eq!(fit(1280, 720, 640, 360), (640, 360));
        assert_eq!(fit(1280, 720, 400, 400), (400, 224));
        assert_eq!(fit(640, 360, 1920, 1080), (640, 360));
        assert_eq!(fit(640, 480, 640, 360), (480, 360));
    }

    #[test]
    fn pictures_come_out_as_rgba_at_the_tile_size() {
        crate::video::codec::needs_codec!();
        let (receiver, frames, _) = started(320, 180);

        for video in stream(640, 360, 10) {
            receiver.push(video);
        }
        let shown = collect(&frames, 10);

        assert_eq!(shown.len(), 10);
        for rgba in &shown {
            assert_eq!((rgba.width, rgba.height), (320, 180));
            assert_eq!(rgba.data.len(), 320 * 180 * 4);
        }
    }

    #[test]
    fn a_tile_larger_than_the_picture_gets_the_picture_size_and_its_shape() {
        crate::video::codec::needs_codec!();
        let (receiver, frames, _) = started(1920, 1440);

        for video in stream(640, 360, 3) {
            receiver.push(video);
        }
        let shown = collect(&frames, 3);

        // Fitted into the tile, kept no larger than the picture.
        assert_eq!((shown[0].width, shown[0].height), (640, 360));
    }

    #[test]
    fn a_receiver_that_missed_the_keyframe_asks_for_one() {
        crate::video::codec::needs_codec!();
        let (receiver, frames, losses) = started(320, 180);
        let mut videos = stream(640, 360, 4);

        // Its keyframe went by before the receiver was there.
        videos.remove(0);
        for video in videos {
            receiver.push(video);
        }

        assert!(losses.recv_timeout(Duration::from_secs(3)).is_ok());
        assert!(frames.recv_timeout(Duration::from_millis(200)).is_err());
    }

    #[test]
    fn a_hidden_tile_decodes_nothing() {
        crate::video::codec::needs_codec!();
        let (receiver, frames, _) = started(320, 180);
        receiver.set_size(0, 0);

        for video in stream(320, 180, 5) {
            receiver.push(video);
        }

        assert!(frames.recv_timeout(Duration::from_millis(500)).is_err());
    }

    #[test]
    fn large_layers_decode_like_small_ones() {
        crate::video::codec::needs_codec!();
        let (receiver, frames, _) = started(1280, 720);

        for video in stream(1280, 720, 5) {
            receiver.push(video);
        }
        let shown = collect(&frames, 5);

        assert_eq!(shown.len(), 5);
        assert_eq!((shown[0].width, shown[0].height), (1280, 720));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn large_hardware_pictures_stay_on_the_gpu_until_the_sink_refuses_one() {
        crate::video::codec::needs_codec!();
        if !Decoder::open(true).is_ok_and(|decoder| decoder.is_hardware()) {
            eprintln!("skipped: no hardware decoder here");
            return;
        }
        let (rgba, frames) = mpsc::channel();
        let (gpu_sizes, on_gpu) = mpsc::channel();
        let mut taken = 0;
        let receiver = VideoReceiver::start_with_gpu(
            move |picture| {
                let _ = rgba.send(picture);
            },
            Box::new(move |picture, size| {
                let _ = gpu_sizes.send((picture.width, picture.height, size));
                taken += 1;
                taken <= 3
            }),
            || {},
        );
        receiver.set_size(640, 360);

        for video in stream(1280, 720, 8) {
            receiver.push(video);
        }
        let on_gpu: Vec<_> = (0..4)
            .filter_map(|_| on_gpu.recv_timeout(Duration::from_secs(5)).ok())
            .collect();
        let back = collect(&frames, 4);

        assert_eq!(on_gpu.len(), 4, "three taken, the fourth refused");
        assert_eq!(
            on_gpu[0],
            (1280, 720, (640, 360)),
            "drawn at the tile's size"
        );
        assert_eq!(back.len(), 4, "then in memory, as RGBA");
        assert_eq!((back[0].width, back[0].height), (640, 360));
    }

    #[test]
    fn damage_asks_for_a_keyframe() {
        crate::video::codec::needs_codec!();
        let (receiver, _frames, losses) = started(320, 180);
        let mut pictures = stream(320, 180, 4);
        for unit in &mut pictures[2].nal_units {
            unit.truncate(3);
            unit.push(0xff);
        }

        for video in pictures {
            receiver.push(video);
        }

        assert!(losses.recv_timeout(Duration::from_secs(3)).is_ok());
    }
}
