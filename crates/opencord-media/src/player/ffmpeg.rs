use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Once};
use std::time::{Duration, Instant};

use ffmpeg_next as ffmpeg;
use ffmpeg_next::format::Pixel;
use ffmpeg_next::format::context::StreamIo;
use ffmpeg_next::software::scaling::flag::Flags;

use super::decode::{
    StereoResampler, convert_audio_frame, rotate_rgb, rotate_rgba, target_video_size,
};
use super::sync::{FrameAction, MediaClock};
use super::{AudioOut, MediaSource, PlayerError, PlayerEvent, Probe, VideoOut};

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

enum PlayerCommand {
    Play,
    Pause,
    Seek(u32),
    SetVolume(f32, bool),
    Close,
}

pub struct MediaPlayer {
    commands: Sender<PlayerCommand>,
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
        let (cmd_tx, cmd_rx) = mpsc::channel();

        let thread_cancelled = cancelled.clone();
        let thread = std::thread::Builder::new()
            .name("opencord-media-player".to_owned())
            .spawn(move || {
                run_player_thread(source, video, audio, events, cmd_rx, thread_cancelled);
            })
            .map_err(|e| PlayerError::Open(e.to_string()))?;

        Ok(Self {
            commands: cmd_tx,
            cancelled,
            thread: Some(thread),
        })
    }

    pub fn play(&self) {
        let _ = self.commands.send(PlayerCommand::Play);
    }

    pub fn pause(&self) {
        let _ = self.commands.send(PlayerCommand::Pause);
    }

    pub fn seek(&self, position_ms: u32) {
        let _ = self.commands.send(PlayerCommand::Seek(position_ms));
    }

    pub fn set_volume(&self, volume: f32, muted: bool) {
        let _ = self.commands.send(PlayerCommand::SetVolume(volume, muted));
    }

    pub fn close(mut self) {
        self.stop_and_join();
    }

    fn stop_and_join(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
        let _ = self.commands.send(PlayerCommand::Close);
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

fn get_stream_rotation(stream: &ffmpeg::format::stream::Stream) -> i32 {
    for sd in stream.side_data() {
        if sd.kind() == ffmpeg::codec::packet::side_data::Type::DisplayMatrix {
            let data = sd.data();
            if data.len() >= 36 {
                let matrix = data.as_ptr() as *const i32;
                unsafe extern "C" {
                    fn av_display_rotation_get(matrix: *const i32) -> f64;
                }
                let rot = -unsafe { av_display_rotation_get(matrix) };
                let rot_round = rot.round() as i32;
                return rot_round.rem_euclid(360);
            }
        }
    }
    0
}

fn run_player_thread(
    source: Box<dyn MediaSource>,
    video_out: Option<Arc<dyn VideoOut>>,
    mut audio_out: Box<dyn AudioOut>,
    mut events: impl FnMut(PlayerEvent),
    cmd_rx: Receiver<PlayerCommand>,
    cancelled: Arc<AtomicBool>,
) {
    let io = match StreamIo::from_read_seek(source) {
        Ok(io) => io,
        Err(e) => {
            events(PlayerEvent::Failed(e.to_string()));
            return;
        }
    };

    let cancel_check = cancelled.clone();
    let mut input =
        match ffmpeg::format::input_from_stream_with_interrupt(io, None, None, move || {
            cancel_check.load(Ordering::Relaxed)
        }) {
            Ok(input) => input,
            Err(e) => {
                events(PlayerEvent::Failed(e.to_string()));
                return;
            }
        };

    let duration_ms = if input.duration() > 0 {
        (input.duration() as f64 / ffmpeg::ffi::AV_TIME_BASE as f64 * 1000.0) as u32
    } else {
        0
    };

    let best_video_idx = input
        .streams()
        .best(ffmpeg::media::Type::Video)
        .map(|s| s.index());
    let best_audio_idx = input
        .streams()
        .best(ffmpeg::media::Type::Audio)
        .map(|s| s.index());

    let mut video_decoder = None;
    let mut video_time_base = ffmpeg::Rational(1, 1000);
    let mut video_rotation = 0;
    let mut opened_width = 0;
    let mut opened_height = 0;

    if let Some(idx) = best_video_idx
        && let Some(stream) = input.stream(idx)
    {
        video_time_base = stream.time_base();
        video_rotation = get_stream_rotation(&stream);
        if let Ok(ctx) = ffmpeg::codec::context::Context::from_parameters(stream.parameters())
            && let Ok(mut dec) = ctx.decoder().video()
        {
            dec.set_threading(ffmpeg::codec::threading::Config {
                kind: ffmpeg::codec::threading::Type::Frame,
                count: 0,
            });
            let (w, h) = (dec.width(), dec.height());
            let (eff_w, eff_h) = if video_rotation == 90 || video_rotation == 270 {
                (h, w)
            } else {
                (w, h)
            };
            opened_width = eff_w;
            opened_height = eff_h;
            video_decoder = Some(dec);
        }
    }

    let mut audio_decoder = None;
    let mut resampler = None;
    if let Some(idx) = best_audio_idx
        && let Some(stream) = input.stream(idx)
        && let Ok(ctx) = ffmpeg::codec::context::Context::from_parameters(stream.parameters())
        && let Ok(mut dec) = ctx.decoder().audio()
    {
        dec.set_threading(ffmpeg::codec::threading::Config {
            kind: ffmpeg::codec::threading::Type::Frame,
            count: 0,
        });
        let rate = dec.rate();
        match StereoResampler::new(rate) {
            Ok(r) => {
                resampler = Some(r);
                audio_decoder = Some(dec);
            }
            Err(err) => {
                events(PlayerEvent::Failed(err));
                return;
            }
        }
    }

    events(PlayerEvent::Opened {
        duration_ms,
        width: opened_width,
        height: opened_height,
        has_video: video_decoder.is_some(),
        has_audio: audio_decoder.is_some(),
    });

    let has_audio = audio_decoder.is_some();
    let mut clock = MediaClock::new(has_audio);
    let mut is_playing = false;
    let mut is_buffering = false;
    let mut volume = 1.0f32;
    let mut muted = false;
    let mut last_position_emit = Instant::now();

    let mut video_queue = VecDeque::<(u32, u32, u32, Vec<u8>)>::new();
    let mut audio_pending = VecDeque::<f32>::new();
    let mut scaler: Option<ffmpeg::software::scaling::context::Context> = None;
    let mut scaler_src_fmt = Pixel::None;
    let mut scaler_src_w = 0;
    let mut scaler_src_h = 0;
    let mut scaler_dst_w = 0;
    let mut scaler_dst_h = 0;

    let mut eof = false;
    let mut video_flushed = false;
    let mut audio_flushed = false;
    let mut seek_target: Option<u32> = None;

    let mut packets_iter = input.packets();

    while !cancelled.load(Ordering::Relaxed) {
        // 1. Process commands
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                PlayerCommand::Play => {
                    if !is_playing {
                        is_playing = true;
                        clock.play();
                    }
                }
                PlayerCommand::Pause => {
                    if is_playing {
                        is_playing = false;
                        clock.pause();
                    }
                }
                PlayerCommand::Seek(pos_ms) => {
                    events(PlayerEvent::Position {
                        position_ms: pos_ms,
                        buffering: true,
                    });

                    let ts = (pos_ms as f64 / 1000.0 * ffmpeg::ffi::AV_TIME_BASE as f64) as i64;
                    let _ = input.seek(ts, ..ts);

                    if let Some(dec) = &mut video_decoder {
                        dec.flush();
                    }
                    if let Some(dec) = &mut audio_decoder {
                        dec.flush();
                    }

                    video_queue.clear();
                    audio_pending.clear();
                    seek_target = Some(pos_ms);
                    clock.seek(pos_ms);

                    eof = false;
                    video_flushed = false;
                    audio_flushed = false;
                    packets_iter = input.packets();

                    is_buffering = false;
                    events(PlayerEvent::Position {
                        position_ms: pos_ms,
                        buffering: false,
                    });
                }
                PlayerCommand::SetVolume(vol, mut_flag) => {
                    volume = vol;
                    muted = mut_flag;
                }
                PlayerCommand::Close => {
                    return;
                }
            }
        }

        if !is_playing {
            std::thread::sleep(Duration::from_millis(10));
            continue;
        }

        // 2. Audio playback output (pace: keep buffer <= 60 ms ahead = 2880 frames)
        if has_audio {
            let queued = audio_out.queued_frames();
            if queued < 2880 && !audio_pending.is_empty() {
                let to_write_frames = (2880 - queued).min(audio_pending.len() / 2);
                if to_write_frames > 0 {
                    let to_write_samples = to_write_frames * 2;
                    let (slice1, slice2) = audio_pending.as_slices();
                    let chunk: Vec<f32> = if slice1.len() >= to_write_samples {
                        slice1[..to_write_samples].to_vec()
                    } else {
                        let mut v = slice1.to_vec();
                        v.extend_from_slice(&slice2[..to_write_samples - slice1.len()]);
                        v
                    };
                    let written = audio_out.write(&chunk);
                    audio_pending.drain(..written);
                    clock.on_audio_written(written);
                }
            }
        }

        // 3. Video frame presentation
        while let Some(front) = video_queue.front() {
            let queued_audio = audio_out.queued_frames();
            match clock.check_video_frame(front.0, queued_audio) {
                FrameAction::Present => {
                    let (_, w, h, rgba) = video_queue.pop_front().unwrap();
                    if let Some(vout) = &video_out {
                        vout.present(w, h, rgba);
                    }
                }
                FrameAction::Drop => {
                    video_queue.pop_front();
                }
                FrameAction::Wait(_) => {
                    break;
                }
            }
        }

        // 4. Demux & Decode when queues have room
        let video_needs_data = video_decoder.is_some() && video_queue.len() < 8;
        let audio_needs_data = has_audio && audio_pending.len() < 48000;

        if (video_needs_data || audio_needs_data) && !eof {
            if let Some((stream, packet)) = packets_iter.next() {
                let stream_idx = stream.index();
                if Some(stream_idx) == best_video_idx
                    && let Some(dec) = &mut video_decoder
                    && dec.send_packet(&packet).is_ok()
                {
                    let mut frame = ffmpeg::frame::Video::empty();
                    while dec.receive_frame(&mut frame).is_ok() {
                        let pts_ms = (frame.pts().unwrap_or(0) as f64
                            * f64::from(video_time_base)
                            * 1000.0) as u32;

                        if let Some(target) = seek_target {
                            if pts_ms < target {
                                continue;
                            } else {
                                seek_target = None;
                            }
                        }

                        let (target_w, target_h) =
                            target_video_size(frame.width(), frame.height(), 1920);

                        if (scaler.is_none()
                            || scaler_src_fmt != frame.format()
                            || scaler_src_w != frame.width()
                            || scaler_src_h != frame.height()
                            || scaler_dst_w != target_w
                            || scaler_dst_h != target_h)
                            && let Ok(sc) = ffmpeg::software::scaling::context::Context::get(
                                frame.format(),
                                frame.width(),
                                frame.height(),
                                Pixel::RGBA,
                                target_w,
                                target_h,
                                Flags::BILINEAR,
                            )
                        {
                            scaler = Some(sc);
                            scaler_src_fmt = frame.format();
                            scaler_src_w = frame.width();
                            scaler_src_h = frame.height();
                            scaler_dst_w = target_w;
                            scaler_dst_h = target_h;
                        }

                        if let Some(sc) = &mut scaler {
                            let mut rgba_frame = ffmpeg::frame::Video::empty();
                            if sc.run(&frame, &mut rgba_frame).is_ok() {
                                let stride = rgba_frame.stride(0);
                                let row_bytes = (target_w * 4) as usize;
                                let mut raw_rgba = vec![0u8; row_bytes * target_h as usize];
                                let data = rgba_frame.data(0);
                                for y in 0..target_h as usize {
                                    let src_row = &data[y * stride..y * stride + row_bytes];
                                    let dst_row = &mut raw_rgba[y * row_bytes..(y + 1) * row_bytes];
                                    dst_row.copy_from_slice(src_row);
                                }

                                let (final_w, final_h, final_rgba) =
                                    rotate_rgba(target_w, target_h, &raw_rgba, video_rotation);
                                video_queue.push_back((pts_ms, final_w, final_h, final_rgba));
                            }
                        }
                    }
                } else if Some(stream_idx) == best_audio_idx
                    && let (Some(dec), Some(resamp)) = (&mut audio_decoder, &mut resampler)
                    && dec.send_packet(&packet).is_ok()
                {
                    let mut frame = ffmpeg::frame::Audio::empty();
                    while dec.receive_frame(&mut frame).is_ok() {
                        let stereo = convert_audio_frame(&frame);
                        let mut q = VecDeque::new();
                        resamp.process(&stereo, &mut q);
                        for s in q {
                            let val = if muted { 0.0 } else { s * volume };
                            audio_pending.push_back(val);
                        }
                    }
                }
            } else {
                eof = true;
                if let Some(dec) = &mut video_decoder {
                    let _ = dec.send_eof();
                    let mut frame = ffmpeg::frame::Video::empty();
                    while dec.receive_frame(&mut frame).is_ok() {
                        let pts_ms = (frame.pts().unwrap_or(0) as f64
                            * f64::from(video_time_base)
                            * 1000.0) as u32;
                        let (target_w, target_h) =
                            target_video_size(frame.width(), frame.height(), 1920);
                        if let Some(sc) = &mut scaler {
                            let mut rgba_frame = ffmpeg::frame::Video::empty();
                            if sc.run(&frame, &mut rgba_frame).is_ok() {
                                let stride = rgba_frame.stride(0);
                                let row_bytes = (target_w * 4) as usize;
                                let mut raw_rgba = vec![0u8; row_bytes * target_h as usize];
                                let data = rgba_frame.data(0);
                                for y in 0..target_h as usize {
                                    let src_row = &data[y * stride..y * stride + row_bytes];
                                    let dst_row = &mut raw_rgba[y * row_bytes..(y + 1) * row_bytes];
                                    dst_row.copy_from_slice(src_row);
                                }
                                let (final_w, final_h, final_rgba) =
                                    rotate_rgba(target_w, target_h, &raw_rgba, video_rotation);
                                video_queue.push_back((pts_ms, final_w, final_h, final_rgba));
                            }
                        }
                    }
                    video_flushed = true;
                }
                if let (Some(dec), Some(resamp)) = (&mut audio_decoder, &mut resampler) {
                    let _ = dec.send_eof();
                    let mut frame = ffmpeg::frame::Audio::empty();
                    while dec.receive_frame(&mut frame).is_ok() {
                        let stereo = convert_audio_frame(&frame);
                        let mut q = VecDeque::new();
                        resamp.process(&stereo, &mut q);
                        resamp.flush(&mut q);
                        for s in q {
                            let val = if muted { 0.0 } else { s * volume };
                            audio_pending.push_back(val);
                        }
                    }
                    audio_flushed = true;
                }
            }
        }

        // 5. Check if playback ended
        let video_done =
            video_decoder.is_none() || (eof && video_flushed && video_queue.is_empty());
        let audio_done = !has_audio
            || (eof && audio_flushed && audio_pending.is_empty() && audio_out.queued_frames() == 0);

        if eof && video_done && audio_done {
            events(PlayerEvent::Ended);
            is_playing = false;
        }

        // 6. Emit periodic position
        if is_playing && last_position_emit.elapsed() >= Duration::from_millis(50) {
            let pos = clock.current_time_ms(audio_out.queued_frames());
            events(PlayerEvent::Position {
                position_ms: pos,
                buffering: is_buffering,
            });
            last_position_emit = Instant::now();
        }

        std::thread::sleep(Duration::from_millis(5));
    }
}

pub fn probe_and_poster(
    path: &Path,
    max_side: u32,
) -> Result<(Probe, Option<Vec<u8>>), PlayerError> {
    init();

    let mut input = ffmpeg::format::input(path).map_err(|e| PlayerError::Open(e.to_string()))?;
    let duration_ms = if input.duration() > 0 {
        (input.duration() as f64 / ffmpeg::ffi::AV_TIME_BASE as f64 * 1000.0) as u32
    } else {
        0
    };

    let video_info = if let Some(stream) = input.streams().best(ffmpeg::media::Type::Video) {
        let rotation = get_stream_rotation(&stream);
        let decoder_ctx = ffmpeg::codec::context::Context::from_parameters(stream.parameters())
            .map_err(|e| PlayerError::Decode(e.to_string()))?;
        let video_decoder = decoder_ctx
            .decoder()
            .video()
            .map_err(|e| PlayerError::Decode(e.to_string()))?;

        let orig_w = video_decoder.width();
        let orig_h = video_decoder.height();
        let (eff_w, eff_h) = if rotation == 90 || rotation == 270 {
            (orig_h, orig_w)
        } else {
            (orig_w, orig_h)
        };
        Some((video_decoder, stream.index(), rotation, eff_w, eff_h))
    } else {
        None
    };

    if let Some((mut video_decoder, stream_idx, rotation, eff_w, eff_h)) = video_info {
        let probe = Probe {
            width: eff_w,
            height: eff_h,
            duration_ms,
            has_video: true,
        };

        let poster = decode_first_frame_as_jpeg(
            &mut input,
            stream_idx,
            &mut video_decoder,
            max_side,
            rotation,
        )?;

        Ok((probe, poster))
    } else {
        let probe = Probe {
            width: 0,
            height: 0,
            duration_ms,
            has_video: false,
        };
        Ok((probe, None))
    }
}

fn decode_first_frame_as_jpeg(
    input: &mut ffmpeg::format::context::Input,
    video_idx: usize,
    video_decoder: &mut ffmpeg::decoder::Video,
    max_side: u32,
    rotation: i32,
) -> Result<Option<Vec<u8>>, PlayerError> {
    for (stream, packet) in input.packets() {
        if stream.index() == video_idx && video_decoder.send_packet(&packet).is_ok() {
            let mut frame = ffmpeg::frame::Video::empty();
            if video_decoder.receive_frame(&mut frame).is_ok() {
                let (target_w, target_h) =
                    target_video_size(frame.width(), frame.height(), max_side);
                let mut scaler = ffmpeg::software::scaling::context::Context::get(
                    frame.format(),
                    frame.width(),
                    frame.height(),
                    Pixel::RGB24,
                    target_w,
                    target_h,
                    Flags::BILINEAR,
                )
                .map_err(|e| PlayerError::Decode(e.to_string()))?;

                let mut rgb_frame = ffmpeg::frame::Video::empty();
                scaler
                    .run(&frame, &mut rgb_frame)
                    .map_err(|e| PlayerError::Decode(e.to_string()))?;

                let stride = rgb_frame.stride(0);
                let row_bytes = (target_w * 3) as usize;
                let mut raw_rgb = vec![0u8; row_bytes * target_h as usize];
                let data = rgb_frame.data(0);
                for y in 0..target_h as usize {
                    let src_row = &data[y * stride..y * stride + row_bytes];
                    let dst_row = &mut raw_rgb[y * row_bytes..(y + 1) * row_bytes];
                    dst_row.copy_from_slice(src_row);
                }

                let (final_w, final_h, final_rgb) =
                    rotate_rgb(target_w, target_h, &raw_rgb, rotation);

                let mut jpeg_bytes = Vec::new();
                let mut encoder =
                    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg_bytes, 82);
                encoder
                    .encode(&final_rgb, final_w, final_h, image::ExtendedColorType::Rgb8)
                    .map_err(|e| PlayerError::Decode(e.to_string()))?;

                return Ok(Some(jpeg_bytes));
            }
        }
    }

    Ok(None)
}
