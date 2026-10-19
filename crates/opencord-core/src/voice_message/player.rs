//! Voice message player (Milestone A6, Task 29): decodes Ogg Opus, seeks,
//! applies speed stretching via wsola, and plays locally.

use std::num::{NonZeroU16, NonZeroU32};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use opencord_common::sound::ogg::OpusFile;
use opencord_media::audio::SAMPLE_RATE;
use opencord_media::audio::convert::ToDevice;
use opencord_media::audio::device::Speaker;
use opencord_media::audio::engine::AudioEngine;
use tokio::sync::mpsc;
use wsola::TimeStretch;

use crate::api::types::{AudioSettings, CoreError, MediaEvent, PlaybackItem};
use crate::now_playing::{NowPlaying, Stoppable};

pub(crate) enum PlayerCommand {
    Pause,
    Resume,
    Seek(u32),
    SetSpeed(f32),
    Stop,
}

pub(crate) struct VoicePlayerHandle {
    commands: mpsc::UnboundedSender<PlayerCommand>,
}

impl Stoppable for VoicePlayerHandle {
    fn stop(&self) {
        let _ = self.commands.send(PlayerCommand::Stop);
    }
}

#[derive(Default)]
pub(crate) struct VoicePlayer {
    commands: Mutex<Option<mpsc::UnboundedSender<PlayerCommand>>>,
}

impl VoicePlayer {
    #[allow(clippy::too_many_arguments)]
    pub fn play(
        &self,
        item: PlaybackItem,
        bytes: Vec<u8>,
        position_ms: u32,
        speed: f32,
        engine: Option<Arc<AudioEngine>>,
        settings: AudioSettings,
        events: mpsc::UnboundedSender<MediaEvent>,
        open_devices: bool,
        now_playing: &NowPlaying,
    ) -> Result<(), CoreError> {
        if speed != 1.0 && speed != 1.5 && speed != 2.0 {
            return Err(CoreError::InvalidInput {
                message: "speed must be 1.0, 1.5, or 2.0".to_owned(),
            });
        }

        let file = OpusFile::read(&bytes).map_err(|e| CoreError::InvalidInput {
            message: e.to_string(),
        })?;

        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        now_playing.replace(Box::new(VoicePlayerHandle {
            commands: cmd_tx.clone(),
        }));
        *self.commands.lock().unwrap_or_else(PoisonError::into_inner) = Some(cmd_tx);

        std::thread::Builder::new()
            .name("opencord-voice-player".to_owned())
            .spawn(move || {
                run_player(
                    file,
                    item,
                    position_ms,
                    speed,
                    engine,
                    settings,
                    events,
                    open_devices,
                    cmd_rx,
                );
            })
            .map_err(|e| CoreError::InvalidInput {
                message: format!("could not spawn voice player thread: {e}"),
            })?;

        Ok(())
    }

    pub fn pause(&self) {
        if let Some(cmd) = self
            .commands
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            let _ = cmd.send(PlayerCommand::Pause);
        }
    }

    pub fn resume(&self) {
        if let Some(cmd) = self
            .commands
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            let _ = cmd.send(PlayerCommand::Resume);
        }
    }

    pub fn seek(&self, position_ms: u32) {
        if let Some(cmd) = self
            .commands
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            let _ = cmd.send(PlayerCommand::Seek(position_ms));
        }
    }

    pub fn set_speed(&self, speed: f32) -> Result<(), CoreError> {
        if speed != 1.0 && speed != 1.5 && speed != 2.0 {
            return Err(CoreError::InvalidInput {
                message: "speed must be 1.0, 1.5, or 2.0".to_owned(),
            });
        }
        if let Some(cmd) = self
            .commands
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            let _ = cmd.send(PlayerCommand::SetSpeed(speed));
        }
        Ok(())
    }

    pub fn stop(&self) {
        if let Some(cmd) = self
            .commands
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            let _ = cmd.send(PlayerCommand::Stop);
        }
    }
}

enum OutputSink {
    Call(rtrb::Producer<f32>),
    Device {
        speaker: Speaker,
        to_device: Box<ToDevice>,
        volume: f32,
    },
    Null,
}

#[allow(clippy::too_many_arguments)]
fn perform_seek(
    file: &OpusFile,
    packet_starts: &[usize],
    duration_ms: u32,
    pre_skip: usize,
    target_ms: u32,
    speed: f32,
    pcm_buf: &mut [f32],
    stretch: &mut TimeStretch,
    decoder: &mut opus::Decoder,
) -> (usize, usize, u32) {
    let target_ms = target_ms.min(duration_ms);
    let target_source_sample = (target_ms as usize) * 48;
    let target_stream_sample = pre_skip + target_source_sample;
    let warmup_stream_sample = target_stream_sample.saturating_sub(80 * 48);

    let mut next_pkt_idx = packet_starts
        .iter()
        .rposition(|&s| s <= warmup_stream_sample)
        .unwrap_or(0);

    if let Ok(fresh) = opus::Decoder::new(SAMPLE_RATE, opus::Channels::Mono) {
        *decoder = fresh;
    }
    stretch.reset();
    stretch.set_tempo(speed);

    // Decode warmup packets until target_stream_sample
    while next_pkt_idx < file.packets.len() {
        let pkt = &file.packets[next_pkt_idx];
        let pkt_start = packet_starts[next_pkt_idx];
        let count = decoder.decode_float(pkt, pcm_buf, false).unwrap_or(0);
        next_pkt_idx += 1;

        if pkt_start + count <= target_stream_sample {
            continue;
        }

        let skip = target_stream_sample.saturating_sub(pkt_start);
        if skip < count {
            stretch.push(&pcm_buf[skip..count]);
        }
        break;
    }

    (next_pkt_idx, target_source_sample, target_ms)
}

#[allow(clippy::too_many_arguments)]
fn run_player(
    file: OpusFile,
    item: PlaybackItem,
    position_ms: u32,
    initial_speed: f32,
    engine: Option<Arc<AudioEngine>>,
    settings: AudioSettings,
    events: mpsc::UnboundedSender<MediaEvent>,
    open_devices: bool,
    mut commands: mpsc::UnboundedReceiver<PlayerCommand>,
) {
    let duration_ms = file.info.duration_ms;
    let pre_skip = file.info.pre_skip as usize;

    let mut packet_starts = Vec::with_capacity(file.packets.len());
    let mut total_samples = 0usize;
    for pkt in &file.packets {
        let n = opus::packet::get_nb_samples(pkt, SAMPLE_RATE).unwrap_or(960);
        packet_starts.push(total_samples);
        total_samples += n;
    }

    let rate = NonZeroU32::new(SAMPLE_RATE).unwrap();
    let channels = NonZeroU16::new(1).unwrap();
    let mut stretch = match TimeStretch::new(rate, channels) {
        Ok(s) => s,
        Err(e) => {
            let _ = events.send(MediaEvent::PlaybackFailed {
                item,
                message: e.to_string(),
            });
            return;
        }
    };
    stretch.set_tempo(initial_speed);

    let mut output = if let Some(engine) = &engine {
        let (producer, consumer) = rtrb::RingBuffer::new(SAMPLE_RATE as usize * 2);
        engine.set_local_media(Some(consumer));
        OutputSink::Call(producer)
    } else if open_devices {
        match Speaker::open(settings.output_device.as_deref()) {
            Ok(speaker) => match ToDevice::new(speaker.rate, speaker.channels) {
                Ok(to_device) => OutputSink::Device {
                    speaker,
                    to_device: Box::new(to_device),
                    volume: settings.output_volume,
                },
                Err(_) => OutputSink::Null,
            },
            Err(_) => OutputSink::Null,
        }
    } else {
        OutputSink::Null
    };

    let mut current_speed = initial_speed;
    let mut is_playing = true;
    let mut pcm_buf = vec![0.0f32; 5760];
    let mut decoder = match opus::Decoder::new(SAMPLE_RATE, opus::Channels::Mono) {
        Ok(d) => d,
        Err(e) => {
            let _ = events.send(MediaEvent::PlaybackFailed {
                item,
                message: e.to_string(),
            });
            return;
        }
    };

    let (mut next_packet_idx, mut current_source_sample, mut current_position_ms) = perform_seek(
        &file,
        &packet_starts,
        duration_ms,
        pre_skip,
        position_ms,
        current_speed,
        &mut pcm_buf,
        &mut stretch,
        &mut decoder,
    );

    let mut last_update_time = Instant::now();
    let mut flushed = false;
    let mut pending_samples = Vec::new();

    let _ = events.send(MediaEvent::PlaybackUpdate {
        item: item.clone(),
        position_ms: current_position_ms,
        duration_ms,
        playing: is_playing,
        buffering: false,
    });

    'main_loop: loop {
        // 1. Process commands
        while let Ok(cmd) = commands.try_recv() {
            match cmd {
                PlayerCommand::Pause => {
                    if is_playing {
                        is_playing = false;
                        let _ = events.send(MediaEvent::PlaybackUpdate {
                            item: item.clone(),
                            position_ms: current_position_ms.min(duration_ms),
                            duration_ms,
                            playing: false,
                            buffering: false,
                        });
                    }
                }
                PlayerCommand::Resume => {
                    if !is_playing {
                        is_playing = true;
                        let _ = events.send(MediaEvent::PlaybackUpdate {
                            item: item.clone(),
                            position_ms: current_position_ms.min(duration_ms),
                            duration_ms,
                            playing: true,
                            buffering: false,
                        });
                    }
                }
                PlayerCommand::SetSpeed(speed) => {
                    current_speed = speed;
                    stretch.set_tempo(speed);
                    let _ = events.send(MediaEvent::PlaybackUpdate {
                        item: item.clone(),
                        position_ms: current_position_ms.min(duration_ms),
                        duration_ms,
                        playing: is_playing,
                        buffering: false,
                    });
                }
                PlayerCommand::Seek(target_ms) => {
                    let (next_idx, cur_src, cur_pos) = perform_seek(
                        &file,
                        &packet_starts,
                        duration_ms,
                        pre_skip,
                        target_ms,
                        current_speed,
                        &mut pcm_buf,
                        &mut stretch,
                        &mut decoder,
                    );
                    next_packet_idx = next_idx;
                    current_source_sample = cur_src;
                    current_position_ms = cur_pos;
                    flushed = false;
                    pending_samples.clear();
                    let _ = events.send(MediaEvent::PlaybackUpdate {
                        item: item.clone(),
                        position_ms: current_position_ms,
                        duration_ms,
                        playing: is_playing,
                        buffering: false,
                    });
                }
                PlayerCommand::Stop => {
                    break 'main_loop;
                }
            }
        }

        if !is_playing {
            std::thread::sleep(Duration::from_millis(10));
            continue;
        }

        // 2. Decode next packet if needed
        while pending_samples.len() < 960 && next_packet_idx < file.packets.len() {
            let pkt = &file.packets[next_packet_idx];
            next_packet_idx += 1;
            if let Ok(count) = decoder.decode_float(pkt, &mut pcm_buf, false) {
                stretch.push(&pcm_buf[..count]);
            }
        }

        // 3. Pull stretched samples
        for s in stretch.pull(960) {
            pending_samples.push(s);
        }

        if next_packet_idx >= file.packets.len() && !flushed {
            flushed = true;
            for s in stretch.flush() {
                pending_samples.push(s);
            }
        }

        // 4. Output samples in 480-sample ticks (10 ms)
        if !pending_samples.is_empty() {
            let take = pending_samples.len().min(480);
            let tick: Vec<f32> = pending_samples.drain(..take).collect();

            match &mut output {
                OutputSink::Call(producer) => {
                    for &s in &tick {
                        let _ = producer.push(s);
                        let _ = producer.push(s);
                    }
                }
                OutputSink::Device {
                    speaker,
                    to_device,
                    volume,
                } => {
                    let mut stereo = Vec::with_capacity(tick.len() * 2);
                    for &s in &tick {
                        stereo.push(s);
                        stereo.push(s);
                    }
                    if stereo.len() == 960 {
                        to_device.push_tick(&stereo);
                        let mut dev_buf = vec![0.0f32; 1024];
                        let frames = to_device.pop(&mut dev_buf);
                        let vol = *volume;
                        for &s in &dev_buf[..frames * usize::from(speaker.channels)] {
                            let _ = speaker.samples.push(s * vol);
                        }
                    }
                }
                OutputSink::Null => {
                    let ms = (take as f32 / 48.0) as u64;
                    std::thread::sleep(Duration::from_millis(ms.max(1)));
                }
            }

            let source_advanced = (take as f32 * current_speed) as usize;
            current_source_sample += source_advanced;
            current_position_ms = (current_source_sample / 48) as u32;

            if last_update_time.elapsed() >= Duration::from_millis(50) {
                let _ = events.send(MediaEvent::PlaybackUpdate {
                    item: item.clone(),
                    position_ms: current_position_ms.min(duration_ms),
                    duration_ms,
                    playing: true,
                    buffering: false,
                });
                last_update_time = Instant::now();
            }
        } else if next_packet_idx >= file.packets.len() {
            // Playback ended
            let _ = events.send(MediaEvent::PlaybackUpdate {
                item: item.clone(),
                position_ms: duration_ms,
                duration_ms,
                playing: false,
                buffering: false,
            });
            let _ = events.send(MediaEvent::PlaybackEnded { item: item.clone() });
            break 'main_loop;
        } else {
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    if let Some(engine) = &engine {
        engine.set_local_media(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct DummyStoppable(Arc<AtomicBool>);
    impl Stoppable for DummyStoppable {
        fn stop(&self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }

    fn generate_sine_ogg(seconds: f32) -> Vec<u8> {
        let samples = (seconds * 48000.0) as usize;
        let mut encoder =
            opus::Encoder::new(SAMPLE_RATE, opus::Channels::Mono, opus::Application::Voip).unwrap();
        encoder.set_bitrate(opus::Bitrate::Bits(32_000)).unwrap();
        let pre_skip = u16::try_from(encoder.get_lookahead().unwrap_or(0)).unwrap_or(0);

        let pcm: Vec<f32> = (0..samples)
            .map(|i| {
                let t = i as f32 / 48000.0;
                0.5 * (2.0 * PI * 440.0 * t).sin()
            })
            .collect();

        let mut packets = Vec::new();
        let mut packet_buf = vec![0u8; 1500];
        for chunk in pcm.chunks(960) {
            let mut frame = [0.0f32; 960];
            frame[..chunk.len()].copy_from_slice(chunk);
            let size = encoder.encode_float(&frame, &mut packet_buf).unwrap();
            packets.push(packet_buf[..size].to_vec());
        }

        let pad_size = encoder
            .encode_float(&[0.0f32; 960], &mut packet_buf)
            .unwrap();
        packets.push(packet_buf[..pad_size].to_vec());

        opencord_common::sound::ogg::write(1, pre_skip, &packets, samples as u64)
    }

    #[test]
    fn tempo_two_halves_the_output() {
        let rate = NonZeroU32::new(48_000).unwrap();
        let channels = NonZeroU16::new(1).unwrap();
        let mut ts = TimeStretch::new(rate, channels).unwrap();
        ts.set_tempo(2.0);

        let input = vec![0.5f32; 48_000];
        ts.push(&input);

        let mut output: Vec<f32> = ts.pull(usize::MAX).collect();
        output.extend(ts.flush());
        let len = output.len() as f32;
        assert!(
            (len - 24_000.0).abs() < 2400.0,
            "output length {len} not close to 24000"
        );
    }

    #[test]
    fn starting_another_item_stops_the_first() {
        let now_playing = NowPlaying::default();
        let item1 = Arc::new(AtomicBool::new(false));
        let item2 = Arc::new(AtomicBool::new(false));

        now_playing.replace(Box::new(DummyStoppable(item1.clone())));
        assert!(!item1.load(Ordering::Relaxed));

        now_playing.replace(Box::new(DummyStoppable(item2.clone())));
        assert!(item1.load(Ordering::Relaxed));
        assert!(!item2.load(Ordering::Relaxed));

        now_playing.clear();
        assert!(item2.load(Ordering::Relaxed));
    }

    #[test]
    fn seek_lands_within_a_frame() {
        let bytes = generate_sine_ogg(3.0);
        let player = VoicePlayer::default();
        let now_playing = NowPlaying::default();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let item = PlaybackItem::Draft { draft_id: 1 };

        player
            .play(
                item.clone(),
                bytes,
                1500,
                1.0,
                None,
                AudioSettings::default(),
                tx,
                false,
                &now_playing,
            )
            .expect("play failed");

        let first_event = rx.blocking_recv().expect("no event received");
        player.stop();

        match first_event {
            MediaEvent::PlaybackUpdate { position_ms, .. } => {
                let diff = (position_ms as i64 - 1500).abs();
                assert!(
                    diff <= 20,
                    "first update was at {position_ms} ms, expected 1500 ± 20 ms"
                );
            }
            other => panic!("expected PlaybackUpdate, got {other:?}"),
        }
    }

    #[test]
    fn a_draft_plays_without_network() {
        let bytes = generate_sine_ogg(1.0);
        let player = VoicePlayer::default();
        let now_playing = NowPlaying::default();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let item = PlaybackItem::Draft { draft_id: 42 };

        player
            .play(
                item.clone(),
                bytes,
                0,
                1.0,
                None,
                AudioSettings::default(),
                tx,
                false,
                &now_playing,
            )
            .expect("play failed");

        let first_event = rx.blocking_recv().expect("no event received");
        player.stop();

        match first_event {
            MediaEvent::PlaybackUpdate {
                position_ms,
                playing,
                ..
            } => {
                assert_eq!(position_ms, 0);
                assert!(playing);
            }
            other => panic!("expected PlaybackUpdate, got {other:?}"),
        }
    }
}
