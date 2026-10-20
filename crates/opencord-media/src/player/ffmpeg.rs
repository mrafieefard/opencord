//! The media player on FFmpeg (Linux and Windows): one thread per player
//! demuxes, decodes, and paces pictures and sound to their outputs.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Once};
use std::time::{Duration, Instant};

use ffmpeg_next as ffmpeg;
use ffmpeg_next::format::Pixel;
use ffmpeg_next::format::context::StreamIo;
use ffmpeg_next::software::scaling::{self, flag::Flags};
use ffmpeg_next::util::error::EAGAIN;
use ffmpeg_next::{Packet, Rational, frame};

use super::decode::{StereoResampler, convert_audio_frame, rotate, target_video_size};
use super::sync::{FrameAction, MediaClock};
use super::{AudioOut, MediaSource, PlayerError, PlayerEvent, Probe, VideoOut};

/// How far ahead of the listener the sound is written: 60 ms at 48 kHz.
const AHEAD_FRAMES: usize = 2_880;
/// Sound decoded ahead of writing it, in samples (0.5 s of stereo).
const AUDIO_BUFFER: usize = 48_000;
/// Pictures decoded ahead of showing them.
const VIDEO_QUEUE: usize = 8;
/// Pictures are scaled to at most this on their long side.
const MAX_SIDE: u32 = 1_920;
/// Sound still queued when it counts as played out: an output may hold
/// back part of a tick it never gets the rest of (20 ms).
const END_SLACK_FRAMES: usize = 960;
const POSITION_EVERY: Duration = Duration::from_millis(50);
/// How long the thread waits for a command when it has nothing to do.
const IDLE_PLAYING: Duration = Duration::from_millis(5);
const IDLE_PAUSED: Duration = Duration::from_millis(100);

fn init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Err(error) = ffmpeg::init() {
            tracing::warn!(%error, "FFmpeg failed to initialize in media player");
        }
        ffmpeg::log::set_level(ffmpeg::log::Level::Fatal);
    });
}

pub fn supported() -> bool {
    true
}

enum Command {
    Play,
    Pause,
    Seek(u32),
    SetVolume(f32, bool),
    Close,
}

pub struct MediaPlayer {
    commands: Sender<Command>,
    cancelled: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl MediaPlayer {
    pub fn open(
        source: Box<dyn MediaSource>,
        video: Option<Arc<dyn VideoOut>>,
        audio: Box<dyn AudioOut>,
        events: impl FnMut(PlayerEvent) + Send + 'static,
    ) -> Result<Self, PlayerError> {
        init();
        let cancelled = Arc::new(AtomicBool::new(false));
        let (commands, received) = mpsc::channel();
        let thread_cancelled = cancelled.clone();
        let thread = std::thread::Builder::new()
            .name("opencord-media-player".to_owned())
            .spawn(move || run(source, video, audio, events, received, thread_cancelled))
            .map_err(|e| PlayerError::Open(e.to_string()))?;
        Ok(Self {
            commands,
            cancelled,
            thread: Some(thread),
        })
    }

    pub fn play(&self) {
        let _ = self.commands.send(Command::Play);
    }

    pub fn pause(&self) {
        let _ = self.commands.send(Command::Pause);
    }

    pub fn seek(&self, position_ms: u32) {
        let _ = self.commands.send(Command::Seek(position_ms));
    }

    pub fn set_volume(&self, volume: f32, muted: bool) {
        let _ = self.commands.send(Command::SetVolume(volume, muted));
    }

    pub fn close(mut self) {
        self.stop_and_join();
    }

    fn stop_and_join(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
        let _ = self.commands.send(Command::Close);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for MediaPlayer {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

fn run(
    source: Box<dyn MediaSource>,
    video_out: Option<Arc<dyn VideoOut>>,
    audio_out: Box<dyn AudioOut>,
    mut events: impl FnMut(PlayerEvent),
    commands: Receiver<Command>,
    cancelled: Arc<AtomicBool>,
) {
    let mut player = match Player::open(source, video_out, audio_out, cancelled.clone()) {
        Ok(player) => player,
        Err(message) => {
            if !cancelled.load(Ordering::Relaxed) {
                events(PlayerEvent::Failed(message));
            }
            return;
        }
    };
    events(PlayerEvent::Opened {
        duration_ms: player.duration_ms,
        width: player.width,
        height: player.height,
        has_video: player.video.is_some(),
        has_audio: player.audio.is_some(),
    });

    let mut wait = Duration::ZERO;
    loop {
        let command = match commands.recv_timeout(wait) {
            Ok(command) => Some(command),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        let result = match command {
            Some(Command::Close) => return,
            Some(command) => player.command(command, &mut events),
            None => player.step(&mut events),
        };
        wait = match result {
            Ok(true) => Duration::ZERO,
            Ok(false) if player.playing => IDLE_PLAYING,
            Ok(false) => IDLE_PAUSED,
            Err(message) => {
                if !cancelled.load(Ordering::Relaxed) {
                    events(PlayerEvent::Failed(message));
                }
                // Only Close is left to answer.
                while !matches!(commands.recv(), Ok(Command::Close) | Err(_)) {}
                return;
            }
        };
    }
}

/// A picture ready to show: its time, size and pixels.
struct Picture {
    pts_ms: u32,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

struct VideoTrack {
    index: usize,
    decoder: ffmpeg::decoder::Video,
    time_base: Rational,
    rotation: i32,
    scaler: Option<((Pixel, u32, u32), scaling::Context)>,
    queue: VecDeque<Picture>,
    drained: bool,
}

struct AudioTrack {
    index: usize,
    decoder: ffmpeg::decoder::Audio,
    time_base: Rational,
    resampler: StereoResampler,
    /// Interleaved stereo at 48 kHz, decoded and not yet written.
    pending: VecDeque<f32>,
    drained: bool,
}

struct Player {
    input: ffmpeg::format::context::Input,
    video_out: Option<Arc<dyn VideoOut>>,
    audio_out: Box<dyn AudioOut>,
    video: Option<VideoTrack>,
    audio: Option<AudioTrack>,
    /// The container's first timestamp, which plays as 0.
    origin_ms: i64,
    duration_ms: u32,
    width: u32,
    height: u32,
    clock: MediaClock,
    playing: bool,
    ended: bool,
    eof: bool,
    gain: f32,
    /// After a seek, what decodes before this is skipped.
    skip_video_before: Option<u32>,
    skip_audio_before: Option<u32>,
    /// A seek while paused shows its first picture.
    preview: bool,
    last_position: Instant,
}

impl Player {
    fn open(
        source: Box<dyn MediaSource>,
        video_out: Option<Arc<dyn VideoOut>>,
        audio_out: Box<dyn AudioOut>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        let io = StreamIo::from_read_seek(source).map_err(|e| e.to_string())?;
        let input = ffmpeg::format::input_from_stream_with_interrupt(io, None, None, move || {
            cancelled.load(Ordering::Relaxed)
        })
        .map_err(|e| e.to_string())?;

        // SAFETY: `input` owns a valid, opened AVFormatContext.
        let start = unsafe { (*input.as_ptr()).start_time };
        let origin_ms = if start == ffmpeg::ffi::AV_NOPTS_VALUE {
            0
        } else {
            start / 1_000
        };
        let duration_ms = u32::try_from(input.duration().max(0) / 1_000).unwrap_or(u32::MAX);

        let video = match input.streams().best(ffmpeg::media::Type::Video) {
            Some(stream) => Some(open_video(&stream).map_err(|e| format!("video: {e}"))?),
            None => None,
        };
        let audio = match input.streams().best(ffmpeg::media::Type::Audio) {
            Some(stream) => match open_audio(&stream) {
                Ok(track) => Some(track),
                // A picture with sound we can't decode still plays, silent.
                Err(error) if video.is_some() => {
                    tracing::warn!(%error, "the media player plays a video without its sound");
                    None
                }
                Err(error) => return Err(format!("audio: {error}")),
            },
            None => None,
        };
        if video.is_none() && audio.is_none() {
            return Err("the file has no video or audio".to_owned());
        }
        let (width, height) = match &video {
            Some(track) if matches!(track.rotation, 90 | 270) => {
                (track.decoder.height(), track.decoder.width())
            }
            Some(track) => (track.decoder.width(), track.decoder.height()),
            None => (0, 0),
        };
        let has_audio = audio.is_some();
        Ok(Self {
            input,
            video_out,
            audio_out,
            video,
            audio,
            origin_ms,
            duration_ms,
            width,
            height,
            clock: MediaClock::new(has_audio),
            playing: false,
            ended: false,
            eof: false,
            gain: 1.0,
            skip_video_before: None,
            skip_audio_before: None,
            preview: false,
            last_position: Instant::now(),
        })
    }

    fn command(
        &mut self,
        command: Command,
        events: &mut impl FnMut(PlayerEvent),
    ) -> Result<bool, String> {
        match command {
            Command::Play if !self.playing => {
                if self.ended {
                    self.seek(0, events)?;
                }
                self.playing = true;
                self.clock.play();
            }
            Command::Pause if self.playing => {
                self.playing = false;
                self.clock.pause();
            }
            Command::Seek(target_ms) => self.seek(target_ms, events)?,
            Command::SetVolume(volume, muted) => {
                self.gain = if muted { 0.0 } else { volume.clamp(0.0, 1.0) };
            }
            Command::Play | Command::Pause | Command::Close => {}
        }
        Ok(true)
    }

    fn seek(&mut self, target_ms: u32, events: &mut impl FnMut(PlayerEvent)) -> Result<(), String> {
        let target_ms = if self.duration_ms > 0 {
            target_ms.min(self.duration_ms)
        } else {
            target_ms
        };
        let ts = (i64::from(target_ms) + self.origin_ms) * 1_000;
        self.input
            .seek(ts, ..ts)
            .map_err(|e| format!("seek: {e}"))?;
        if let Some(video) = &mut self.video {
            video.decoder.flush();
            video.queue.clear();
            video.drained = false;
        }
        if let Some(audio) = &mut self.audio {
            audio.decoder.flush();
            audio.resampler.reset();
            audio.pending.clear();
            audio.drained = false;
        }
        self.eof = false;
        self.ended = false;
        self.skip_video_before = self.video.as_ref().map(|_| target_ms);
        self.skip_audio_before = self.audio.as_ref().map(|_| target_ms);
        self.preview = !self.playing && self.video.is_some();
        self.clock.seek(target_ms, self.audio.is_some());
        events(PlayerEvent::Position {
            position_ms: target_ms,
            buffering: false,
        });
        self.last_position = Instant::now();
        Ok(())
    }

    /// One round of work; whether anything was done.
    fn step(&mut self, events: &mut impl FnMut(PlayerEvent)) -> Result<bool, String> {
        let mut worked = false;
        if self.playing {
            worked |= self.write_audio();
            self.present_due();
        } else if self.preview
            && let Some(picture) = self.video.as_mut().and_then(|v| v.queue.pop_front())
        {
            self.preview = false;
            if let Some(out) = &self.video_out {
                out.present(picture.width, picture.height, picture.rgba);
            }
        }

        if self.wants_data() {
            if self.eof {
                self.drain()?;
            } else {
                self.read_packet()?;
            }
            worked = true;
        }

        if self.playing && self.audio_finished() {
            self.clock.audio_ended();
        }
        if self.playing && self.finished() {
            self.playing = false;
            self.ended = true;
            self.clock.pause();
            events(PlayerEvent::Ended);
        } else if self.playing && self.last_position.elapsed() >= POSITION_EVERY {
            let position = self.clock.now_ms(self.audio_out.queued_frames());
            events(PlayerEvent::Position {
                position_ms: if self.duration_ms > 0 {
                    position.min(self.duration_ms)
                } else {
                    position
                },
                buffering: false,
            });
            self.last_position = Instant::now();
        }
        Ok(worked)
    }

    /// Writes decoded sound up to 60 ms ahead of the listener, at the volume
    /// set now.
    fn write_audio(&mut self) -> bool {
        let Some(audio) = &mut self.audio else {
            return false;
        };
        let queued = self.audio_out.queued_frames();
        let frames = AHEAD_FRAMES
            .saturating_sub(queued)
            .min(audio.pending.len() / 2);
        if frames == 0 {
            return false;
        }
        let gain = self.gain;
        let chunk: Vec<f32> = audio
            .pending
            .iter()
            .take(frames * 2)
            .map(|s| s * gain)
            .collect();
        let written = self.audio_out.write(&chunk) / 2 * 2;
        audio.pending.drain(..written);
        self.clock.on_audio_written(written / 2);
        written > 0
    }

    fn present_due(&mut self) {
        let Some(video) = &mut self.video else {
            return;
        };
        while let Some(front) = video.queue.front() {
            match self
                .clock
                .check_video_frame(front.pts_ms, self.audio_out.queued_frames())
            {
                FrameAction::Wait => break,
                FrameAction::Drop => {
                    video.queue.pop_front();
                }
                FrameAction::Present => {
                    if let Some(picture) = video.queue.pop_front()
                        && let Some(out) = &self.video_out
                    {
                        out.present(picture.width, picture.height, picture.rgba);
                    }
                }
            }
        }
    }

    fn wants_data(&self) -> bool {
        let video = self
            .video
            .as_ref()
            .is_some_and(|v| !v.drained && v.queue.len() < VIDEO_QUEUE);
        let audio = self
            .audio
            .as_ref()
            .is_some_and(|a| !a.drained && a.pending.len() < AUDIO_BUFFER);
        video || audio
    }

    fn audio_finished(&self) -> bool {
        self.audio
            .as_ref()
            .is_some_and(|a| a.drained && a.pending.is_empty())
            && self.audio_out.queued_frames() <= END_SLACK_FRAMES
    }

    fn finished(&self) -> bool {
        let video = self
            .video
            .as_ref()
            .is_none_or(|v| v.drained && v.queue.is_empty());
        let audio = self.audio.is_none() || self.audio_finished();
        self.eof && video && audio
    }

    fn read_packet(&mut self) -> Result<(), String> {
        let mut packet = Packet::empty();
        match packet.read(&mut self.input) {
            Ok(()) => {}
            Err(ffmpeg::Error::Eof) => {
                self.eof = true;
                return Ok(());
            }
            // A corrupt packet: the demuxer resyncs past it.
            Err(ffmpeg::Error::InvalidData) => return Ok(()),
            Err(error) => return Err(format!("reading: {error}")),
        }
        let index = packet.stream();
        if let Some(video) = &mut self.video
            && video.index == index
        {
            send(&mut video.decoder, &packet)?;
            video.receive(self.origin_ms, &mut self.skip_video_before)?;
        } else if let Some(audio) = &mut self.audio
            && audio.index == index
        {
            send(&mut audio.decoder, &packet)?;
            audio.receive(self.origin_ms, &mut self.skip_audio_before)?;
        }
        Ok(())
    }

    /// At the end of the file, takes the frames the decoders still hold.
    fn drain(&mut self) -> Result<(), String> {
        if let Some(video) = &mut self.video
            && !video.drained
        {
            video.decoder.send_eof().map_err(|e| e.to_string())?;
            video.receive(self.origin_ms, &mut self.skip_video_before)?;
            video.drained = true;
        }
        if let Some(audio) = &mut self.audio
            && !audio.drained
        {
            audio.decoder.send_eof().map_err(|e| e.to_string())?;
            audio.receive(self.origin_ms, &mut self.skip_audio_before)?;
            audio.resampler.flush(&mut audio.pending)?;
            audio.drained = true;
        }
        Ok(())
    }
}

fn send(decoder: &mut ffmpeg::decoder::Opened, packet: &Packet) -> Result<(), String> {
    match decoder.send_packet(packet) {
        // A corrupt packet is skipped; the decoder conceals it.
        Ok(()) | Err(ffmpeg::Error::InvalidData) => Ok(()),
        Err(error) => Err(format!("decoding: {error}")),
    }
}

/// Whether a frame came out; `Ok(false)` once the decoder wants more.
fn received(result: Result<(), ffmpeg::Error>) -> Result<bool, String> {
    match result {
        Ok(()) => Ok(true),
        Err(ffmpeg::Error::Other { errno: EAGAIN } | ffmpeg::Error::Eof) => Ok(false),
        Err(error) => Err(format!("decoding: {error}")),
    }
}

/// `ts` in milliseconds from the container's start.
fn millis(ts: i64, time_base: Rational, origin_ms: i64) -> i64 {
    let ms = ts as f64 * f64::from(time_base) * 1_000.0;
    ms as i64 - origin_ms
}

impl VideoTrack {
    fn receive(&mut self, origin_ms: i64, skip_before: &mut Option<u32>) -> Result<(), String> {
        let mut decoded = frame::Video::empty();
        while received(self.decoder.receive_frame(&mut decoded))? {
            let pts = decoded.timestamp().unwrap_or(0);
            let pts_ms = millis(pts, self.time_base, origin_ms).max(0) as u32;
            if let Some(target) = *skip_before {
                if pts_ms < target {
                    continue;
                }
                *skip_before = None;
            }
            let (width, height, rgba) = self.picture(&decoded)?;
            self.queue.push_back(Picture {
                pts_ms,
                width,
                height,
                rgba,
            });
        }
        Ok(())
    }

    fn picture(&mut self, decoded: &frame::Video) -> Result<(u32, u32, Vec<u8>), String> {
        let key = (decoded.format(), decoded.width(), decoded.height());
        let (width, height) = target_video_size(decoded.width(), decoded.height(), MAX_SIDE);
        let scaler = match &mut self.scaler {
            Some((made_for, scaler)) if *made_for == key => scaler,
            slot => {
                let scaler = scaling::Context::get(
                    key.0,
                    key.1,
                    key.2,
                    Pixel::RGBA,
                    width,
                    height,
                    Flags::BILINEAR,
                )
                .map_err(|e| format!("scaling: {e}"))?;
                &mut slot.insert((key, scaler)).1
            }
        };
        let mut rgba = frame::Video::empty();
        scaler
            .run(decoded, &mut rgba)
            .map_err(|e| format!("scaling: {e}"))?;
        let pixels = packed_rows(&rgba, width, height, 4);
        Ok(rotate(width, height, &pixels, 4, self.rotation))
    }
}

impl AudioTrack {
    fn receive(&mut self, origin_ms: i64, skip_before: &mut Option<u32>) -> Result<(), String> {
        let mut decoded = frame::Audio::empty();
        while received(self.decoder.receive_frame(&mut decoded))? {
            let mut stereo = convert_audio_frame(&decoded)?;
            if let Some(target) = *skip_before {
                // Drops what starts before the seek's target, to the sample.
                let rate = i64::from(decoded.rate().max(1));
                let start_ms = decoded.timestamp().map_or(i64::from(target), |ts| {
                    millis(ts, self.time_base, origin_ms)
                });
                let skip = ((i64::from(target) - start_ms).max(0) * rate / 1_000) as usize * 2;
                if skip >= stereo.len() {
                    continue;
                }
                stereo.drain(..skip);
                *skip_before = None;
            }
            self.resampler.process(&stereo, &mut self.pending)?;
        }
        Ok(())
    }
}

fn open_video(stream: &ffmpeg::format::stream::Stream) -> Result<VideoTrack, ffmpeg::Error> {
    let context = ffmpeg::codec::context::Context::from_parameters(stream.parameters())?;
    let mut decoder = context.decoder();
    decoder.set_threading(ffmpeg::codec::threading::Config {
        kind: ffmpeg::codec::threading::Type::Frame,
        count: 0,
    });
    Ok(VideoTrack {
        index: stream.index(),
        decoder: decoder.video()?,
        time_base: stream.time_base(),
        rotation: stream_rotation(stream),
        scaler: None,
        queue: VecDeque::new(),
        drained: false,
    })
}

fn open_audio(stream: &ffmpeg::format::stream::Stream) -> Result<AudioTrack, String> {
    let context = ffmpeg::codec::context::Context::from_parameters(stream.parameters())
        .map_err(|e| e.to_string())?;
    let decoder = context.decoder().audio().map_err(|e| e.to_string())?;
    let resampler = StereoResampler::new(decoder.rate())?;
    Ok(AudioTrack {
        index: stream.index(),
        decoder,
        time_base: stream.time_base(),
        resampler,
        pending: VecDeque::new(),
        drained: false,
    })
}

/// The turn the stream's display matrix asks for, clockwise in degrees.
fn stream_rotation(stream: &ffmpeg::format::stream::Stream) -> i32 {
    for side_data in stream.side_data() {
        if side_data.kind() != ffmpeg::codec::packet::side_data::Type::DisplayMatrix {
            continue;
        }
        let data = side_data.data();
        if data.len() < 9 * size_of::<i32>() {
            return 0;
        }
        // SAFETY: a display matrix is nine i32s (checked above), in side
        // data FFmpeg allocated with av_malloc, so aligned for i32.
        let counter_clockwise =
            unsafe { ffmpeg::ffi::av_display_rotation_get(data.as_ptr().cast::<i32>()) };
        if counter_clockwise.is_nan() {
            return 0;
        }
        return (-counter_clockwise.round() as i32).rem_euclid(360);
    }
    0
}

/// The first plane of `picture` without the padding at the end of its rows.
fn packed_rows(picture: &frame::Video, width: u32, height: u32, bytes: usize) -> Vec<u8> {
    let stride = picture.stride(0);
    let row = width as usize * bytes;
    let data = picture.data(0);
    let mut out = Vec::with_capacity(row * height as usize);
    for y in 0..height as usize {
        out.extend_from_slice(&data[y * stride..y * stride + row]);
    }
    out
}

pub fn probe_and_poster(
    path: &Path,
    max_side: u32,
) -> Result<(Probe, Option<Vec<u8>>), PlayerError> {
    init();
    // Through our own reading, as the player does: the FFmpeg the app
    // carries has no protocols, not even `file`.
    let file = std::fs::File::open(path)?;
    let io = StreamIo::from_read_seek(file).map_err(|e| PlayerError::Open(e.to_string()))?;
    let mut input = ffmpeg::format::input_from_stream_with_interrupt(io, None, None, || false)
        .map_err(|e| PlayerError::Open(e.to_string()))?;
    let duration_ms = u32::try_from(input.duration().max(0) / 1_000).unwrap_or(u32::MAX);
    let Some(stream) = input.streams().best(ffmpeg::media::Type::Video) else {
        let probe = Probe {
            width: 0,
            height: 0,
            duration_ms,
            has_video: false,
        };
        return Ok((probe, None));
    };
    let index = stream.index();
    let rotation = stream_rotation(&stream);
    let mut decoder = ffmpeg::codec::context::Context::from_parameters(stream.parameters())
        .and_then(|context| context.decoder().video())
        .map_err(|e| PlayerError::Decode(e.to_string()))?;
    let (width, height) = if matches!(rotation, 90 | 270) {
        (decoder.height(), decoder.width())
    } else {
        (decoder.width(), decoder.height())
    };
    let probe = Probe {
        width,
        height,
        duration_ms,
        has_video: true,
    };
    let poster = first_frame(&mut input, index, &mut decoder)
        .map_err(PlayerError::Decode)?
        .map(|decoded| jpeg(&decoded, max_side, rotation))
        .transpose()?;
    Ok((probe, poster))
}

/// The stream's first picture, if it has one.
fn first_frame(
    input: &mut ffmpeg::format::context::Input,
    index: usize,
    decoder: &mut ffmpeg::decoder::Video,
) -> Result<Option<frame::Video>, String> {
    let mut decoded = frame::Video::empty();
    loop {
        let mut packet = Packet::empty();
        match packet.read(input) {
            Ok(()) if packet.stream() == index => {
                send(decoder, &packet)?;
                if received(decoder.receive_frame(&mut decoded))? {
                    return Ok(Some(decoded));
                }
            }
            Ok(()) | Err(ffmpeg::Error::InvalidData) => {}
            Err(ffmpeg::Error::Eof) => break,
            Err(error) => return Err(format!("reading: {error}")),
        }
    }
    decoder.send_eof().map_err(|e| e.to_string())?;
    Ok(received(decoder.receive_frame(&mut decoded))?.then_some(decoded))
}

/// `decoded`, at most `max_side` on its long side and upright, as a JPEG.
fn jpeg(decoded: &frame::Video, max_side: u32, rotation: i32) -> Result<Vec<u8>, PlayerError> {
    let (width, height) = target_video_size(decoded.width(), decoded.height(), max_side);
    let mut scaler = scaling::Context::get(
        decoded.format(),
        decoded.width(),
        decoded.height(),
        Pixel::RGB24,
        width,
        height,
        Flags::BILINEAR,
    )
    .map_err(|e| PlayerError::Decode(e.to_string()))?;
    let mut rgb = frame::Video::empty();
    scaler
        .run(decoded, &mut rgb)
        .map_err(|e| PlayerError::Decode(e.to_string()))?;
    let pixels = packed_rows(&rgb, width, height, 3);
    let (width, height, pixels) = rotate(width, height, &pixels, 3, rotation);
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 82)
        .encode(&pixels, width, height, image::ExtendedColorType::Rgb8)
        .map_err(|e| PlayerError::Decode(e.to_string()))?;
    Ok(bytes)
}
