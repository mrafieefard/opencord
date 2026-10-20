//! Voice message player (Milestone A6, Task 29): decodes Ogg Opus, seeks,
//! applies speed stretching via wsola, and plays locally.

use std::num::{NonZeroU16, NonZeroU32};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use opencord_common::sound::ogg::OpusFile;
use opencord_media::audio::engine::AudioEngine;
use opencord_media::audio::{SAMPLE_RATE, TICK};
use tokio::sync::mpsc;
use wsola::TimeStretch;

use crate::api::types::{AudioSettings, CoreError, MediaEvent, PlaybackItem};
use crate::now_playing::{NowPlaying, Player, Stoppable};
use crate::output_sink::{AHEAD, CallMix, OutputSink};

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
        now_playing.replace(
            Player::Voice,
            Box::new(VoicePlayerHandle {
                commands: cmd_tx.clone(),
            }),
        );
        *self.commands.lock().unwrap_or_else(PoisonError::into_inner) = Some(cmd_tx);

        std::thread::Builder::new()
            .name("opencord-voice-player".to_owned())
            .spawn(move || {
                run_player(
                    file,
                    item,
                    position_ms,
                    speed,
                    || {
                        let call = engine.map(|e| e as Arc<dyn CallMix>);
                        OutputSink::open(call, &settings, open_devices)
                    },
                    events,
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

/// Decoding state between seeks: the next packet to decode, and the source
/// position of the next sample the listener hears.
struct Cursor {
    next_packet: usize,
    source_sample: usize,
}

/// Restarts decoding so the next stretched sample is at `target_ms`. Decodes
/// from 80 ms earlier, so the decoder has settled when the sound starts.
fn seek(
    file: &OpusFile,
    packet_starts: &[usize],
    pre_skip: usize,
    target_ms: u32,
    stretch: &mut TimeStretch,
    decoder: &mut opus::Decoder,
    pcm: &mut [f32],
) -> Result<Cursor, CoreError> {
    let target = pre_skip + target_ms.min(file.info.duration_ms) as usize * 48;
    let warmup = target.saturating_sub(80 * 48);
    let mut next_packet = packet_starts
        .iter()
        .rposition(|&start| start <= warmup)
        .unwrap_or(0);
    decoder.reset_state().map_err(decoding)?;
    stretch.reset();
    while next_packet < file.packets.len() {
        let start = packet_starts[next_packet];
        let count = decoder
            .decode_float(&file.packets[next_packet], pcm, false)
            .map_err(decoding)?;
        next_packet += 1;
        if start + count > target {
            stretch.push(&pcm[target.saturating_sub(start)..count]);
            break;
        }
    }
    Ok(Cursor {
        next_packet,
        source_sample: target - pre_skip,
    })
}

fn decoding(error: impl std::fmt::Display) -> CoreError {
    CoreError::InvalidInput {
        message: format!("The voice message couldn't be played: {error}"),
    }
}

/// Plays `file` until it ends or is stopped, answering commands as it goes.
/// Errors end playback with a `PlaybackFailed` event.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_player(
    file: OpusFile,
    item: PlaybackItem,
    position_ms: u32,
    speed: f32,
    open_output: impl FnOnce() -> Result<OutputSink, CoreError>,
    events: mpsc::UnboundedSender<MediaEvent>,
    commands: mpsc::UnboundedReceiver<PlayerCommand>,
) {
    if let Err(error) = play_file(
        &file,
        &item,
        position_ms,
        speed,
        open_output,
        &events,
        commands,
    ) {
        let _ = events.send(MediaEvent::PlaybackFailed {
            item,
            message: error.to_string(),
        });
    }
}

fn play_file(
    file: &OpusFile,
    item: &PlaybackItem,
    position_ms: u32,
    mut speed: f32,
    open_output: impl FnOnce() -> Result<OutputSink, CoreError>,
    events: &mpsc::UnboundedSender<MediaEvent>,
    mut commands: mpsc::UnboundedReceiver<PlayerCommand>,
) -> Result<(), CoreError> {
    let duration_ms = file.info.duration_ms;
    let pre_skip = usize::from(file.info.pre_skip);
    let mut packet_starts = Vec::with_capacity(file.packets.len());
    let mut total = 0;
    for packet in &file.packets {
        packet_starts.push(total);
        total += opus::packet::get_nb_samples(packet, SAMPLE_RATE).map_err(decoding)?;
    }

    let mut stretch = TimeStretch::new(
        const { NonZeroU32::new(SAMPLE_RATE).unwrap() },
        const { NonZeroU16::new(1).unwrap() },
    )
    .map_err(decoding)?;
    stretch.set_tempo(speed);
    let mut decoder = opus::Decoder::new(SAMPLE_RATE, opus::Channels::Mono).map_err(decoding)?;
    let mut pcm = vec![0.0f32; 5760];
    let mut cursor = seek(
        file,
        &packet_starts,
        pre_skip,
        position_ms,
        &mut stretch,
        &mut decoder,
        &mut pcm,
    )?;
    let mut output = open_output()?;

    let mut playing = true;
    let mut flushed = false;
    let mut pending: Vec<f32> = Vec::with_capacity(2 * TICK);
    let mut last_update = Instant::now();
    let update = |position: usize, playing: bool| MediaEvent::PlaybackUpdate {
        item: item.clone(),
        position_ms: ((position / 48) as u32).min(duration_ms),
        duration_ms,
        playing,
        buffering: false,
    };
    let _ = events.send(update(cursor.source_sample, playing));

    loop {
        while let Ok(command) = commands.try_recv() {
            match command {
                PlayerCommand::Pause => playing = false,
                PlayerCommand::Resume => playing = true,
                PlayerCommand::SetSpeed(new) => {
                    speed = new;
                    stretch.set_tempo(new);
                }
                PlayerCommand::Seek(target_ms) => {
                    cursor = seek(
                        file,
                        &packet_starts,
                        pre_skip,
                        target_ms,
                        &mut stretch,
                        &mut decoder,
                        &mut pcm,
                    )?;
                    flushed = false;
                    pending.clear();
                }
                PlayerCommand::Stop => return Ok(()),
            }
            let _ = events.send(update(cursor.source_sample, playing));
        }
        if !playing || !output.has_room() {
            std::thread::sleep(Duration::from_millis(5));
            continue;
        }

        // One tick of stretched sound, decoding a packet at a time, so a
        // new speed applies from the next packet on. After `flush` the
        // stretcher is never pulled again until a seek resets it: it would
        // replay the input it still holds.
        while pending.len() < TICK && !flushed {
            pending.extend(stretch.pull(TICK - pending.len()));
            if pending.len() == TICK {
                break;
            }
            if let Some(packet) = file.packets.get(cursor.next_packet) {
                let count = decoder
                    .decode_float(packet, &mut pcm, false)
                    .map_err(decoding)?;
                stretch.push(&pcm[..count]);
                cursor.next_packet += 1;
            } else {
                flushed = true;
                pending.extend(stretch.flush());
            }
        }

        if pending.is_empty() {
            // Let what is queued be heard before saying it ended.
            std::thread::sleep(AHEAD);
            let _ = events.send(update(duration_ms as usize * 48, false));
            let _ = events.send(MediaEvent::PlaybackEnded { item: item.clone() });
            return Ok(());
        }
        let take = pending.len().min(TICK);
        output.write(&pending[..take]);
        pending.drain(..take);
        cursor.source_sample += (take as f32 * speed) as usize;
        if last_update.elapsed() >= Duration::from_millis(50) {
            let _ = events.send(update(cursor.source_sample, true));
            last_update = Instant::now();
        }
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

        now_playing.replace(Player::Voice, Box::new(DummyStoppable(item1.clone())));
        assert!(!item1.load(Ordering::Relaxed));

        now_playing.replace(Player::Media, Box::new(DummyStoppable(item2.clone())));
        assert!(item1.load(Ordering::Relaxed));
        assert!(!item2.load(Ordering::Relaxed));

        now_playing.clear(Player::Media);
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

    /// Pops the call's side of `consumer` the way the engine does, one tick
    /// every 10 ms, until playback ends; returns the frames heard and how
    /// long it took.
    fn hear_in_real_time(
        mut consumer: rtrb::Consumer<f32>,
        events: &mut mpsc::UnboundedReceiver<MediaEvent>,
    ) -> (usize, Duration) {
        let started = Instant::now();
        let mut samples = 0;
        while started.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(10));
            for _ in 0..2 * TICK {
                if consumer.pop().is_err() {
                    break;
                }
                samples += 1;
            }
            while let Ok(event) = events.try_recv() {
                if matches!(event, MediaEvent::PlaybackEnded { .. }) {
                    return (samples / 2, started.elapsed());
                }
                assert!(
                    !matches!(event, MediaEvent::PlaybackFailed { .. }),
                    "{event:?}"
                );
            }
        }
        panic!("playback never ended");
    }

    /// A call that hands the test whatever the player attaches.
    struct FakeCall(std::sync::Mutex<std::sync::mpsc::Sender<rtrb::Consumer<f32>>>);

    impl CallMix for FakeCall {
        fn attach(&self, source: rtrb::Consumer<f32>) -> u64 {
            let _ = self.0.lock().unwrap().send(source);
            1
        }

        fn detach(&self, _id: u64) {}

        fn holds(&self, id: u64) -> bool {
            id == 1
        }
    }

    fn play_into_a_call(seconds: f32, commands_first: Vec<PlayerCommand>) -> (usize, Duration) {
        let file = OpusFile::read(&generate_sine_ogg(seconds)).unwrap();
        let (attached, consumer) = std::sync::mpsc::channel();
        let call: Arc<dyn CallMix> = Arc::new(FakeCall(std::sync::Mutex::new(attached)));
        let (events_tx, mut events) = mpsc::unbounded_channel();
        let (commands, commands_rx) = mpsc::unbounded_channel();
        for command in commands_first {
            commands.send(command).unwrap();
        }
        let in_call = call.clone();
        let player = std::thread::spawn(move || {
            run_player(
                file,
                PlaybackItem::Draft { draft_id: 1 },
                0,
                1.0,
                || OutputSink::open(Some(in_call), &AudioSettings::default(), false),
                events_tx,
                commands_rx,
            );
        });
        let consumer = consumer.recv_timeout(Duration::from_secs(5)).unwrap();
        let heard = hear_in_real_time(consumer, &mut events);
        player.join().unwrap();
        drop(call);
        heard
    }

    #[test]
    fn playing_into_a_call_drops_nothing_and_keeps_time() {
        let (frames, took) = play_into_a_call(1.0, Vec::new());
        let heard_ms = frames / 48;
        assert!(
            (950..=1100).contains(&heard_ms),
            "heard {heard_ms} ms of a 1 s message"
        );
        assert!(took >= Duration::from_millis(900), "played in {took:?}");
    }

    #[test]
    fn a_new_speed_applies_to_what_follows() {
        let (frames, _) = play_into_a_call(2.0, vec![PlayerCommand::SetSpeed(2.0)]);
        let heard_ms = frames / 48;
        assert!(
            (900..=1200).contains(&heard_ms),
            "heard {heard_ms} ms of a 2 s message at 2×"
        );
    }
}
