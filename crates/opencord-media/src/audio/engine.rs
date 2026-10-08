//! The audio engine (plan §7.1): one thread, at raised priority where the
//! system allows, that runs the processing tick between the devices and
//! the network every 5 ms. It owns the microphone and the speaker; when a
//! chosen device goes away it falls back to the default one and says so.
//! Who is speaking, and the microphone's level while a meter is open, go
//! out at most every 50 ms (plan §7.5, §7.12).

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::capture::{EncodedFrame, InputMode};
use super::device::{DeviceError, Microphone, Speaker};
use super::processing::ProcessingSettings;
use super::processor::{Processor, ProcessorError, ProcessorSettings};
use super::{SAMPLE_RATE, TICK};

/// How often the engine wakes.
const WAKE_EVERY: Duration = Duration::from_millis(5);
/// Audio kept queued for the speaker: enough to ride out a late wake.
const SPEAKER_QUEUE: Duration = Duration::from_millis(30);
/// Speaking changes and the input level go out at most this often.
const REPORT_EVERY: Duration = Duration::from_millis(50);

/// Which device to use.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DeviceChoice {
    /// The system's default, following it when it changes.
    #[default]
    Default,
    /// A device id from [`super::device::devices`]; the default stands in
    /// while it is missing.
    Id(String),
    /// None at all: nothing is captured, and playback runs on the clock.
    Off,
}

/// Where audio goes in and out, and how.
#[derive(Debug, Clone, PartialEq)]
pub struct EngineSettings {
    pub input_device: DeviceChoice,
    pub output_device: DeviceChoice,
    pub processor: ProcessorSettings,
}

/// What the engine reports, from its own thread.
#[derive(Debug, Clone, PartialEq)]
pub enum EngineEvent {
    /// The chosen device is missing; the default one plays or listens.
    DeviceFellBack { output: bool, device: String },
    /// No device could be opened.
    DeviceFailed { output: bool, message: String },
    /// The microphone started or stopped sending, and whether as the
    /// priority speaker.
    Talking { talking: bool, priority: bool },
    /// Who started or stopped speaking (user, speaking), since the last.
    Speaking(Vec<(i64, bool)>),
    /// The microphone's loudest tick in dBFS, while a meter is open.
    InputLevel(f32),
    /// Someone spoke while muted (at most every 30 s).
    SpeakingWhileMuted,
    /// High noise suppression could not keep up; Standard took over.
    NoiseSuppressionFellBack,
}

enum Command {
    Receive {
        user_id: i64,
        timestamp: u32,
        marker: bool,
        payload: Arc<[u8]>,
        arrived: Instant,
    },
    RemoveUser(i64),
    Muted(bool),
    Deafened(bool),
    Mode(InputMode),
    PushToTalk(bool),
    InputVolume(f32),
    OutputVolume(f32),
    UserVolume(i64, f32),
    LocalMute(i64, bool),
    Bitrate(u32),
    ExpectedLoss(u8),
    InputDevice(DeviceChoice),
    OutputDevice(DeviceChoice),
    Processing(ProcessingSettings),
    PriorityHeld(bool),
    Priority(i64, bool),
    MicTest(bool),
    LevelMeter(bool),
    Stop,
}

/// A running engine. Dropping it stops it and closes the devices.
pub struct AudioEngine {
    commands: Sender<Command>,
    thread: Option<JoinHandle<()>>,
}

impl AudioEngine {
    /// Opens the devices and starts. `send` gets each frame for the
    /// network; `events` hears about devices and talking. Both are called
    /// on the engine's thread and must not block.
    pub fn start(
        settings: EngineSettings,
        send: impl FnMut(EncodedFrame) + Send + 'static,
        events: impl FnMut(EngineEvent) + Send + 'static,
    ) -> Result<Self, ProcessorError> {
        let (commands, receiver) = mpsc::channel();
        let (started, start_result) = mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("opencord-audio".to_owned())
            .spawn(move || {
                // The processor stays on this thread (its processing graph
                // cannot move between threads). Device rates are known once
                // they are open; until then it assumes 48 kHz stereo.
                let processor =
                    match Processor::new(settings.processor, (SAMPLE_RATE, 2), (SAMPLE_RATE, 2)) {
                        Ok(processor) => {
                            let _ = started.send(Ok(()));
                            processor
                        }
                        Err(error) => {
                            let _ = started.send(Err(error));
                            return;
                        }
                    };
                let mut engine = Engine {
                    level_meter: false,
                    reported_at: Instant::now(),
                    changes: Vec::new(),
                    processor,
                    microphone: None,
                    speaker: None,
                    input_device: settings.input_device,
                    output_device: settings.output_device,
                    send: Box::new(send),
                    events: Box::new(events),
                    talking: (false, false),
                    unplayed_since: Instant::now(),
                    scratch: Vec::new(),
                };
                engine.open_microphone();
                engine.open_speaker();
                engine.run(receiver);
            })
            .expect("the system can start a thread");
        match start_result.recv() {
            Ok(Ok(())) => Ok(Self {
                commands,
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => {
                let _ = thread.join();
                Err(ProcessorError::Stopped)
            }
        }
    }

    /// A packet from `user_id`.
    pub fn receive(
        &self,
        user_id: i64,
        timestamp: u32,
        marker: bool,
        payload: Arc<[u8]>,
        arrived: Instant,
    ) {
        self.command(Command::Receive {
            user_id,
            timestamp,
            marker,
            payload,
            arrived,
        });
    }

    pub fn remove_user(&self, user_id: i64) {
        self.command(Command::RemoveUser(user_id));
    }

    pub fn set_muted(&self, muted: bool) {
        self.command(Command::Muted(muted));
    }

    pub fn set_deafened(&self, deafened: bool) {
        self.command(Command::Deafened(deafened));
    }

    pub fn set_mode(&self, mode: InputMode) {
        self.command(Command::Mode(mode));
    }

    pub fn set_push_to_talk(&self, held: bool) {
        self.command(Command::PushToTalk(held));
    }

    pub fn set_input_volume(&self, volume: f32) {
        self.command(Command::InputVolume(volume));
    }

    pub fn set_output_volume(&self, volume: f32) {
        self.command(Command::OutputVolume(volume));
    }

    pub fn set_user_volume(&self, user_id: i64, volume: f32) {
        self.command(Command::UserVolume(user_id, volume));
    }

    pub fn set_local_mute(&self, user_id: i64, muted: bool) {
        self.command(Command::LocalMute(user_id, muted));
    }

    pub fn set_bitrate(&self, bitrate: u32) {
        self.command(Command::Bitrate(bitrate));
    }

    /// Loss the receivers report, in percent.
    pub fn set_expected_loss(&self, percent: u8) {
        self.command(Command::ExpectedLoss(percent));
    }

    pub fn set_input_device(&self, choice: DeviceChoice) {
        self.command(Command::InputDevice(choice));
    }

    pub fn set_output_device(&self, choice: DeviceChoice) {
        self.command(Command::OutputDevice(choice));
    }

    /// Echo cancellation, noise suppression and gain control.
    pub fn set_processing(&self, settings: ProcessingSettings) {
        self.command(Command::Processing(settings));
    }

    /// The priority speaker key, held or let go.
    pub fn set_priority_held(&self, held: bool) {
        self.command(Command::PriorityHeld(held));
    }

    /// Whether `user_id` speaks as the priority speaker now.
    pub fn set_priority(&self, user_id: i64, priority: bool) {
        self.command(Command::Priority(user_id, priority));
    }

    /// Hear what would be sent.
    pub fn set_mic_test(&self, on: bool) {
        self.command(Command::MicTest(on));
    }

    /// Report the microphone's level 20 times a second while on.
    pub fn set_level_meter(&self, on: bool) {
        self.command(Command::LevelMeter(on));
    }

    fn command(&self, command: Command) {
        let _ = self.commands.send(command);
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Engine {
    processor: Processor,
    microphone: Option<Microphone>,
    speaker: Option<Speaker>,
    input_device: DeviceChoice,
    output_device: DeviceChoice,
    send: Box<dyn FnMut(EncodedFrame) + Send>,
    events: Box<dyn FnMut(EngineEvent) + Send>,
    /// Talking, and as the priority speaker.
    talking: (bool, bool),
    level_meter: bool,
    reported_at: Instant,
    changes: Vec<(i64, bool)>,
    /// Without a speaker, playing follows the clock from here.
    unplayed_since: Instant,
    scratch: Vec<f32>,
}

impl Engine {
    fn run(&mut self, commands: Receiver<Command>) {
        let _priority = match audio_thread_priority::promote_current_thread_to_real_time(
            TICK as u32,
            SAMPLE_RATE,
        ) {
            Ok(handle) => Some(handle),
            Err(error) => {
                tracing::warn!(%error, "the audio thread runs at normal priority");
                None
            }
        };
        let mut wake = Instant::now();
        loop {
            loop {
                match commands.try_recv() {
                    Ok(Command::Stop) | Err(TryRecvError::Disconnected) => return,
                    Ok(command) => self.apply(command),
                    Err(TryRecvError::Empty) => break,
                }
            }
            self.replace_lost_devices();
            self.capture();
            let now = Instant::now();
            self.play(now);
            self.report(now);
            wake += WAKE_EVERY;
            let now = Instant::now();
            if wake <= now {
                // Late; start over rather than catch up.
                wake = now + WAKE_EVERY;
            }
            std::thread::sleep(wake - now);
        }
    }

    fn apply(&mut self, command: Command) {
        let processor = &mut self.processor;
        match command {
            Command::Receive {
                user_id,
                timestamp,
                marker,
                payload,
                arrived,
            } => processor.receive(user_id, timestamp, marker, payload, arrived),
            Command::RemoveUser(user_id) => processor.remove_user(user_id),
            Command::Muted(muted) => processor.set_muted(muted),
            Command::Deafened(deafened) => processor.set_deafened(deafened),
            Command::Mode(mode) => processor.set_mode(mode),
            Command::PushToTalk(held) => processor.set_push_to_talk(held),
            Command::InputVolume(volume) => processor.set_input_volume(volume),
            Command::OutputVolume(volume) => processor.set_output_volume(volume),
            Command::UserVolume(user_id, volume) => processor.set_user_volume(user_id, volume),
            Command::LocalMute(user_id, muted) => processor.set_local_mute(user_id, muted),
            Command::Bitrate(bitrate) => {
                if let Err(error) = processor.set_bitrate(bitrate) {
                    tracing::warn!(%error, bitrate, "could not change the voice bitrate");
                }
            }
            Command::ExpectedLoss(percent) => {
                let _ = processor.set_expected_loss(percent);
            }
            Command::InputDevice(choice) => {
                self.input_device = choice;
                self.microphone = None;
                self.open_microphone();
            }
            Command::OutputDevice(choice) => {
                self.output_device = choice;
                self.speaker = None;
                self.unplayed_since = Instant::now();
                self.open_speaker();
            }
            Command::Processing(settings) => {
                if let Err(error) = processor.set_processing(settings) {
                    tracing::warn!(%error, "could not change the voice processing");
                }
            }
            Command::PriorityHeld(held) => processor.set_priority_held(held),
            Command::Priority(user_id, priority) => processor.set_priority(user_id, priority),
            Command::MicTest(on) => processor.set_mic_test(on),
            Command::LevelMeter(on) => self.level_meter = on,
            Command::Stop => {}
        }
    }

    fn report(&mut self, now: Instant) {
        let talking = (self.processor.is_talking(), self.processor.is_priority());
        if talking != self.talking {
            self.talking = talking;
            (self.events)(EngineEvent::Talking {
                talking: talking.0,
                priority: talking.1,
            });
        }
        if self.processor.take_speaking_while_muted() {
            (self.events)(EngineEvent::SpeakingWhileMuted);
        }
        if self.processor.take_noise_suppression_fallback() {
            (self.events)(EngineEvent::NoiseSuppressionFellBack);
        }
        if now < self.reported_at + REPORT_EVERY {
            return;
        }
        self.reported_at = now;
        self.processor.speaking_changes(&mut self.changes);
        if !self.changes.is_empty() {
            (self.events)(EngineEvent::Speaking(std::mem::take(&mut self.changes)));
        }
        let level = self.processor.take_input_level();
        if let Some(level) = level.filter(|_| self.level_meter) {
            (self.events)(EngineEvent::InputLevel(level));
        }
    }

    fn replace_lost_devices(&mut self) {
        if self
            .microphone
            .as_ref()
            .is_some_and(|microphone| microphone.health.is_lost())
        {
            self.microphone = None;
            self.open_microphone();
        }
        if self
            .speaker
            .as_ref()
            .is_some_and(|speaker| speaker.health.is_lost())
        {
            self.speaker = None;
            self.open_speaker();
        }
    }

    /// The chosen microphone, or the default one when it is missing.
    fn open_microphone(&mut self) {
        let Some(opened) = open_or_default(&self.input_device, Microphone::open) else {
            return;
        };
        match opened {
            Ok((microphone, fell_back)) => {
                if let Err(error) = self
                    .processor
                    .set_input_device(microphone.rate, microphone.channels)
                {
                    tracing::warn!(%error, "the microphone's format is not usable");
                    return;
                }
                if fell_back {
                    (self.events)(EngineEvent::DeviceFellBack {
                        output: false,
                        device: microphone.name.clone(),
                    });
                }
                self.microphone = Some(microphone);
            }
            Err(error) => (self.events)(EngineEvent::DeviceFailed {
                output: false,
                message: error.to_string(),
            }),
        }
    }

    fn open_speaker(&mut self) {
        let Some(opened) = open_or_default(&self.output_device, Speaker::open) else {
            return;
        };
        match opened {
            Ok((speaker, fell_back)) => {
                if let Err(error) = self
                    .processor
                    .set_output_device(speaker.rate, speaker.channels)
                {
                    tracing::warn!(%error, "the speaker's format is not usable");
                    return;
                }
                if fell_back {
                    (self.events)(EngineEvent::DeviceFellBack {
                        output: true,
                        device: speaker.name.clone(),
                    });
                }
                self.speaker = Some(speaker);
            }
            Err(error) => {
                self.unplayed_since = Instant::now();
                (self.events)(EngineEvent::DeviceFailed {
                    output: true,
                    message: error.to_string(),
                });
            }
        }
    }

    fn capture(&mut self) {
        let Some(microphone) = self.microphone.as_mut() else {
            return;
        };
        let ready = microphone.samples.slots();
        if ready == 0 {
            return;
        }
        self.scratch.clear();
        if let Ok(chunk) = microphone.samples.read_chunk(ready) {
            self.scratch.extend(chunk);
        }
        self.processor.capture(&self.scratch, &mut self.send);
    }

    /// Keeps the speaker's queue topped up; without a speaker, plays to
    /// nowhere at the clock's pace so incoming audio does not pile up.
    fn play(&mut self, now: Instant) {
        let Some(speaker) = self.speaker.as_mut() else {
            let ticks = now
                .saturating_duration_since(self.unplayed_since)
                .as_millis()
                / 10;
            for _ in 0..ticks {
                self.processor.play_unheard(now);
            }
            self.unplayed_since += Duration::from_millis(10 * ticks as u64);
            return;
        };
        let channels = usize::from(speaker.channels.max(1));
        let ten_ms = speaker.rate as usize / 100 * channels;
        let target = (speaker.rate as usize * SPEAKER_QUEUE.as_millis() as usize / 1000) * channels;
        while speaker.queued() < target && speaker.samples.slots() >= ten_ms {
            self.scratch.resize(ten_ms, 0.0);
            let frames = self.processor.play(now, &mut self.scratch[..ten_ms]);
            let samples = frames * channels;
            if let Ok(chunk) = speaker.samples.write_chunk_uninit(samples) {
                chunk.fill_from_iter(self.scratch[..samples].iter().copied());
            }
            if samples == 0 {
                break;
            }
        }
    }
}

/// Opens the chosen device, or the default one when that fails, and says
/// whether it fell back; `None` when the choice is no device.
fn open_or_default<T>(
    choice: &DeviceChoice,
    open: impl Fn(Option<&str>) -> Result<T, DeviceError>,
) -> Option<Result<(T, bool), DeviceError>> {
    Some(match choice {
        DeviceChoice::Off => return None,
        DeviceChoice::Default => open(None).map(|device| (device, false)),
        DeviceChoice::Id(id) => match open(Some(id)) {
            Ok(device) => Ok((device, false)),
            Err(error) => {
                tracing::info!(%error, "using the default device instead");
                open(None).map(|device| (device, true))
            }
        },
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::audio::capture::Sensitivity;
    use crate::audio::processing::ProcessingSettings;

    /// Needs a microphone and a speaker; run with `--ignored` on a machine
    /// that has them.
    #[test]
    #[ignore = "needs audio devices"]
    fn an_open_microphone_sends_frames_and_a_missing_device_falls_back() {
        let frames = Arc::new(Mutex::new(0usize));
        let events = Arc::new(Mutex::new(Vec::new()));
        let counted = Arc::clone(&frames);
        let heard = Arc::clone(&events);
        let engine = AudioEngine::start(
            EngineSettings {
                input_device: DeviceChoice::Id("pipewire:no-such-microphone".to_owned()),
                output_device: DeviceChoice::Default,
                processor: ProcessorSettings {
                    // Everything passes, so the room's noise is sent.
                    mode: InputMode::VoiceActivity(Sensitivity::Manual {
                        threshold_dbfs: -130.0,
                    }),
                    processing: ProcessingSettings::default(),
                    bitrate: 64_000,
                    input_volume: 1.0,
                    output_volume: 1.0,
                },
            },
            move |_| *counted.lock().unwrap() += 1,
            move |event| heard.lock().unwrap().push(event),
        )
        .unwrap();

        std::thread::sleep(Duration::from_millis(600));
        drop(engine);

        let sent = *frames.lock().unwrap();
        assert!(sent >= 15, "{sent} frames in 600 ms");
        let events = events.lock().unwrap();
        assert!(
            events
                .iter()
                .any(|event| matches!(event, EngineEvent::DeviceFellBack { output: false, .. })),
            "{events:?}"
        );
    }

    fn quiet_engine(events: Arc<Mutex<Vec<(Instant, EngineEvent)>>>) -> AudioEngine {
        AudioEngine::start(
            EngineSettings {
                input_device: DeviceChoice::Off,
                output_device: DeviceChoice::Off,
                processor: ProcessorSettings {
                    mode: InputMode::VoiceActivity(Sensitivity::Automatic),
                    processing: ProcessingSettings::default(),
                    bitrate: 64_000,
                    input_volume: 1.0,
                    output_volume: 1.0,
                },
            },
            |_| {},
            move |event| events.lock().unwrap().push((Instant::now(), event)),
        )
        .unwrap()
    }

    #[test]
    fn who_starts_and_stops_speaking_is_reported_as_it_happens() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let engine = quiet_engine(Arc::clone(&events));
        let mut encoder = crate::audio::codec::VoiceEncoder::new(64_000).unwrap();
        let mut packet = [0u8; crate::audio::codec::MAX_PACKET];

        // Half a second of a tone from user 7, as it would arrive.
        for frame in 0..25 {
            let tone: Vec<f32> = (0..crate::audio::FRAME)
                .map(|i| {
                    let t = (frame * crate::audio::FRAME + i) as f32 / SAMPLE_RATE as f32;
                    0.2 * (std::f32::consts::TAU * 300.0 * t).sin()
                })
                .collect();
            let size = encoder.encode(&tone, &mut packet).unwrap();
            engine.receive(
                7,
                (frame * crate::audio::FRAME) as u32,
                frame == 0,
                Arc::from(&packet[..size]),
                Instant::now(),
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        std::thread::sleep(Duration::from_millis(600));
        drop(engine);

        let events = events.lock().unwrap();
        let speaking: Vec<&(Instant, EngineEvent)> = events
            .iter()
            .filter(|(_, event)| matches!(event, EngineEvent::Speaking(_)))
            .collect();
        let changes: Vec<&Vec<(i64, bool)>> = speaking
            .iter()
            .map(|(_, event)| match event {
                EngineEvent::Speaking(changes) => changes,
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(
            changes,
            vec![&vec![(7, true)], &vec![(7, false)]],
            "{events:?}"
        );
        assert!(speaking[1].0 - speaking[0].0 >= Duration::from_millis(400));
    }

    #[test]
    fn the_input_level_is_only_reported_while_a_meter_is_open() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let engine = quiet_engine(Arc::clone(&events));

        engine.set_level_meter(true);
        engine.set_mic_test(true);
        engine.set_priority_held(true);
        engine.set_priority(7, true);
        engine.set_processing(ProcessingSettings::default());
        std::thread::sleep(Duration::from_millis(200));
        drop(engine);

        // Without a microphone there is no level to report, and nothing
        // else goes wrong.
        let events = events.lock().unwrap();
        assert!(
            !events
                .iter()
                .any(|(_, event)| matches!(event, EngineEvent::InputLevel(_))),
            "{events:?}"
        );
    }
}
