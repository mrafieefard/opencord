//! Voice media for this device (Phase 2 plan §7, §7.14): when a server sends
//! this device to a voice node, connect to it and run the audio engine;
//! follow moves and voice node failovers; report how the connection is
//! doing; ask for a fresh token when a connection cannot be saved.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use opencord_common::voice::{close, speaking};
use opencord_media::audio::capture::{EncodedFrame, InputMode, Sensitivity};
use opencord_media::audio::device;
use opencord_media::audio::engine::{AudioEngine, DeviceChoice, EngineEvent, EngineSettings};
use opencord_media::audio::processing::ProcessingSettings;
use opencord_media::audio::processor::ProcessorSettings;
use opencord_media::transport::{
    AudioFrame, TransportError, VoiceConnection, VoiceEvent, VoiceTarget,
};
use tokio::runtime::Handle;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::api::types::{
    AudioDevice, AudioDevices, AudioSettings, MediaEvent, VoiceConnectionState,
};
use crate::client::Client;
use crate::voice::VoiceServer;

/// A voice server update waits this long for this device's voice state to
/// name the same channel.
const PENDING_FOR: Duration = Duration::from_secs(10);
/// Media not up by then: the UDP port is probably blocked.
const NO_ROUTE_AFTER: Duration = Duration::from_secs(10);
/// After the node closed, a new voice server update usually comes; if not
/// by then, ask.
const REFRESH_AFTER_SHUTDOWN: Duration = Duration::from_secs(5);
/// Asking again after a failed connection, doubling up to the maximum.
const RETRY_MIN: Duration = Duration::from_secs(1);
const RETRY_MAX: Duration = Duration::from_secs(30);
/// How often the device list is checked while in voice.
const DEVICE_POLL: Duration = Duration::from_secs(2);
/// Push-to-talk keeps sending this long after the key is released (plan §7.4).
const RELEASE_DELAY: Duration = Duration::from_millis(200);
/// The bitrate until the node says the channel's.
const DEFAULT_BITRATE: u32 = 64_000;

/// How the media engine is run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaOptions {
    /// Off in tests, so they never open the computer's microphone.
    pub open_devices: bool,
}

/// This device's voice state, as far as media cares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaTarget {
    pub server_key: String,
    pub channel_id: i64,
    /// Self mute, server mute, or no permission to speak.
    pub muted: bool,
    /// Self or server deafen.
    pub deafened: bool,
}

/// What to do with a voice server update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Decision {
    Connect,
    /// It is for a channel this device is not in (yet).
    Wait,
    /// Too old to use.
    Drop,
}

fn decide(target: Option<&MediaTarget>, server: &VoiceServer, age: Duration) -> Decision {
    if age > PENDING_FOR {
        return Decision::Drop;
    }
    match target {
        Some(target)
            if target.server_key == server.server_key && target.channel_id == server.channel_id =>
        {
            Decision::Connect
        }
        _ => Decision::Wait,
    }
}

pub(crate) struct Media {
    runtime: Handle,
    options: MediaOptions,
    events: mpsc::UnboundedSender<MediaEvent>,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    settings: AudioSettings,
    target: Option<MediaTarget>,
    pending: Option<(VoiceServer, Instant)>,
    session: Option<MediaSession>,
    push_to_talk_held: bool,
    /// The listener's choices, per server and user: volume and local mute.
    listening: HashMap<(String, i64), (f32, bool)>,
}

/// One server's voice, from joining to leaving: the engine, and the task
/// that keeps its connection.
struct MediaSession {
    server_key: String,
    engine: Arc<AudioEngine>,
    commands: mpsc::UnboundedSender<SessionCommand>,
}

impl Drop for MediaSession {
    fn drop(&mut self) {
        let _ = self.commands.send(SessionCommand::Stop);
    }
}

enum SessionCommand {
    Connect(VoiceServer),
    Settings(AudioSettings),
    Stop,
}

impl Media {
    pub fn new(
        runtime: Handle,
        options: MediaOptions,
        events: mpsc::UnboundedSender<MediaEvent>,
    ) -> Self {
        Self {
            runtime,
            options,
            events,
            state: Mutex::new(State::default()),
        }
    }

    /// This device's voice state changed: joined, left, moved, or flags.
    pub fn on_target(&self, client: &Client, target: Option<MediaTarget>) {
        let mut state = self.lock();
        if state.target == target {
            return;
        }
        state.target = target.clone();
        let Some(target) = target else {
            state.session = None;
            state.pending = None;
            return;
        };
        if state
            .session
            .as_ref()
            .is_some_and(|session| session.server_key != target.server_key)
        {
            state.session = None;
        }
        if let Some(session) = &state.session {
            session.engine.set_muted(target.muted);
            session.engine.set_deafened(target.deafened);
        }
        self.connect_if_ready(&mut state, client);
    }

    /// A server named the voice node to use.
    pub fn on_voice_server(&self, client: &Client, server: VoiceServer) {
        let mut state = self.lock();
        state.pending = Some((server, Instant::now()));
        self.connect_if_ready(&mut state, client);
    }

    pub fn apply_settings(&self, settings: AudioSettings) {
        let mut state = self.lock();
        if let Some(session) = &state.session {
            let engine = &session.engine;
            if settings.input_device != state.settings.input_device {
                engine.set_input_device(self.device_choice(settings.input_device.as_deref()));
            }
            if settings.output_device != state.settings.output_device {
                engine.set_output_device(self.device_choice(settings.output_device.as_deref()));
            }
            engine.set_mode(input_mode(settings.push_to_talk));
            engine.set_input_volume(settings.input_volume);
            engine.set_output_volume(settings.output_volume);
            let _ = session
                .commands
                .send(SessionCommand::Settings(settings.clone()));
        }
        state.settings = settings;
    }

    pub fn set_push_to_talk(&self, held: bool) {
        let mut state = self.lock();
        state.push_to_talk_held = held;
        if let Some(session) = &state.session {
            session.engine.set_push_to_talk(held);
        }
    }

    pub fn set_user_volume(&self, server_key: &str, user_id: i64, volume: f32) {
        let mut state = self.lock();
        let choice = state
            .listening
            .entry((server_key.to_owned(), user_id))
            .or_insert((1.0, false));
        choice.0 = volume;
        if let Some(session) = state
            .session
            .as_ref()
            .filter(|s| s.server_key == server_key)
        {
            session.engine.set_user_volume(user_id, volume);
        }
    }

    pub fn set_user_local_mute(&self, server_key: &str, user_id: i64, muted: bool) {
        let mut state = self.lock();
        let choice = state
            .listening
            .entry((server_key.to_owned(), user_id))
            .or_insert((1.0, false));
        choice.1 = muted;
        if let Some(session) = state
            .session
            .as_ref()
            .filter(|s| s.server_key == server_key)
        {
            session.engine.set_local_mute(user_id, muted);
        }
    }

    fn connect_if_ready(&self, state: &mut State, client: &Client) {
        let Some((server, at)) = state.pending.take() else {
            return;
        };
        match decide(state.target.as_ref(), &server, at.elapsed()) {
            Decision::Drop => {}
            Decision::Wait => state.pending = Some((server, at)),
            Decision::Connect => {
                if state.session.is_none() {
                    state.session = self.start_session(state, client, &server.server_key);
                }
                if let Some(session) = &state.session {
                    let _ = session.commands.send(SessionCommand::Connect(server));
                }
            }
        }
    }

    fn start_session(
        &self,
        state: &State,
        client: &Client,
        server_key: &str,
    ) -> Option<MediaSession> {
        let settings = &state.settings;
        let (frames, frames_received) = mpsc::unbounded_channel();
        let (engine_events, engine_events_received) = mpsc::unbounded_channel();
        let engine = AudioEngine::start(
            EngineSettings {
                input_device: self.device_choice(settings.input_device.as_deref()),
                output_device: self.device_choice(settings.output_device.as_deref()),
                processor: ProcessorSettings {
                    mode: input_mode(settings.push_to_talk),
                    processing: ProcessingSettings::default(),
                    bitrate: DEFAULT_BITRATE,
                    input_volume: settings.input_volume,
                    output_volume: settings.output_volume,
                },
            },
            move |frame| {
                let _ = frames.send(frame);
            },
            move |event| {
                let _ = engine_events.send(event);
            },
        );
        let engine = match engine {
            Ok(engine) => Arc::new(engine),
            Err(error) => {
                let _ = self.events.send(MediaEvent::DeviceFailed {
                    output: false,
                    message: error.to_string(),
                });
                return None;
            }
        };
        if let Some(target) = &state.target {
            engine.set_muted(target.muted);
            engine.set_deafened(target.deafened);
        }
        engine.set_push_to_talk(state.push_to_talk_held);
        for ((key, user_id), (volume, muted)) in &state.listening {
            if key == server_key {
                engine.set_user_volume(*user_id, *volume);
                engine.set_local_mute(*user_id, *muted);
            }
        }
        let (commands, commands_received) = mpsc::unbounded_channel();
        let task = SessionTask {
            client: client.clone(),
            server_key: server_key.to_owned(),
            engine: Arc::clone(&engine),
            events: self.events.clone(),
            settings: settings.clone(),
            open_devices: self.options.open_devices,
        };
        self.runtime
            .spawn(task.run(commands_received, frames_received, engine_events_received));
        Some(MediaSession {
            server_key: server_key.to_owned(),
            engine,
            commands,
        })
    }

    fn device_choice(&self, id: Option<&str>) -> DeviceChoice {
        device_choice(self.options.open_devices, id)
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn device_choice(open_devices: bool, id: Option<&str>) -> DeviceChoice {
    match (open_devices, id) {
        (false, _) => DeviceChoice::Off,
        (true, None) => DeviceChoice::Default,
        (true, Some(id)) => DeviceChoice::Id(id.to_owned()),
    }
}

fn input_mode(push_to_talk: bool) -> InputMode {
    if push_to_talk {
        InputMode::PushToTalk {
            release_delay: RELEASE_DELAY,
        }
    } else {
        InputMode::VoiceActivity(Sensitivity::Automatic)
    }
}

/// Microphones and speakers the system offers now. Listing can take a
/// moment; call it off the UI thread.
pub fn audio_devices() -> AudioDevices {
    let list = device::devices();
    let convert = |devices: Vec<device::DeviceInfo>| {
        devices
            .into_iter()
            .map(|device| AudioDevice {
                id: device.id,
                name: device.name,
            })
            .collect()
    };
    AudioDevices {
        inputs: convert(list.inputs),
        outputs: convert(list.outputs),
        default_input: list.default_input,
        default_output: list.default_output,
    }
}

/// A voice connection that is up.
struct Live {
    server: VoiceServer,
    connection: VoiceConnection,
    events: mpsc::UnboundedReceiver<VoiceEvent>,
}

type Connecting =
    JoinHandle<Result<(VoiceConnection, mpsc::UnboundedReceiver<VoiceEvent>), TransportError>>;

/// Keeps one session's connection: connects where the server says,
/// moves audio between it and the engine, reconnects, reports.
struct SessionTask {
    client: Client,
    server_key: String,
    engine: Arc<AudioEngine>,
    events: mpsc::UnboundedSender<MediaEvent>,
    settings: AudioSettings,
    open_devices: bool,
}

impl SessionTask {
    async fn run(
        mut self,
        mut commands: mpsc::UnboundedReceiver<SessionCommand>,
        mut frames: mpsc::UnboundedReceiver<EncodedFrame>,
        mut engine_events: mpsc::UnboundedReceiver<EngineEvent>,
    ) {
        let mut live: Option<Live> = None;
        let mut connecting: Option<(VoiceServer, Connecting)> = None;
        let mut refresh_at: Option<Instant> = None;
        let mut retry_pause = RETRY_MIN;
        let mut no_route_at: Option<Instant> = None;
        let mut devices_at = Instant::now() + DEVICE_POLL;
        let mut devices: Option<AudioDevices> = None;
        let mut fell_back = (false, false);
        loop {
            tokio::select! {
                command = commands.recv() => match command {
                    Some(SessionCommand::Connect(server)) => {
                        live = None;
                        if let Some((_, handle)) = connecting.take() {
                            handle.abort();
                        }
                        refresh_at = None;
                        self.report(server.channel_id, VoiceConnectionState::Authenticating);
                        let target = voice_target(&server);
                        connecting = Some((server, tokio::spawn(VoiceConnection::connect(target))));
                    }
                    Some(SessionCommand::Settings(settings)) => self.settings = settings,
                    Some(SessionCommand::Stop) | None => {
                        if let Some((_, handle)) = connecting.take() {
                            handle.abort();
                        }
                        return;
                    }
                },
                result = finished(&mut connecting) => {
                    let (server, result) = result;
                    match result {
                        Ok((connection, events)) => {
                            if let Some(bitrate) = connection.voice_bitrate() {
                                self.engine.set_bitrate(bitrate);
                            }
                            retry_pause = RETRY_MIN;
                            no_route_at = Some(Instant::now() + NO_ROUTE_AFTER);
                            self.report(server.channel_id, VoiceConnectionState::RtcConnecting);
                            live = Some(Live { server, connection, events });
                        }
                        Err(error) => {
                            self.report(
                                server.channel_id,
                                VoiceConnectionState::Disconnected { reason: error.to_string() },
                            );
                            refresh_at = Some(Instant::now() + retry_pause);
                            retry_pause = (retry_pause * 2).min(RETRY_MAX);
                        }
                    }
                }
                Some(frame) = frames.recv() => {
                    if let Some(live) = &live {
                        live.connection.send_audio(audio_frame(frame));
                    }
                }
                event = next_event(&mut live) => {
                    let channel_id = live.as_ref().map_or(0, |live| live.server.channel_id);
                    match event {
                        VoiceEvent::MediaConnected => {
                            no_route_at = None;
                            self.report(channel_id, VoiceConnectionState::Connected);
                        }
                        VoiceEvent::MediaDisconnected => {
                            self.report(channel_id, VoiceConnectionState::Reconnecting);
                        }
                        VoiceEvent::Audio(audio) => self.engine.receive(
                            audio.user_id,
                            audio.timestamp,
                            audio.marker,
                            audio.payload,
                            audio.arrived,
                        ),
                        VoiceEvent::ClientDisconnected { user_id } => self.engine.remove_user(user_id),
                        VoiceEvent::Closed { code } => {
                            live = None;
                            no_route_at = None;
                            match code {
                                // Moved, left or replaced: the main server
                                // already has news on the way.
                                Some(close::DISCONNECTED | close::SESSION_REPLACED) => {}
                                Some(close::NODE_SHUTDOWN) => {
                                    self.report(channel_id, VoiceConnectionState::AwaitingEndpoint);
                                    refresh_at = Some(Instant::now() + REFRESH_AFTER_SHUTDOWN);
                                }
                                _ => {
                                    self.report(channel_id, VoiceConnectionState::Reconnecting);
                                    refresh_at = Some(Instant::now());
                                }
                            }
                        }
                        VoiceEvent::ClientConnected { .. }
                        | VoiceEvent::Speaking { .. }
                        | VoiceEvent::Resumed => {}
                    }
                }
                Some(event) = engine_events.recv() => match event {
                    EngineEvent::Talking(talking) => {
                        if let Some(live) = &live {
                            live.connection.set_speaking(if talking { speaking::MICROPHONE } else { 0 });
                        }
                    }
                    EngineEvent::DeviceFellBack { output, device } => {
                        if output { fell_back.1 = true } else { fell_back.0 = true }
                        let _ = self.events.send(MediaEvent::DeviceFellBack { output, device });
                    }
                    EngineEvent::DeviceFailed { output, message } => {
                        let _ = self.events.send(MediaEvent::DeviceFailed { output, message });
                    }
                },
                () = sleep_until(refresh_at) => {
                    refresh_at = None;
                    let client = self.client.clone();
                    let key = self.server_key.clone();
                    tokio::spawn(async move { client.refresh_voice_server(&key).await });
                }
                () = sleep_until(no_route_at) => {
                    no_route_at = None;
                    let channel_id = live.as_ref().map_or(0, |live| live.server.channel_id);
                    self.report(channel_id, VoiceConnectionState::NoRoute);
                }
                () = tokio::time::sleep_until(devices_at), if self.open_devices => {
                    devices_at = Instant::now() + DEVICE_POLL;
                    let Ok(now) = tokio::task::spawn_blocking(audio_devices).await else {
                        continue;
                    };
                    if devices.as_ref() == Some(&now) {
                        continue;
                    }
                    self.switch_back(&now, &mut fell_back);
                    let _ = self.events.send(MediaEvent::DevicesChanged(now.clone()));
                    devices = Some(now);
                }
            }
        }
    }

    /// A chosen device that had gone away is back: use it again.
    fn switch_back(&self, devices: &AudioDevices, fell_back: &mut (bool, bool)) {
        let listed = |list: &[AudioDevice], id: &Option<String>| {
            id.as_ref()
                .is_some_and(|id| list.iter().any(|device| device.id == *id))
        };
        if fell_back.0 && listed(&devices.inputs, &self.settings.input_device) {
            fell_back.0 = false;
            self.engine.set_input_device(device_choice(
                self.open_devices,
                self.settings.input_device.as_deref(),
            ));
        }
        if fell_back.1 && listed(&devices.outputs, &self.settings.output_device) {
            fell_back.1 = false;
            self.engine.set_output_device(device_choice(
                self.open_devices,
                self.settings.output_device.as_deref(),
            ));
        }
    }

    fn report(&self, channel_id: i64, state: VoiceConnectionState) {
        let _ = self.events.send(MediaEvent::ConnectionState {
            server_key: self.server_key.clone(),
            channel_id,
            state,
        });
    }
}

fn voice_target(server: &VoiceServer) -> VoiceTarget {
    VoiceTarget {
        gateway_url: server.gateway_url(),
        certificate_fingerprint: server.certificate_fingerprint.as_slice().try_into().ok(),
        token: server.token.clone(),
        user_id: server.user_id,
        session_id: server.session_id.clone(),
        channel_id: server.channel_id,
    }
}

fn audio_frame(frame: EncodedFrame) -> AudioFrame {
    AudioFrame {
        payload: frame.payload,
        position: frame.position,
        marker: frame.marker,
        audio_level: frame.audio_level,
        voice_activity: true,
    }
}

/// The connection being made, once it is made or fails.
async fn finished(
    connecting: &mut Option<(VoiceServer, Connecting)>,
) -> (
    VoiceServer,
    Result<(VoiceConnection, mpsc::UnboundedReceiver<VoiceEvent>), TransportError>,
) {
    let Some((_, handle)) = connecting.as_mut() else {
        return std::future::pending().await;
    };
    let result = match handle.await {
        Ok(result) => result,
        Err(error) => Err(TransportError::Connect(error.to_string())),
    };
    let (server, _) = connecting.take().expect("a connection was being made");
    (server, result)
}

/// The live connection's next event; a connection whose events ended is
/// closed.
async fn next_event(live: &mut Option<Live>) -> VoiceEvent {
    match live.as_mut() {
        Some(live) => live
            .events
            .recv()
            .await
            .unwrap_or(VoiceEvent::Closed { code: None }),
        None => std::future::pending().await,
    }
}

async fn sleep_until(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(key: &str, channel_id: i64) -> VoiceServer {
        VoiceServer {
            server_key: key.to_owned(),
            user_id: 1,
            session_id: "s".to_owned(),
            channel_id,
            endpoint: String::new(),
            certificate_fingerprint: vec![0; 32],
            token: vec![1],
        }
    }

    fn target(key: &str, channel_id: i64) -> MediaTarget {
        MediaTarget {
            server_key: key.to_owned(),
            channel_id,
            muted: false,
            deafened: false,
        }
    }

    #[test]
    fn a_voice_server_for_this_devices_channel_connects() {
        let decision = decide(Some(&target("a", 5)), &server("a", 5), Duration::ZERO);

        assert_eq!(decision, Decision::Connect);
    }

    #[test]
    fn a_voice_server_waits_for_the_voice_state_that_matches_it() {
        let elsewhere = decide(Some(&target("a", 5)), &server("a", 6), Duration::ZERO);
        let other_server = decide(Some(&target("b", 5)), &server("a", 5), Duration::ZERO);
        let not_in_voice = decide(None, &server("a", 5), Duration::ZERO);

        assert_eq!(elsewhere, Decision::Wait);
        assert_eq!(other_server, Decision::Wait);
        assert_eq!(not_in_voice, Decision::Wait);
    }

    #[test]
    fn an_old_voice_server_is_dropped() {
        let decision = decide(
            Some(&target("a", 5)),
            &server("a", 5),
            PENDING_FOR + Duration::from_millis(1),
        );

        assert_eq!(decision, Decision::Drop);
    }

    #[test]
    fn devices_stay_closed_when_media_must_not_open_them() {
        assert_eq!(device_choice(false, Some("pipewire:x")), DeviceChoice::Off);
        assert_eq!(device_choice(true, None), DeviceChoice::Default);
        assert_eq!(
            device_choice(true, Some("pipewire:x")),
            DeviceChoice::Id("pipewire:x".to_owned())
        );
    }
}
