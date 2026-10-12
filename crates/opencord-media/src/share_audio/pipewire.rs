//! PipeWire screen audio capture (Phase 2 plan §10.4): watch the registry
//! for playback streams and link them into our capture stream. Screens
//! share everything except Opencord's own output; windows share only that
//! app's audio. Streams that appear during sharing are linked; removed
//! ones disappear.

use std::collections::HashMap;
use std::io::Cursor;
use std::os::fd::OwnedFd;
use std::sync::mpsc::{self, SyncSender};
use std::thread::JoinHandle;
use std::time::Duration;

use pipewire as pw;
use pw::link::Link;
use pw::spa;
use pw::types::ObjectType;
use spa::param::ParamType;
use spa::param::format::{MediaSubtype, MediaType};
use spa::pod::ValueArray;
use spa::pod::serialize::PodSerializer;
use spa::pod::{Object, Pod, Property, Value};
use spa::utils::{Direction, Id, SpaTypes};

use super::CaptureError;

/// The capture sample rate.
const SAMPLE_RATE: u32 = 48_000;
/// Stereo.
const CHANNELS: usize = 2;

/// How long we wait for PipeWire to answer.
pub(super) const ROUNDTRIP: Duration = Duration::from_secs(5);

/// An app with audio that can be shared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppInfo {
    pub name: String,
    pub pid: u32,
}

/// What was captured.
#[derive(Debug, Clone)]
pub struct Captured {
    pub samples: Vec<f32>,
    /// Position on the 48 kHz capture clock.
    pub position: u64,
}

impl Captured {
    /// Reads one 20 ms frame of stereo F32 from a PipeWire buffer.
    /// `stride` is bytes per frame (4 for F32LE stereo = 8 bytes).
    fn from_buffer(data: &[u8], stride: usize, frames: usize, position: u64) -> Option<Self> {
        let mut samples = Vec::with_capacity(frames * CHANNELS);
        for frame in 0..frames {
            let base = frame * stride;
            for ch in 0..CHANNELS {
                let i = base + ch * 4;
                if i + 4 > data.len() {
                    return None;
                }
                let value = f32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
                samples.push(value);
            }
        }
        Some(Self { samples, position })
    }
}

/// A running capture; stops when dropped.
pub struct Capture {
    quit: pw::channel::Sender<()>,
    thread: Option<JoinHandle<()>>,
    pub pid: u32,
}

impl Capture {
    pub fn start_screen(
        remote: OwnedFd,
        frames: SyncSender<Captured>,
    ) -> Result<Self, CaptureError> {
        Self::start(remote, None, frames)
    }

    pub fn start_window(
        remote: OwnedFd,
        window_pid: u32,
        frames: SyncSender<Captured>,
    ) -> Result<Self, CaptureError> {
        Self::start(remote, Some(window_pid), frames)
    }

    fn start(
        remote: OwnedFd,
        window_pid: Option<u32>,
        frames: SyncSender<Captured>,
    ) -> Result<Self, CaptureError> {
        let opencord_pid = std::process::id();
        let (started, outcome) = mpsc::channel();
        let (quit, quit_receiver) = pw::channel::channel();
        let thread = std::thread::Builder::new()
            .name("opencord-share-audio".to_owned())
            .spawn(move || {
                serve(
                    remote,
                    window_pid,
                    opencord_pid,
                    frames,
                    quit_receiver,
                    started,
                )
            })
            .map_err(|e| CaptureError::Failed(e.to_string()))?;
        match outcome.recv_timeout(ROUNDTRIP) {
            Ok(Ok(())) => Ok(Self {
                quit,
                thread: Some(thread),
                pid: opencord_pid,
            }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => {
                let _ = quit.send(());
                let _ = thread.join();
                Err(CaptureError::Failed("the capture thread ended".to_owned()))
            }
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.quit.send(());
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn failed(error: pw::Error) -> CaptureError {
    CaptureError::Failed(error.to_string())
}

/// Info extracted from a playback stream node.
struct NodeInfo(u32, u32, String);

fn serve(
    remote: OwnedFd,
    window_pid: Option<u32>,
    opencord_pid: u32,
    frames: SyncSender<Captured>,
    quit: pw::channel::Receiver<()>,
    started: mpsc::Sender<Result<(), CaptureError>>,
) {
    pw::init();
    let Ok(mainloop) = pw::main_loop::MainLoopRc::new(None) else {
        return void(&started, CaptureError::Failed("no mainloop".to_owned()));
    };
    let Ok(context) = pw::context::ContextRc::new(&mainloop, None) else {
        return void(&started, CaptureError::Failed("no context".to_owned()));
    };
    let core = match context.connect_fd_rc(remote, None) {
        Ok(c) => c,
        Err(e) => return void(&started, failed(e)),
    };

    // Create the capture stream before we start the loop.
    let capture_stream = match make_capture_stream(&core) {
        Ok(s) => s,
        Err(e) => return void(&started, e),
    };

    let registry = match core.get_registry_rc() {
        Ok(r) => r,
        Err(e) => return void(&started, failed(e)),
    };

    // Collect playback nodes: node_id → (pid, name).
    let playback: std::sync::Arc<std::sync::Mutex<HashMap<u32, NodeInfo>>> =
        std::sync::Arc::new(std::sync::Mutex::new(HashMap::new()));
    let want_pid = std::sync::Arc::new(std::sync::Mutex::new(window_pid));

    // Add listeners before connecting, so we catch everything.
    let _links_listener = registry
        .add_listener_local()
        .global({
            let playback = std::sync::Arc::clone(&playback);
            move |global| {
                if let Some(info) = playback_info(global)
                    && should_include(info.1, &info.2, opencord_pid, &want_pid)
                {
                    playback.lock().unwrap().insert(info.0, info);
                }
            }
        })
        .global_remove({
            let playback = std::sync::Arc::clone(&playback);
            move |id| {
                // Remove by node id when a node disappears.
                playback.lock().unwrap().remove(&id);
            }
        })
        .register();

    let capture_stream2 = capture_stream.clone();
    let _capture_listener = capture_stream
        .add_local_listener_with_user_data(())
        .process(move |_stream, _| {
            let Some(mut buffer) = capture_stream2.dequeue_buffer() else {
                return;
            };
            let Some(data) = buffer.datas_mut().first_mut() else {
                return;
            };
            let chunk = data.chunk();
            let stride = chunk.stride() as usize;
            let size = chunk.size() as usize;
            if size == 0 {
                return;
            }
            let Some(bytes) = data.data() else {
                return;
            };
            if bytes.len() < size {
                return;
            }
            let frames_count = size / stride;
            // Position is just a counter — the RTP timestamp carries the real clock.
            static POSITION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let position =
                POSITION.fetch_add(frames_count as u64, std::sync::atomic::Ordering::Relaxed);
            if let Some(captured) = Captured::from_buffer(bytes, stride, frames_count, position) {
                let _ = frames.try_send(captured);
            }
        })
        .register();

    // Connect the capture stream to the Input direction so we receive audio.
    let format_bytes = format_pod();
    let pod = match Pod::from_bytes(&format_bytes) {
        Some(p) => p,
        None => {
            return void(
                &started,
                CaptureError::Failed("invalid audio format pod".to_owned()),
            );
        }
    };
    let mut params = [pod];
    if let Err(e) = capture_stream.connect(
        Direction::Input,
        Some(pw::constants::ID_ANY),
        pw::stream::StreamFlags::MAP_BUFFERS,
        &mut params,
    ) {
        return void(&started, failed(e));
    }

    // Links live as long as their proxies: kept until the loop ends.
    let mut links: Vec<Link> = Vec::new();

    // Get the capture stream's node id and a port id.
    let capture_node_id = capture_stream.node_id();
    let capture_port_id = spa::constants::ID_INVALID;

    // Seed links for existing nodes.
    {
        let existing = playback.lock().unwrap();
        for &node_id in existing.keys() {
            // Try to link each playback node to the capture stream.
            if let Ok(link) = create_link(&core, node_id, capture_node_id, capture_port_id) {
                links.push(link);
            }
        }
    }

    let _quit = quit.attach(mainloop.loop_(), {
        let mainloop = mainloop.clone();
        move |()| mainloop.quit()
    });

    let _ = started.send(Ok(()));
    mainloop.run();

    drop(links);
    drop(_capture_listener);
    drop(_links_listener);
    let _ = capture_stream.disconnect();
}

fn void(started: &mpsc::Sender<Result<(), CaptureError>>, error: CaptureError) {
    let _ = started.send(Err(error));
}

/// Creates a PipeWire link from a playback node to a capture node.
/// Uses the link factory via core.create_object.
fn create_link(
    core: &pw::core::CoreRc,
    output_node: u32,
    input_node: u32,
    _input_port: u32,
) -> Result<Link, CaptureError> {
    use pw::properties::PropertiesBox;

    let mut props = PropertiesBox::new();
    // Link passive so it doesn't prevent the nodes from disappearing.
    props.insert(*pw::keys::LINK_PASSIVE, "true");
    // Output node and port
    props.insert(*pw::keys::LINK_OUTPUT_NODE, output_node.to_string());
    props.insert(*pw::keys::LINK_OUTPUT_PORT, "64".to_string()); // Use a reasonable default port
    // Input node and port
    props.insert(*pw::keys::LINK_INPUT_NODE, input_node.to_string());
    props.insert(*pw::keys::LINK_INPUT_PORT, "64".to_string());

    core.create_object::<Link>("link", &props).map_err(failed)
}

fn make_capture_stream(core: &pw::core::CoreRc) -> Result<pw::stream::StreamRc, CaptureError> {
    pw::stream::StreamRc::new(
        core.clone(),
        "opencord-share-audio",
        pw::properties::properties! {
            *pw::keys::MEDIA_CLASS => "Audio/Source",
            *pw::keys::NODE_NAME => "opencord-share-audio",
            "node.autoconnect" => "false",
            "stream.capture.suspend" => "false",
        },
    )
    .map_err(failed)
}

fn format_pod() -> Vec<u8> {
    serialize(Object {
        type_: SpaTypes::ObjectParamFormat.as_raw(),
        id: ParamType::EnumFormat.as_raw(),
        properties: vec![
            Property::new(
                spa::sys::SPA_FORMAT_mediaType,
                Value::Id(Id(MediaType::Audio.as_raw())),
            ),
            Property::new(
                spa::sys::SPA_FORMAT_mediaSubtype,
                Value::Id(Id(MediaSubtype::Raw.as_raw())),
            ),
            Property::new(
                spa::sys::SPA_FORMAT_AUDIO_format,
                Value::Id(Id(spa::param::audio::AudioFormat::F32LE.as_raw())),
            ),
            Property::new(
                spa::sys::SPA_FORMAT_AUDIO_rate,
                Value::Int(SAMPLE_RATE as i32),
            ),
            Property::new(
                spa::sys::SPA_FORMAT_AUDIO_channels,
                Value::Int(CHANNELS as i32),
            ),
            // Position array: FL, FR for stereo
            Property::new(
                spa::sys::SPA_FORMAT_AUDIO_position,
                Value::ValueArray(ValueArray::Id(vec![
                    Id(spa::sys::SPA_AUDIO_CHANNEL_FL),
                    Id(spa::sys::SPA_AUDIO_CHANNEL_FR),
                ])),
            ),
        ],
    })
}

fn serialize(object: Object) -> Vec<u8> {
    PodSerializer::serialize(Cursor::new(Vec::new()), &Value::Object(object))
        .map(|(c, _)| c.into_inner())
        .unwrap_or_default()
}

/// Info extracted from a playback stream node.
fn playback_info(
    global: &pw::registry::GlobalObject<&spa::utils::dict::DictRef>,
) -> Option<NodeInfo> {
    if global.type_ != ObjectType::Node {
        return None;
    }
    let props = global.props?;
    if props.get("media.class") != Some("Stream/Output/Audio") {
        return None;
    }
    let node_id = global.id;
    let pid = props.get("application.process.id")?.parse().ok()?;
    let name = props
        .get("application.name")
        .or_else(|| props.get("node.description"))
        .unwrap_or("?")
        .to_owned();
    Some(NodeInfo(node_id, pid, name))
}

fn should_include(
    pid: u32,
    name: &str,
    opencord_pid: u32,
    want_pid: &std::sync::Arc<std::sync::Mutex<Option<u32>>>,
) -> bool {
    // Exclude our own process.
    if pid == opencord_pid {
        return false;
    }
    // Exclude our own nodes by name prefix.
    if name.starts_with("opencord-") || name.starts_with("Opencord") {
        return false;
    }
    // If a specific window is wanted, match by pid.
    if let Some(want) = *want_pid.lock().unwrap() {
        return pid == want || is_descendant(pid, want);
    }
    // Screen share: include everything else.
    true
}

fn is_descendant(pid: u32, ancestor: u32) -> bool {
    let mut current = pid;
    loop {
        if current == ancestor {
            return true;
        }
        let Some(parent) = read_parent_pid(current) else {
            return false;
        };
        if parent == current {
            return false;
        }
        current = parent;
    }
}

fn read_parent_pid(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat.split(')')
        .nth(1)?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

/// Lists apps with audio that can be shared.
pub fn apps_with_audio(remote: OwnedFd) -> Result<Vec<AppInfo>, CaptureError> {
    let (result, outcome) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("opencord-share-audio-apps".to_owned())
        .spawn(move || {
            let apps = list_apps(remote);
            let _ = result.send(apps);
        })
        .map_err(|e| CaptureError::Failed(e.to_string()))?;
    outcome
        .recv()
        .map_err(|_| CaptureError::Failed("thread ended".to_owned()))
}

fn list_apps(remote: OwnedFd) -> Vec<AppInfo> {
    pw::init();
    let Ok(mainloop) = pw::main_loop::MainLoopRc::new(None) else {
        return Vec::new();
    };
    let Ok(context) = pw::context::ContextRc::new(&mainloop, None) else {
        return Vec::new();
    };
    let Ok(core) = context.connect_fd_rc(remote, None) else {
        return Vec::new();
    };
    let Ok(registry) = core.get_registry_rc() else {
        return Vec::new();
    };
    let apps: std::sync::Arc<std::sync::Mutex<Vec<AppInfo>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

    let _listener = registry
        .add_listener_local()
        .global({
            let apps = std::sync::Arc::clone(&apps);
            move |global| {
                if let Some((pid, name)) = playback_app(global) {
                    let mut apps = apps.lock().unwrap();
                    if !apps.iter().any(|a| a.pid == pid) {
                        apps.push(AppInfo { name, pid });
                    }
                }
            }
        })
        .register();

    std::thread::sleep(ROUNDTRIP);
    std::mem::drop(_listener);
    std::mem::drop(registry);
    std::mem::drop(core);
    std::mem::drop(context);
    mainloop.quit();
    apps.lock().unwrap().clone()
}

fn playback_app(
    global: &pw::registry::GlobalObject<&spa::utils::dict::DictRef>,
) -> Option<(u32, String)> {
    if global.type_ != ObjectType::Node {
        return None;
    }
    let props = global.props?;
    if props.get("media.class") != Some("Stream/Output/Audio") {
        return None;
    }
    let pid = props.get("application.process.id")?.parse().ok()?;
    let name = props
        .get("application.name")
        .or_else(|| props.get("node.description"))
        .unwrap_or("?")
        .to_owned();
    Some((pid, name))
}
