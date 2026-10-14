//! Linux: every other app's playback stream on PipeWire (a
//! `Stream/Output/Audio` node not of this process or its children) gets its
//! channel volumes scaled down, and put back as they were. A stream that
//! starts while others are lowered is lowered too. Someone moving a lowered
//! stream's volume moves what it goes back to.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Cursor;
use std::rc::Rc;
use std::sync::mpsc;
use std::thread::JoinHandle;

use pipewire as pw;
use pw::spa;
use pw::types::ObjectType;
use spa::param::ParamType;
use spa::pod::deserialize::PodDeserializer;
use spa::pod::serialize::PodSerializer;
use spa::pod::{Object, Pod, Property, Value, ValueArray};
use spa::utils::SpaTypes;

use crate::share_audio::pipewire::is_descendant;

pub const SUPPORTED: bool = true;

enum Command {
    Set(Option<f32>),
    Stop,
}

pub struct Attenuator {
    commands: pw::channel::Sender<Command>,
    thread: Option<JoinHandle<()>>,
}

impl Attenuator {
    pub fn start() -> Option<Self> {
        let (commands, receiver) = pw::channel::channel();
        let (started, ready) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("opencord-attenuation".to_owned())
            .spawn(move || {
                if let Err(error) = serve(receiver, &started) {
                    tracing::debug!(%error, "other apps cannot be lowered");
                    let _ = started.send(false);
                }
            })
            .ok()?;
        if ready.recv().unwrap_or(false) {
            Some(Self {
                commands,
                thread: Some(thread),
            })
        } else {
            let _ = thread.join();
            None
        }
    }

    pub fn set(&self, level: Option<f32>) {
        let _ = self.commands.send(Command::Set(level));
    }
}

impl Drop for Attenuator {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Another app's playback stream.
struct Stream {
    node: pw::node::Node,
    _listener: pw::node::NodeListener,
    /// Its volumes as it last said.
    volumes: Option<Vec<f32>>,
    /// What it goes back to, while lowered.
    original: Option<Vec<f32>>,
}

#[derive(Default)]
struct State {
    streams: HashMap<u32, Stream>,
    level: Option<f32>,
}

fn serve(
    commands: pw::channel::Receiver<Command>,
    started: &mpsc::Sender<bool>,
) -> Result<(), pw::Error> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let registry = core.get_registry_rc()?;
    let state = Rc::new(RefCell::new(State::default()));
    let ours = std::process::id();

    let _registry_listener = registry
        .add_listener_local()
        .global({
            let state = Rc::clone(&state);
            let registry = registry.clone();
            move |global| {
                if global.type_ != ObjectType::Node {
                    return;
                }
                let Some(props) = global.props else {
                    return;
                };
                if props.get("media.class") != Some("Stream/Output/Audio") {
                    return;
                }
                let pid = props
                    .get("application.process.id")
                    .and_then(|pid| pid.parse::<u32>().ok());
                if pid.is_some_and(|pid| pid == ours || is_descendant(pid, ours)) {
                    return;
                }
                let Ok(node) = registry.bind::<pw::node::Node, _>(global) else {
                    return;
                };
                let id = global.id;
                let listener = node
                    .add_listener_local()
                    .param({
                        let state = Rc::clone(&state);
                        move |_, kind, _, _, pod| {
                            if kind != ParamType::Props {
                                return;
                            }
                            if let Some(volumes) = pod.and_then(channel_volumes) {
                                reported(&mut state.borrow_mut(), id, volumes);
                            }
                        }
                    })
                    .register();
                node.subscribe_params(&[ParamType::Props]);
                state.borrow_mut().streams.insert(
                    id,
                    Stream {
                        node,
                        _listener: listener,
                        volumes: None,
                        original: None,
                    },
                );
            }
        })
        .global_remove({
            let state = Rc::clone(&state);
            move |id| {
                state.borrow_mut().streams.remove(&id);
            }
        })
        .register();

    // Stopping puts everything back, then waits for the server to have it
    // all before leaving.
    let stopping = Rc::new(RefCell::new(None::<i32>));
    let _core_listener = core
        .add_listener_local()
        .done({
            let stopping = Rc::clone(&stopping);
            let mainloop = mainloop.clone();
            move |_, seq| {
                if *stopping.borrow() == Some(seq.seq()) {
                    mainloop.quit();
                }
            }
        })
        .register();
    let _commands = commands.attach(mainloop.loop_(), {
        let state = Rc::clone(&state);
        let core = core.clone();
        move |command| match command {
            Command::Set(level) => lower(&mut state.borrow_mut(), level),
            Command::Stop => {
                lower(&mut state.borrow_mut(), None);
                match core.sync(0) {
                    Ok(seq) => *stopping.borrow_mut() = Some(seq.seq()),
                    Err(_) => *stopping.borrow_mut() = Some(i32::MIN),
                }
            }
        }
    });
    let _ = started.send(true);
    mainloop.run();
    Ok(())
}

/// Lowers every stream to `level`, or puts them back.
fn lower(state: &mut State, level: Option<f32>) {
    state.level = level;
    for stream in state.streams.values_mut() {
        match level {
            Some(level) => {
                let Some(base) = stream.original.clone().or_else(|| stream.volumes.clone()) else {
                    continue;
                };
                set_volumes(&stream.node, &scaled(&base, level));
                stream.original = Some(base);
            }
            None => {
                if let Some(original) = stream.original.take() {
                    set_volumes(&stream.node, &original);
                }
            }
        }
    }
}

/// A stream said what its volumes are: on first sight, after our own
/// change, or after someone else's.
fn reported(state: &mut State, id: u32, volumes: Vec<f32>) {
    let level = state.level;
    let Some(stream) = state.streams.get_mut(&id) else {
        return;
    };
    match (level, &stream.original) {
        // New while others are lowered: lower it too.
        (Some(level), None) => {
            set_volumes(&stream.node, &scaled(&volumes, level));
            stream.original = Some(volumes.clone());
        }
        // Not what was set: someone moved it, so it goes back to what that
        // means at full volume.
        (Some(level), Some(original)) => {
            if !close(&volumes, &scaled(original, level)) && level >= 0.05 {
                stream.original = Some(volumes.iter().map(|volume| volume / level).collect());
            }
        }
        (None, _) => {}
    }
    stream.volumes = Some(volumes);
}

fn scaled(volumes: &[f32], level: f32) -> Vec<f32> {
    volumes.iter().map(|volume| volume * level).collect()
}

fn close(a: &[f32], b: &[f32]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-3)
}

/// A Props param's `channelVolumes`.
fn channel_volumes(pod: &Pod) -> Option<Vec<f32>> {
    let (_, Value::Object(object)) = PodDeserializer::deserialize_any_from(pod.as_bytes()).ok()?
    else {
        return None;
    };
    object
        .properties
        .into_iter()
        .find(|property| property.key == spa::sys::SPA_PROP_channelVolumes)
        .and_then(|property| match property.value {
            Value::ValueArray(ValueArray::Float(volumes)) => Some(volumes),
            _ => None,
        })
}

fn props_pod(volumes: &[f32]) -> Vec<u8> {
    let object = Object {
        type_: SpaTypes::ObjectParamProps.as_raw(),
        id: ParamType::Props.as_raw(),
        properties: vec![Property::new(
            spa::sys::SPA_PROP_channelVolumes,
            Value::ValueArray(ValueArray::Float(volumes.to_vec())),
        )],
    };
    PodSerializer::serialize(Cursor::new(Vec::new()), &Value::Object(object))
        .map(|(cursor, _)| cursor.into_inner())
        .unwrap_or_default()
}

fn set_volumes(node: &pw::node::Node, volumes: &[f32]) {
    let bytes = props_pod(volumes);
    if let Some(pod) = Pod::from_bytes(&bytes) {
        node.set_param(ParamType::Props, 0, pod);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volumes_go_into_a_props_param_and_come_back_out() {
        let bytes = props_pod(&[0.5, 0.25]);
        let pod = Pod::from_bytes(&bytes).unwrap();

        assert_eq!(channel_volumes(pod), Some(vec![0.5, 0.25]));
    }
}
