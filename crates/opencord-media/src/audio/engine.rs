//! The audio engine (plan §7.1): one thread, at raised priority where the
//! system allows, that runs the processing tick between the devices and
//! the network every 5 ms. It owns the microphone and the speaker; when a
//! chosen device goes away it falls back to the default one and says so.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::capture::{EncodedFrame, InputMode};
use super::device::{DeviceError, Microphone, Speaker};
use super::processor::{Processor, ProcessorError, ProcessorSettings};
use super::{SAMPLE_RATE, TICK};

/// How often the engine wakes.
const WAKE_EVERY: Duration = Duration::from_millis(5);
/// Audio kept queued for the speaker: enough to ride out a late wake.
const SPEAKER_QUEUE: Duration = Duration::from_millis(30);

/// Where audio goes in and out, and how.
#[derive(Debug, Clone, PartialEq)]
pub struct EngineSettings {
    /// A device id from [`super::device::devices`]; `None` follows the
    /// system's default.
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    pub processor: ProcessorSettings,
}

/// What the engine reports, from its own thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineEvent {
    /// The chosen device is missing; the default one plays or listens.
    DeviceFellBack { output: bool, device: String },
    /// No device could be opened.
    DeviceFailed { output: bool, message: String },
    /// The microphone started or stopped sending.
    Talking(bool),
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
    InputDevice(Option<String>),
    OutputDevice(Option<String>),
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
        // Device rates are known once they are open, on the engine thread;
        // until then the processor assumes 48 kHz stereo.
        let processor = Processor::new(settings.processor, (SAMPLE_RATE, 2), (SAMPLE_RATE, 2))?;
        let (commands, receiver) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("opencord-audio".to_owned())
            .spawn(move || {
                let mut engine = Engine {
                    processor,
                    microphone: None,
                    speaker: None,
                    input_device: settings.input_device,
                    output_device: settings.output_device,
                    send: Box::new(send),
                    events: Box::new(events),
                    talking: false,
                    unplayed_since: Instant::now(),
                    scratch: Vec::new(),
                };
                engine.open_microphone();
                engine.open_speaker();
                engine.run(receiver);
            })
            .expect("the system can start a thread");
        Ok(Self {
            commands,
            thread: Some(thread),
        })
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

    /// `None` follows the system's default microphone.
    pub fn set_input_device(&self, id: Option<String>) {
        self.command(Command::InputDevice(id));
    }

    pub fn set_output_device(&self, id: Option<String>) {
        self.command(Command::OutputDevice(id));
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
    /// What the user chose; `None` is the system's default.
    input_device: Option<String>,
    output_device: Option<String>,
    send: Box<dyn FnMut(EncodedFrame) + Send>,
    events: Box<dyn FnMut(EngineEvent) + Send>,
    talking: bool,
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
            self.play(Instant::now());
            let talking = self.processor.is_talking();
            if talking != self.talking {
                self.talking = talking;
                (self.events)(EngineEvent::Talking(talking));
            }
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
            Command::InputDevice(id) => {
                self.input_device = id;
                self.microphone = None;
                self.open_microphone();
            }
            Command::OutputDevice(id) => {
                self.output_device = id;
                self.speaker = None;
                self.open_speaker();
            }
            Command::Stop => {}
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
        let opened = open_or_default(self.input_device.as_deref(), Microphone::open);
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
        let opened = open_or_default(self.output_device.as_deref(), Speaker::open);
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
                self.scratch.resize(TICK, 0.0);
                self.processor.play(now, &mut self.scratch[..TICK]);
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

/// Opens the device with `id`, or the default one when that fails; says
/// whether it fell back.
fn open_or_default<T>(
    id: Option<&str>,
    open: impl Fn(Option<&str>) -> Result<T, DeviceError>,
) -> Result<(T, bool), DeviceError> {
    match id {
        None => open(None).map(|device| (device, false)),
        Some(id) => match open(Some(id)) {
            Ok(device) => Ok((device, false)),
            Err(error) => {
                tracing::info!(%error, "using the default device instead");
                open(None).map(|device| (device, true))
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

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
                input_device: Some("pipewire:no-such-microphone".to_owned()),
                output_device: None,
                processor: ProcessorSettings {
                    // Everything passes, so the room's noise is sent.
                    mode: InputMode::VoiceActivity {
                        threshold_dbfs: -130.0,
                    },
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
}
