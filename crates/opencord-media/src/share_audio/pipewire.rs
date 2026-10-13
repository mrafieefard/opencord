//! PipeWire screen audio capture (Phase 2 plan §10.4): a capture stream of
//! our own, not connected to anything by the session manager, and links
//! from the playback streams being shared into it. An entire screen shares
//! every app's playback except Opencord's own (its process and children,
//! and nodes named `opencord-…`); a window shares only its app's. Streams
//! that start while sharing are linked as they appear; ones that end take
//! their links with them. Devices (microphones, monitors) are never
//! linked: only application playback streams.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::rc::Rc;
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use pipewire as pw;
use pw::link::Link;
use pw::spa;
use pw::types::ObjectType;
use spa::param::ParamType;
use spa::param::format::{MediaSubtype, MediaType};
use spa::pod::serialize::PodSerializer;
use spa::pod::{Object, Pod, Property, Value, ValueArray};
use spa::utils::{Direction, Id, SpaTypes};

use super::CaptureError;

/// The capture sample rate.
pub const SAMPLE_RATE: u32 = 48_000;
/// Stereo.
pub const CHANNELS: usize = 2;
/// Our capture node's name; nodes named like it are never shared.
const NODE_NAME: &str = "opencord-screen-audio";

/// How long we wait for PipeWire to answer.
const ROUNDTRIP: Duration = Duration::from_secs(5);

/// What is shared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// Every app's sound but Opencord's: an entire screen.
    Everything,
    /// One app's: every playback stream it names itself by
    /// (`application.name`), from any of its processes.
    App(String),
    /// Every playback stream of a process and its children: a window
    /// whose process is known (X11's `_NET_WM_PID`).
    Process(u32),
    /// Nothing yet: a window share before its app is chosen.
    Nothing,
}

/// An app with a playback stream open, which a window share can share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppInfo {
    /// As it names itself (`application.name`).
    pub name: String,
    /// Its icon's name in the desktop's icon theme, when it gives one.
    pub icon: Option<String>,
    pub pid: Option<u32>,
}

/// What was captured.
#[derive(Debug, Clone)]
pub struct Captured {
    /// Interleaved stereo at 48 kHz.
    pub samples: Vec<f32>,
    /// The first frame's position on this capture's 48 kHz clock.
    pub position: u64,
    /// When the first frame was captured.
    pub captured: Instant,
}

/// A running capture; stops when dropped.
pub struct Capture {
    commands: pw::channel::Sender<Command>,
    thread: Option<JoinHandle<()>>,
    /// The apps linked now, by name.
    linked: Arc<Mutex<Vec<String>>>,
}

enum Command {
    Share(Source),
    Quit,
}

impl Capture {
    /// Captures `source` from the session's PipeWire. Fails when there is
    /// none (a PulseAudio-only system).
    pub fn start(source: Source, frames: SyncSender<Captured>) -> Result<Self, CaptureError> {
        let (started, outcome) = mpsc::channel();
        let (commands, received) = pw::channel::channel();
        let linked = Arc::new(Mutex::new(Vec::new()));
        let shown = Arc::clone(&linked);
        let thread = std::thread::Builder::new()
            .name("opencord-share-audio".to_owned())
            .spawn(move || {
                if let Err(error) = serve(source, frames, received, &started, shown) {
                    let _ = started.send(Err(error));
                }
            })
            .map_err(|error| CaptureError::Failed(error.to_string()))?;
        let failed = match outcome.recv_timeout(ROUNDTRIP) {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(error),
            Err(_) => Some(CaptureError::Failed("PipeWire did not answer".to_owned())),
        };
        let capture = Self {
            commands,
            thread: Some(thread),
            linked,
        };
        match failed {
            None => Ok(capture),
            Some(error) => Err(error),
        }
    }

    /// Shares another source from now: links that no longer belong go,
    /// new ones come.
    pub fn share(&self, source: Source) {
        let _ = self.commands.send(Command::Share(source));
    }

    /// The apps whose sound is linked in now.
    pub fn linked_apps(&self) -> Vec<String> {
        self.linked
            .lock()
            .map(|apps| apps.clone())
            .unwrap_or_default()
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Quit);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn failed(error: impl std::fmt::Display) -> CaptureError {
    CaptureError::Failed(error.to_string())
}

/// A process with PipeWire objects: its id and name.
#[derive(Debug, Clone, Default)]
struct Client {
    pid: Option<u32>,
    name: Option<String>,
    icon: Option<String>,
}

/// A playback stream.
#[derive(Debug, Clone)]
struct Playback {
    client: Option<u32>,
    pid: Option<u32>,
    name: Option<String>,
    icon: Option<String>,
    node_name: String,
}

#[derive(Debug, Clone)]
struct Port {
    node: u32,
    output: bool,
    channel: String,
}

/// What the registry has shown, and the links made from it.
#[derive(Default)]
struct Graph {
    clients: HashMap<u32, Client>,
    playbacks: HashMap<u32, Playback>,
    ports: HashMap<u32, Port>,
    /// Ours, once PipeWire has made it.
    capture_node: Option<u32>,
    /// Links by output and input port.
    links: HashMap<(u32, u32), Link>,
}

impl Graph {
    /// A playback stream's process and app name: its own properties, else
    /// its client's (native PipeWire clients put them there).
    fn owner(&self, playback: &Playback) -> (Option<u32>, Option<String>, Option<String>) {
        let client = playback.client.and_then(|id| self.clients.get(&id));
        (
            playback.pid.or(client.and_then(|c| c.pid)),
            playback
                .name
                .clone()
                .or_else(|| client.and_then(|c| c.name.clone())),
            playback
                .icon
                .clone()
                .or_else(|| client.and_then(|c| c.icon.clone())),
        )
    }

    /// Whether `playback` is to be shared.
    fn shares(&self, playback: &Playback, source: &Source, opencord: u32) -> bool {
        let (pid, name, _) = self.owner(playback);
        let ours = playback.node_name.starts_with("opencord-")
            || pid.is_some_and(|pid| pid == opencord || is_descendant(pid, opencord));
        if ours {
            return false;
        }
        match source {
            Source::Everything => true,
            Source::App(app) => name.as_deref() == Some(app.as_str()),
            Source::Process(root) => {
                pid.is_some_and(|pid| pid == *root || is_descendant(pid, *root))
            }
            Source::Nothing => false,
        }
    }

    /// The links `source` needs now: each shared stream's output ports to
    /// our input ports, channel by channel.
    fn wanted(&self, source: &Source, opencord: u32) -> HashSet<(u32, u32)> {
        let Some(capture) = self.capture_node else {
            return HashSet::new();
        };
        let inputs: HashMap<&str, u32> = self
            .ports
            .iter()
            .filter(|(_, port)| port.node == capture && !port.output)
            .map(|(id, port)| (port.channel.as_str(), *id))
            .collect();
        let mut wanted = HashSet::new();
        for (node, playback) in &self.playbacks {
            if !self.shares(playback, source, opencord) {
                continue;
            }
            for (id, port) in &self.ports {
                if port.node != *node || !port.output {
                    continue;
                }
                for channel in into_channels(&port.channel) {
                    if let Some(input) = inputs.get(channel) {
                        wanted.insert((*id, *input));
                    }
                }
            }
        }
        wanted
    }

    /// The apps `source` shares now, for showing.
    fn shared_apps(&self, source: &Source, opencord: u32) -> Vec<String> {
        let mut apps: Vec<String> = self
            .playbacks
            .values()
            .filter(|playback| self.shares(playback, source, opencord))
            .map(|playback| {
                self.owner(playback)
                    .1
                    .unwrap_or_else(|| playback.node_name.clone())
            })
            .collect();
        apps.sort();
        apps.dedup();
        apps
    }
}

/// Which of our two channels an output port's channel goes to: stereo
/// stays apart; mono and centre go to both; unpositioned ports count as
/// left then right; surround's rear and LFE are left out (plan §10.4:
/// multichannel → front left and right).
fn into_channels(channel: &str) -> &'static [&'static str] {
    match channel {
        "FL" | "AUX0" => &["FL"],
        "FR" | "AUX1" => &["FR"],
        "MONO" | "FC" => &["FL", "FR"],
        _ => &[],
    }
}

fn serve(
    source: Source,
    frames: SyncSender<Captured>,
    commands: pw::channel::Receiver<Command>,
    started: &mpsc::Sender<Result<(), CaptureError>>,
    linked: Arc<Mutex<Vec<String>>>,
) -> Result<(), CaptureError> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(failed)?;
    let context = pw::context::ContextRc::new(&mainloop, None).map_err(failed)?;
    let core = context
        .connect_rc(None)
        .map_err(|_| CaptureError::NotSupported)?;
    let registry = core.get_registry_rc().map_err(failed)?;
    let opencord = std::process::id();
    let graph = Rc::new(RefCell::new(Graph::default()));
    let source = Rc::new(RefCell::new(source));

    let relink = {
        let graph = Rc::clone(&graph);
        let source = Rc::clone(&source);
        let core = core.clone();
        move || {
            let mut graph = graph.borrow_mut();
            let source = source.borrow();
            let wanted = graph.wanted(&source, opencord);
            graph.links.retain(|key, _| wanted.contains(key));
            for (output, input) in wanted {
                if graph.links.contains_key(&(output, input)) {
                    continue;
                }
                let (Some(from), Some(capture)) = (graph.ports.get(&output), graph.capture_node)
                else {
                    continue;
                };
                match make_link(&core, from.node, output, capture, input) {
                    Ok(link) => {
                        graph.links.insert((output, input), link);
                    }
                    Err(error) => tracing::debug!(%error, "a screen audio link failed"),
                }
            }
            if let Ok(mut shown) = linked.lock() {
                *shown = graph.shared_apps(&source, opencord);
            }
        }
    };
    let relink = Rc::new(relink);

    let stream = pw::stream::StreamRc::new(
        core.clone(),
        NODE_NAME,
        pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Audio",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_CLASS => "Stream/Input/Audio",
            *pw::keys::NODE_NAME => NODE_NAME,
            *pw::keys::NODE_DESCRIPTION => "Opencord screen share sound",
            *pw::keys::APP_NAME => "Opencord",
            // Linked by hand: the session manager must not connect the
            // microphone, or anything, to it.
            "node.autoconnect" => "false",
            "node.dont-reconnect" => "true",
            "stream.dont-remix" => "true",
        },
    )
    .map_err(failed)?;

    let _registry_listener = registry
        .add_listener_local()
        .global({
            let graph = Rc::clone(&graph);
            let relink = Rc::clone(&relink);
            move |global| {
                if remember(&mut graph.borrow_mut(), global) {
                    relink();
                }
            }
        })
        .global_remove({
            let graph = Rc::clone(&graph);
            let relink = Rc::clone(&relink);
            move |id| {
                let mut graph = graph.borrow_mut();
                let known = graph.clients.remove(&id).is_some()
                    | graph.playbacks.remove(&id).is_some()
                    | graph.ports.remove(&id).is_some();
                // Its links went with it.
                graph
                    .links
                    .retain(|(output, input), _| *output != id && *input != id);
                drop(graph);
                if known {
                    relink();
                }
            }
        })
        .register();

    let _stream_listener = stream
        .add_local_listener_with_user_data(0u64)
        .state_changed({
            let graph = Rc::clone(&graph);
            let relink = Rc::clone(&relink);
            move |stream, _, _, state| {
                if matches!(
                    state,
                    pw::stream::StreamState::Paused | pw::stream::StreamState::Streaming
                ) {
                    let node = stream.node_id();
                    let known = graph.borrow().capture_node == Some(node);
                    if !known && node != spa::constants::ID_INVALID {
                        graph.borrow_mut().capture_node = Some(node);
                        relink();
                    }
                }
            }
        })
        .process(move |stream, position| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let Some(data) = buffer.datas_mut().first_mut() else {
                return;
            };
            let chunk = data.chunk();
            let offset = chunk.offset() as usize;
            let size = chunk.size() as usize;
            let stride = (chunk.stride() as usize).max(CHANNELS * 4);
            let Some(bytes) = data.data() else {
                return;
            };
            let Some(bytes) = bytes.get(offset..offset + size) else {
                return;
            };
            let frames_count = size / stride;
            if frames_count == 0 {
                return;
            }
            let samples: Vec<f32> = bytes
                .chunks_exact(stride)
                .flat_map(|frame| {
                    (0..CHANNELS).map(move |channel| {
                        let at = channel * 4;
                        f32::from_le_bytes([frame[at], frame[at + 1], frame[at + 2], frame[at + 3]])
                    })
                })
                .collect();
            let length = Duration::from_secs_f64(frames_count as f64 / f64::from(SAMPLE_RATE));
            let captured = Captured {
                samples,
                position: *position,
                captured: Instant::now()
                    .checked_sub(length)
                    .unwrap_or_else(Instant::now),
            };
            *position += frames_count as u64;
            let _ = frames.try_send(captured);
        })
        .register()
        .map_err(failed)?;

    let format = format_pod();
    let pod = Pod::from_bytes(&format)
        .ok_or_else(|| CaptureError::Failed("the audio format did not serialize".to_owned()))?;
    stream
        .connect(
            Direction::Input,
            None,
            pw::stream::StreamFlags::MAP_BUFFERS | pw::stream::StreamFlags::RT_PROCESS,
            &mut [pod],
        )
        .map_err(failed)?;

    let _commands = commands.attach(mainloop.loop_(), {
        let mainloop = mainloop.clone();
        let source = Rc::clone(&source);
        let relink = Rc::clone(&relink);
        move |command| match command {
            Command::Share(new) => {
                *source.borrow_mut() = new;
                relink();
            }
            Command::Quit => mainloop.quit(),
        }
    });

    let _ = started.send(Ok(()));
    mainloop.run();

    graph.borrow_mut().links.clear();
    let _ = stream.disconnect();
    Ok(())
}

/// Keeps what matters of a new registry object; whether it might change
/// the links.
fn remember(
    graph: &mut Graph,
    global: &pw::registry::GlobalObject<&spa::utils::dict::DictRef>,
) -> bool {
    let Some(props) = global.props else {
        return false;
    };
    let number = |key: &str| props.get(key).and_then(|value| value.parse::<u32>().ok());
    let text = |key: &str| props.get(key).map(str::to_owned);
    match global.type_ {
        ObjectType::Client => {
            graph.clients.insert(
                global.id,
                Client {
                    // The global's properties hold the pid PipeWire took
                    // from the socket; the client's own claim may be there
                    // too.
                    pid: number("application.process.id").or_else(|| number("pipewire.sec.pid")),
                    name: text("application.name"),
                    icon: text("application.icon-name"),
                },
            );
            false
        }
        ObjectType::Node if props.get("media.class") == Some("Stream/Output/Audio") => {
            graph.playbacks.insert(
                global.id,
                Playback {
                    client: number("client.id"),
                    pid: number("application.process.id"),
                    name: text("application.name"),
                    icon: text("application.icon-name").or_else(|| text("media.icon-name")),
                    node_name: text("node.name").unwrap_or_default(),
                },
            );
            true
        }
        ObjectType::Port => {
            let (Some(node), Some(direction)) = (number("node.id"), props.get("port.direction"))
            else {
                return false;
            };
            // A playback stream's monitor ports are its output again.
            if props.get("port.monitor") == Some("true") {
                return false;
            }
            graph.ports.insert(
                global.id,
                Port {
                    node,
                    output: direction == "out",
                    channel: text("audio.channel").unwrap_or_default(),
                },
            );
            true
        }
        _ => false,
    }
}

fn make_link(
    core: &pw::core::CoreRc,
    output_node: u32,
    output_port: u32,
    input_node: u32,
    input_port: u32,
) -> Result<Link, pw::Error> {
    core.create_object::<Link>(
        "link-factory",
        &pw::properties::properties! {
            "link.output.node" => output_node.to_string(),
            "link.output.port" => output_port.to_string(),
            "link.input.node" => input_node.to_string(),
            "link.input.port" => input_port.to_string(),
            // Gone with its proxy: when the share ends, or Opencord does.
            "object.linger" => "false",
        },
    )
}

fn format_pod() -> Vec<u8> {
    let object = Object {
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
            Property::new(
                spa::sys::SPA_FORMAT_AUDIO_position,
                Value::ValueArray(ValueArray::Id(vec![
                    Id(spa::sys::SPA_AUDIO_CHANNEL_FL),
                    Id(spa::sys::SPA_AUDIO_CHANNEL_FR),
                ])),
            ),
        ],
    };
    PodSerializer::serialize(Cursor::new(Vec::new()), &Value::Object(object))
        .map(|(cursor, _)| cursor.into_inner())
        .unwrap_or_default()
}

/// Whether `pid` descends from `ancestor`, by the parent chain in /proc.
fn is_descendant(pid: u32, ancestor: u32) -> bool {
    let mut current = pid;
    for _ in 0..64 {
        let Some(parent) = parent_of(current) else {
            return false;
        };
        if parent == ancestor {
            return true;
        }
        if parent <= 1 || parent == current {
            return false;
        }
        current = parent;
    }
    false
}

fn parent_of(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The name in parentheses may hold spaces; the fields after it do not.
    stat.rsplit_once(')')?
        .1
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

/// Whether the session has PipeWire: screen sound needs it.
pub fn available() -> bool {
    std::thread::spawn(|| {
        pw::init();
        let Ok(mainloop) = pw::main_loop::MainLoopRc::new(None) else {
            return false;
        };
        let Ok(context) = pw::context::ContextRc::new(&mainloop, None) else {
            return false;
        };
        context.connect_rc(None).is_ok()
    })
    .join()
    .unwrap_or(false)
}

/// The apps with a playback stream open now, Opencord left out, each once.
pub fn apps_with_audio() -> Result<Vec<AppInfo>, CaptureError> {
    std::thread::Builder::new()
        .name("opencord-share-audio-apps".to_owned())
        .spawn(list_apps)
        .map_err(failed)?
        .join()
        .map_err(|_| CaptureError::Failed("listing apps failed".to_owned()))?
}

fn list_apps() -> Result<Vec<AppInfo>, CaptureError> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(failed)?;
    let context = pw::context::ContextRc::new(&mainloop, None).map_err(failed)?;
    let core = context
        .connect_rc(None)
        .map_err(|_| CaptureError::NotSupported)?;
    let registry = core.get_registry_rc().map_err(failed)?;
    let graph = Rc::new(RefCell::new(Graph::default()));
    let _listener = registry
        .add_listener_local()
        .global({
            let graph = Rc::clone(&graph);
            move |global| {
                remember(&mut graph.borrow_mut(), global);
            }
        })
        .register();
    let pending = core.sync(0).map_err(failed)?;
    let done = Rc::new(std::cell::Cell::new(false));
    let _core_listener = core
        .add_listener_local()
        .done({
            let done = Rc::clone(&done);
            let mainloop = mainloop.clone();
            move |id, seq| {
                if id == pw::core::PW_ID_CORE && seq == pending {
                    done.set(true);
                    mainloop.quit();
                }
            }
        })
        .register();
    let deadline = Instant::now() + ROUNDTRIP;
    while !done.get() && Instant::now() < deadline {
        mainloop
            .loop_()
            .iterate(pw::loop_::Timeout::Finite(Duration::from_millis(50)));
    }
    let graph = graph.borrow();
    let opencord = std::process::id();
    let mut apps: Vec<AppInfo> = Vec::new();
    for playback in graph.playbacks.values() {
        if !graph.shares(playback, &Source::Everything, opencord) {
            continue;
        }
        let (pid, name, icon) = graph.owner(playback);
        let Some(name) = name else {
            continue;
        };
        if !apps.iter().any(|app| app.name == name) {
            apps.push(AppInfo { name, icon, pid });
        }
    }
    apps.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(apps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_go_to_the_front_pair() {
        assert_eq!(into_channels("FL"), ["FL"]);
        assert_eq!(into_channels("FR"), ["FR"]);
        assert_eq!(into_channels("MONO"), ["FL", "FR"]);
        assert_eq!(into_channels("FC"), ["FL", "FR"]);
        assert_eq!(into_channels("AUX1"), ["FR"]);
        assert!(into_channels("RL").is_empty());
        assert!(into_channels("LFE").is_empty());
    }

    #[test]
    fn this_process_descends_from_its_parent_but_not_from_a_stranger() {
        let me = std::process::id();
        let parent = parent_of(me).unwrap();

        assert!(is_descendant(me, parent));
        assert!(!is_descendant(parent, me));
        assert!(!is_descendant(me, u32::MAX));
    }

    fn graph() -> Graph {
        let mut graph = Graph::default();
        graph.clients.insert(
            7,
            Client {
                pid: Some(4242),
                name: Some("Player".to_owned()),
                icon: None,
            },
        );
        let playback = |client, pid, name: Option<&str>, node: &str| Playback {
            client,
            pid,
            name: name.map(str::to_owned),
            icon: None,
            node_name: node.to_owned(),
        };
        graph
            .playbacks
            .insert(10, playback(Some(7), None, None, "player"));
        graph.playbacks.insert(
            11,
            playback(None, Some(std::process::id()), Some("Opencord"), "voice"),
        );
        graph
            .playbacks
            .insert(12, playback(None, Some(5151), Some("Browser"), "tab"));
        graph
            .playbacks
            .insert(13, playback(None, None, Some("Tool"), "opencord-preview"));
        graph.capture_node = Some(99);
        let port = |node, output, channel: &str| Port {
            node,
            output,
            channel: channel.to_owned(),
        };
        graph.ports.insert(100, port(10, true, "FL"));
        graph.ports.insert(101, port(10, true, "FR"));
        graph.ports.insert(110, port(11, true, "MONO"));
        graph.ports.insert(120, port(12, true, "MONO"));
        graph.ports.insert(130, port(13, true, "FL"));
        graph.ports.insert(990, port(99, false, "FL"));
        graph.ports.insert(991, port(99, false, "FR"));
        graph
    }

    #[test]
    fn a_screen_links_every_app_but_opencord_channel_by_channel() {
        let wanted = graph().wanted(&Source::Everything, std::process::id());

        assert_eq!(
            wanted,
            HashSet::from([(100, 990), (101, 991), (120, 990), (120, 991)])
        );
    }

    #[test]
    fn a_window_links_only_its_app() {
        let opencord = std::process::id();
        let graph = graph();

        // The app name comes from the client when the node has none.
        let player = graph.wanted(&Source::App("Player".to_owned()), opencord);
        let browser = graph.wanted(&Source::Process(5151), opencord);
        let nothing = graph.wanted(&Source::Nothing, opencord);
        let opencord_by_name = graph.wanted(&Source::App("Opencord".to_owned()), opencord);

        assert_eq!(player, HashSet::from([(100, 990), (101, 991)]));
        assert_eq!(browser, HashSet::from([(120, 990), (120, 991)]));
        assert!(nothing.is_empty());
        assert!(opencord_by_name.is_empty(), "never Opencord's own");
        assert_eq!(
            graph.shared_apps(&Source::Everything, opencord),
            ["Browser", "Player"]
        );
    }

    #[test]
    fn nothing_links_before_the_capture_node_exists() {
        let mut graph = graph();
        graph.capture_node = None;

        assert!(graph.wanted(&Source::Everything, 1).is_empty());
    }
}
