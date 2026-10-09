//! Cameras through PipeWire (plan §8). The Camera portal grants access and
//! hands over a PipeWire remote that shows only camera nodes; without the
//! portal (tests, systems that lack it) PipeWire's own socket is used.
//! Each capture runs PipeWire's loop on a thread of its own and hands
//! frames over a short queue, dropping them while the encoder is behind.

use std::cell::{Cell, RefCell};
use std::io::Cursor;
use std::os::fd::OwnedFd;
use std::rc::Rc;
use std::sync::mpsc::{self, SyncSender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use pipewire as pw;
use pw::spa;
use pw::types::ObjectType;
use spa::param::ParamType;
use spa::param::format::{FormatProperties, MediaSubtype, MediaType};
use spa::param::video::VideoFormat;
use spa::pod::deserialize::PodDeserializer;
use spa::pod::serialize::PodSerializer;
use spa::pod::{ChoiceValue, Object, Pod, Property, Value};
use spa::utils::{ChoiceEnum, Direction, Fraction, Id, Rectangle, SpaTypes};

use super::mode::{self, Mode};
use super::{CameraError, CameraInfo, RawFormat, RawFrame, pack};

/// Camera ids from PipeWire start with this, then the node's name.
pub(super) const ID_PREFIX: &str = "pipewire:";
/// The most a question to PipeWire may take.
pub(crate) const ROUNDTRIP: Duration = Duration::from_secs(5);

/// Where cameras are found.
pub enum Remote {
    /// The Camera portal's remote, once access was granted.
    Portal(OwnedFd),
    /// PipeWire's own socket.
    Session,
}

/// Asks the Camera portal for access (it may ask the user) and opens its
/// PipeWire remote.
pub async fn portal_remote() -> Result<OwnedFd, CameraError> {
    use ashpd::desktop::camera::Camera;

    let failed = |error: ashpd::Error| CameraError::Failed(error.to_string());
    let camera = Camera::new().await.map_err(failed)?;
    if !camera.is_present().await.map_err(failed)? {
        return Err(CameraError::NoCamera);
    }
    let request = camera
        .request_access(Default::default())
        .await
        .map_err(failed)?;
    match request.response() {
        Ok(()) => {}
        Err(ashpd::Error::Response(_)) => return Err(CameraError::Denied),
        Err(error) => return Err(failed(error)),
    }
    camera
        .open_pipe_wire_remote(Default::default())
        .await
        .map_err(failed)
}

fn failed(error: pw::Error) -> CameraError {
    CameraError::Failed(error.to_string())
}

/// Which camera node to bind while listing them.
#[derive(Clone, Copy)]
enum Bind<'a> {
    Nothing,
    First,
    Named(&'a str),
}

/// A camera node's id and a proxy bound to it.
type Bound = (u32, pw::node::Node);

/// A camera node.
#[derive(Debug, Clone)]
struct Node {
    name: String,
    nick: String,
}

impl Node {
    fn info(&self) -> CameraInfo {
        CameraInfo {
            id: format!("{ID_PREFIX}{}", self.name),
            name: self.nick.clone(),
        }
    }
}

/// A connection to PipeWire, on the thread that made it.
struct Session {
    mainloop: pw::main_loop::MainLoopRc,
    _context: pw::context::ContextRc,
    core: pw::core::CoreRc,
    registry: pw::registry::RegistryRc,
}

impl Session {
    fn connect(remote: Remote) -> Result<Self, CameraError> {
        pw::init();
        let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(failed)?;
        let context = pw::context::ContextRc::new(&mainloop, None).map_err(failed)?;
        let core = match remote {
            Remote::Portal(fd) => context.connect_fd_rc(fd, None),
            Remote::Session => context.connect_rc(None),
        }
        .map_err(failed)?;
        let registry = core.get_registry_rc().map_err(failed)?;
        Ok(Self {
            mainloop,
            _context: context,
            core,
            registry,
        })
    }

    /// Runs the loop until PipeWire has answered everything asked so far.
    fn roundtrip(&self) -> Result<(), CameraError> {
        let pending = self.core.sync(0).map_err(failed)?;
        let done = Rc::new(Cell::new(false));
        let error = Rc::new(RefCell::new(None));
        let _listener = self
            .core
            .add_listener_local()
            .done({
                let (mainloop, done) = (self.mainloop.clone(), Rc::clone(&done));
                move |id, seq| {
                    if id == pw::core::PW_ID_CORE && seq == pending {
                        done.set(true);
                        mainloop.quit();
                    }
                }
            })
            .error({
                let (mainloop, error) = (self.mainloop.clone(), Rc::clone(&error));
                move |_, _, _, message| {
                    *error.borrow_mut() = Some(message.to_owned());
                    mainloop.quit();
                }
            })
            .register();
        let timer = self.mainloop.loop_().add_timer({
            let mainloop = self.mainloop.clone();
            move |_| mainloop.quit()
        });
        timer.update_timer(Some(ROUNDTRIP), None);
        self.mainloop.run();
        if let Some(message) = error.take() {
            return Err(CameraError::Failed(message));
        }
        if !done.get() {
            return Err(CameraError::Failed("PipeWire did not answer".to_owned()));
        }
        Ok(())
    }

    /// The camera nodes, and a proxy bound to the one asked for.
    fn cameras(&self, bind: Bind<'_>) -> Result<(Vec<Node>, Option<Bound>), CameraError> {
        let found = Rc::new(RefCell::new(Vec::new()));
        let bound = Rc::new(RefCell::new(None));
        let _listener = self
            .registry
            .add_listener_local()
            .global({
                let (found, bound, registry) =
                    (Rc::clone(&found), Rc::clone(&bound), self.registry.clone());
                let wanted = match bind {
                    Bind::Nothing => None,
                    Bind::First => Some(None),
                    Bind::Named(name) => Some(Some(name.to_owned())),
                };
                move |global| {
                    if global.type_ != ObjectType::Node {
                        return;
                    }
                    let Some(props) = global.props else {
                        return;
                    };
                    let camera = props.get("media.class") == Some("Video/Source")
                        && props.get("media.role") == Some("Camera");
                    let Some(name) = props.get("node.name").filter(|_| camera) else {
                        return;
                    };
                    let nick = props
                        .get("node.nick")
                        .or_else(|| props.get("node.description"))
                        .unwrap_or(name);
                    found.borrow_mut().push(Node {
                        name: name.to_owned(),
                        nick: nick.to_owned(),
                    });
                    let chosen = wanted.as_ref().is_some_and(|wanted| {
                        wanted.as_deref().is_none_or(|wanted| wanted == name)
                    });
                    if chosen && bound.borrow().is_none() {
                        match registry.bind::<pw::node::Node, _>(global) {
                            Ok(node) => *bound.borrow_mut() = Some((global.id, node)),
                            Err(error) => tracing::debug!(%error, "a camera could not be bound"),
                        }
                    }
                }
            })
            .register();
        self.roundtrip()?;
        let nodes = found.take();
        let bound = bound.take();
        Ok((nodes, bound))
    }

    /// The modes a camera node offers.
    fn modes(&self, node: &pw::node::Node) -> Result<Vec<Mode>, CameraError> {
        let modes = Rc::new(RefCell::new(Vec::new()));
        let _listener = node
            .add_listener_local()
            .param({
                let modes = Rc::clone(&modes);
                move |_, _, _, _, param| {
                    if let Some(pod) = param {
                        modes.borrow_mut().extend(modes_of(pod));
                    }
                }
            })
            .register();
        node.enum_params(0, Some(ParamType::EnumFormat), 0, u32::MAX);
        self.roundtrip()?;
        Ok(modes.take())
    }
}

/// The cameras a remote shows. Blocks while PipeWire answers.
pub fn cameras(remote: Remote) -> Result<Vec<CameraInfo>, CameraError> {
    let session = Session::connect(remote)?;
    let (nodes, _) = session.cameras(Bind::Nothing)?;
    Ok(nodes.iter().map(Node::info).collect())
}

/// A running capture; stops when dropped.
pub struct Capture {
    pub mode: Mode,
    quit: pw::channel::Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl Capture {
    /// Starts the camera with this id (or the first), sending its frames to
    /// `frames`. Returns once the stream is set up.
    pub fn start(
        remote: Remote,
        camera: Option<&str>,
        frames: SyncSender<RawFrame>,
    ) -> Result<Self, CameraError> {
        let wanted = match camera {
            Some(id) => Some(
                id.strip_prefix(ID_PREFIX)
                    .ok_or(CameraError::NoCamera)?
                    .to_owned(),
            ),
            None => None,
        };
        let (started, outcome) = mpsc::channel();
        let (quit, quit_receiver) = pw::channel::channel();
        let thread = std::thread::Builder::new()
            .name("opencord-camera".to_owned())
            .spawn(move || run(remote, wanted, frames, quit_receiver, started))
            .map_err(|error| CameraError::Failed(error.to_string()))?;
        match outcome.recv() {
            Ok(Ok(mode)) => Ok(Self {
                mode,
                quit,
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => {
                let _ = thread.join();
                Err(CameraError::Failed("the camera thread ended".to_owned()))
            }
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.quit.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The camera thread: finds the camera, picks its mode, streams until told
/// to stop.
fn run(
    remote: Remote,
    wanted: Option<String>,
    frames: SyncSender<RawFrame>,
    quit: pw::channel::Receiver<()>,
    started: mpsc::Sender<Result<Mode, CameraError>>,
) {
    let session = match Session::connect(remote) {
        Ok(session) => session,
        Err(error) => {
            let _ = started.send(Err(error));
            return;
        }
    };
    let stream = match open_stream(&session, wanted.as_deref(), frames) {
        Ok(stream) => stream,
        Err(error) => {
            let _ = started.send(Err(error));
            return;
        }
    };
    let _quit = quit.attach(session.mainloop.loop_(), {
        let mainloop = session.mainloop.clone();
        move |()| mainloop.quit()
    });
    let _ = started.send(Ok(stream.mode));
    session.mainloop.run();
    let _ = stream.stream.disconnect();
}

struct Open {
    mode: Mode,
    stream: pw::stream::StreamRc,
    _listener: pw::stream::StreamListener<()>,
}

fn open_stream(
    session: &Session,
    wanted: Option<&str>,
    frames: SyncSender<RawFrame>,
) -> Result<Open, CameraError> {
    let bind = wanted.map_or(Bind::First, Bind::Named);
    let (_, bound) = session.cameras(bind)?;
    let (node_id, node) = bound.ok_or(CameraError::NoCamera)?;
    let mode = mode::choose(&session.modes(&node)?).ok_or(CameraError::NoUsableMode)?;
    let stream = pw::stream::StreamRc::new(
        session.core.clone(),
        "Opencord camera",
        pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Camera",
        },
    )
    .map_err(failed)?;
    let listener = stream
        .add_local_listener_with_user_data(())
        .state_changed({
            // A camera unplugged, or failing, ends the capture: the thread
            // finishes and the frame queue closes.
            let mainloop = session.mainloop.clone();
            move |_, _, old, state| match state {
                pw::stream::StreamState::Error(message) => {
                    tracing::warn!(%message, "the camera stream failed");
                    mainloop.quit();
                }
                pw::stream::StreamState::Unconnected
                    if matches!(
                        old,
                        pw::stream::StreamState::Streaming | pw::stream::StreamState::Paused
                    ) =>
                {
                    mainloop.quit();
                }
                _ => {}
            }
        })
        .process(move |stream, _| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let Some(data) = buffer.datas_mut().first_mut() else {
                return;
            };
            let chunk = data.chunk();
            let (offset, size, stride) = (
                chunk.offset() as usize,
                chunk.size() as usize,
                chunk.stride(),
            );
            let Some(bytes) = data.data() else {
                return;
            };
            let Some(region) = bytes.get(offset..offset + size) else {
                return;
            };
            if let Some(frame) = pack(mode, region, stride, Instant::now()) {
                // Dropped while the encoder is behind.
                let _ = frames.try_send(frame);
            }
        })
        .register()
        .map_err(failed)?;
    let format = format_pod(mode);
    let mut params = [Pod::from_bytes(&format).ok_or(CameraError::NoUsableMode)?];
    stream
        .connect(
            Direction::Input,
            Some(node_id),
            pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
            &mut params,
        )
        .map_err(failed)?;
    Ok(Open {
        mode,
        stream,
        _listener: listener,
    })
}

/// The one format to ask a camera for.
pub(super) fn format_pod(mode: Mode) -> Vec<u8> {
    let (subtype, raw) = match mode.format {
        RawFormat::Mjpeg => (MediaSubtype::Mjpg, None),
        RawFormat::Yuyv => (MediaSubtype::Raw, Some(VideoFormat::YUY2)),
        RawFormat::Nv12 => (MediaSubtype::Raw, Some(VideoFormat::NV12)),
        RawFormat::I420 => (MediaSubtype::Raw, Some(VideoFormat::I420)),
        RawFormat::Bgrx => (MediaSubtype::Raw, Some(VideoFormat::BGRx)),
        RawFormat::Rgbx => (MediaSubtype::Raw, Some(VideoFormat::RGBx)),
    };
    let mut properties = vec![
        Property::new(
            FormatProperties::MediaType.as_raw(),
            Value::Id(Id(MediaType::Video.as_raw())),
        ),
        Property::new(
            FormatProperties::MediaSubtype.as_raw(),
            Value::Id(Id(subtype.as_raw())),
        ),
    ];
    if let Some(format) = raw {
        properties.push(Property::new(
            FormatProperties::VideoFormat.as_raw(),
            Value::Id(Id(format.as_raw())),
        ));
    }
    properties.push(Property::new(
        FormatProperties::VideoSize.as_raw(),
        Value::Rectangle(Rectangle {
            width: mode.width,
            height: mode.height,
        }),
    ));
    properties.push(Property::new(
        FormatProperties::VideoFramerate.as_raw(),
        Value::Fraction(Fraction {
            num: mode.fps,
            denom: 1,
        }),
    ));
    serialize(Object {
        type_: SpaTypes::ObjectParamFormat.as_raw(),
        id: ParamType::EnumFormat.as_raw(),
        properties,
    })
}

pub(crate) fn serialize(object: Object) -> Vec<u8> {
    PodSerializer::serialize(Cursor::new(Vec::new()), &Value::Object(object))
        .map(|(cursor, _)| cursor.into_inner())
        .unwrap_or_default()
}

/// The modes in one `EnumFormat` parameter: every combination of the
/// formats, sizes and frame rates it allows that Opencord can use.
fn modes_of(pod: &Pod) -> Vec<Mode> {
    let Ok((_, Value::Object(object))) = PodDeserializer::deserialize_any_from(pod.as_bytes())
    else {
        return Vec::new();
    };
    let property = |key: FormatProperties| {
        object
            .properties
            .iter()
            .find(|property| property.key == key.as_raw())
            .map(|property| &property.value)
    };
    let video = property(FormatProperties::MediaType)
        .is_some_and(|value| ids(value).contains(&MediaType::Video.as_raw()));
    if !video {
        return Vec::new();
    }
    let subtypes = property(FormatProperties::MediaSubtype)
        .map(ids)
        .unwrap_or_default();
    let formats: Vec<RawFormat> = if subtypes.contains(&MediaSubtype::Mjpg.as_raw()) {
        vec![RawFormat::Mjpeg]
    } else if subtypes.contains(&MediaSubtype::Raw.as_raw()) {
        property(FormatProperties::VideoFormat)
            .map(ids)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|id| match VideoFormat::from_raw(id) {
                VideoFormat::YUY2 => Some(RawFormat::Yuyv),
                VideoFormat::NV12 => Some(RawFormat::Nv12),
                VideoFormat::I420 => Some(RawFormat::I420),
                _ => None,
            })
            .collect()
    } else {
        Vec::new()
    };
    let sizes = property(FormatProperties::VideoSize)
        .map(rectangles)
        .unwrap_or_default();
    let rates = property(FormatProperties::VideoFramerate)
        .map(rates)
        .unwrap_or_default();
    let mut modes = Vec::new();
    for &format in &formats {
        for size in &sizes {
            for &fps in &rates {
                let mode = Mode {
                    format,
                    width: size.width,
                    height: size.height,
                    fps,
                };
                if !modes.contains(&mode) {
                    modes.push(mode);
                }
            }
        }
    }
    modes
}

fn ids(value: &Value) -> Vec<u32> {
    match value {
        Value::Id(id) => vec![id.0],
        Value::Choice(ChoiceValue::Id(choice)) => match &choice.1 {
            ChoiceEnum::None(id) => vec![id.0],
            ChoiceEnum::Enum {
                default,
                alternatives,
            } => std::iter::once(default)
                .chain(alternatives)
                .map(|id| id.0)
                .collect(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

fn rectangles(value: &Value) -> Vec<Rectangle> {
    let ideal = Rectangle {
        width: 1280,
        height: 720,
    };
    match value {
        Value::Rectangle(size) => vec![*size],
        Value::Choice(ChoiceValue::Rectangle(choice)) => match &choice.1 {
            ChoiceEnum::None(size) => vec![*size],
            ChoiceEnum::Enum {
                default,
                alternatives,
            } => std::iter::once(default)
                .chain(alternatives)
                .copied()
                .collect(),
            ChoiceEnum::Range { default, min, max }
            | ChoiceEnum::Step {
                default, min, max, ..
            } => {
                let fits = (min.width..=max.width).contains(&ideal.width)
                    && (min.height..=max.height).contains(&ideal.height);
                if fits {
                    vec![*default, ideal]
                } else {
                    vec![*default]
                }
            }
            ChoiceEnum::Flags { .. } => Vec::new(),
        },
        _ => Vec::new(),
    }
}

fn rates(value: &Value) -> Vec<u32> {
    let fps = |fraction: &Fraction| {
        (fraction.denom > 0).then(|| (fraction.num + fraction.denom / 2) / fraction.denom)
    };
    match value {
        Value::Fraction(rate) => fps(rate).into_iter().collect(),
        Value::Choice(ChoiceValue::Fraction(choice)) => match &choice.1 {
            ChoiceEnum::None(rate) => fps(rate).into_iter().collect(),
            ChoiceEnum::Enum {
                default,
                alternatives,
            } => std::iter::once(default)
                .chain(alternatives)
                .filter_map(fps)
                .collect(),
            ChoiceEnum::Range { default, min, max }
            | ChoiceEnum::Step {
                default, min, max, ..
            } => {
                let thirty = (fps(min).unwrap_or(0)..=fps(max).unwrap_or(0)).contains(&30);
                let mut rates: Vec<u32> = fps(default).into_iter().collect();
                if thirty {
                    rates.push(30);
                }
                rates
            }
            ChoiceEnum::Flags { .. } => Vec::new(),
        },
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use spa::utils::{Choice, ChoiceFlags};

    use super::*;
    use crate::video::camera::virtual_camera::VirtualCamera;

    #[test]
    fn a_camera_on_pipewire_is_listed_and_captured() {
        let Some(camera) = VirtualCamera::start() else {
            eprintln!("no PipeWire here");
            return;
        };
        let id = format!("{ID_PREFIX}{}", camera.name);

        let listed = cameras(Remote::Session).unwrap();
        let info = listed
            .iter()
            .find(|info| info.id == id)
            .expect("the camera is listed");
        assert_eq!(info.name, VirtualCamera::NICK);

        let (frames, received) = mpsc::sync_channel(4);
        let started = Instant::now();
        let capture = Capture::start(Remote::Session, Some(&id), frames).unwrap();
        assert_eq!(
            capture.mode,
            Mode {
                format: RawFormat::Yuyv,
                width: 1280,
                height: 720,
                fps: 30
            }
        );
        let first = received
            .recv_timeout(Duration::from_secs(3))
            .expect("a first frame");
        let first_after = started.elapsed();
        let mut lumas = vec![first.data[0]];
        while lumas.len() < 10 {
            let frame = received
                .recv_timeout(Duration::from_secs(3))
                .expect("frames keep coming");
            assert_eq!(
                (frame.format, frame.width, frame.height),
                (RawFormat::Yuyv, 1280, 720)
            );
            assert_eq!(frame.data.len(), 1280 * 720 * 2);
            lumas.push(frame.data[0]);
        }

        assert!(lumas.windows(2).any(|pair| pair[0] != pair[1]), "{lumas:?}");
        assert!(
            first_after < Duration::from_secs(1),
            "first frame after {first_after:?}"
        );
        drop(capture);
        while received.try_recv().is_ok() {}
        std::thread::sleep(Duration::from_millis(200));
        assert!(received.try_recv().is_err(), "nothing after stopping");
    }

    #[test]
    fn an_mjpeg_camera_is_captured_and_its_frames_decode() {
        use crate::video::camera::virtual_camera::VirtualFormat;
        use crate::video::codec::raw::RawDecoder;

        let Some(camera) = VirtualCamera::start_with(VirtualFormat::Mjpeg) else {
            eprintln!("no PipeWire here");
            return;
        };
        let (frames, received) = mpsc::sync_channel(4);
        let capture = Capture::start(Remote::Session, Some(&camera.id()), frames).unwrap();

        assert_eq!(capture.mode.format, RawFormat::Mjpeg);
        let mut decoder = RawDecoder::new();
        for _ in 0..5 {
            let frame = received
                .recv_timeout(Duration::from_secs(3))
                .expect("a frame");
            let picture = decoder.picture(&frame).unwrap();
            assert_eq!((picture.width, picture.height), (1280, 720));
        }
    }

    #[test]
    fn a_camera_that_goes_away_closes_the_frame_queue() {
        let Some(camera) = VirtualCamera::start() else {
            eprintln!("no PipeWire here");
            return;
        };
        let id = format!("{ID_PREFIX}{}", camera.name);
        let (frames, received) = mpsc::sync_channel(4);
        let _capture = Capture::start(Remote::Session, Some(&id), frames).unwrap();
        received
            .recv_timeout(Duration::from_secs(3))
            .expect("a first frame");

        drop(camera);

        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match received.recv_timeout(Duration::from_millis(100)) {
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                _ => assert!(Instant::now() < deadline, "the queue stayed open"),
            }
        }
    }

    #[test]
    fn a_camera_that_is_not_there_is_not_found() {
        if VirtualCamera::start().is_none() {
            eprintln!("no PipeWire here");
            return;
        }
        let (frames, _received) = mpsc::sync_channel(1);

        let missing = Capture::start(Remote::Session, Some("pipewire:no-such-camera"), frames);
        let wrong = Capture::start(
            Remote::Session,
            Some("v4l2:/dev/video9"),
            mpsc::sync_channel(1).0,
        );

        assert_eq!(missing.err(), Some(CameraError::NoCamera));
        assert_eq!(wrong.err(), Some(CameraError::NoCamera));
    }

    fn object(properties: Vec<Property>) -> Vec<u8> {
        serialize(Object {
            type_: SpaTypes::ObjectParamFormat.as_raw(),
            id: ParamType::EnumFormat.as_raw(),
            properties,
        })
    }

    fn id(key: FormatProperties, value: u32) -> Property {
        Property::new(key.as_raw(), Value::Id(Id(value)))
    }

    fn parsed(bytes: &[u8]) -> Vec<Mode> {
        modes_of(Pod::from_bytes(bytes).unwrap())
    }

    fn mode(format: RawFormat, width: u32, height: u32, fps: u32) -> Mode {
        Mode {
            format,
            width,
            height,
            fps,
        }
    }

    #[test]
    fn the_format_asked_for_reads_back_as_its_mode() {
        for wanted in [
            mode(RawFormat::Mjpeg, 1280, 720, 30),
            mode(RawFormat::Yuyv, 640, 480, 30),
            mode(RawFormat::Nv12, 1280, 720, 30),
            mode(RawFormat::I420, 320, 240, 15),
        ] {
            assert_eq!(parsed(&format_pod(wanted)), vec![wanted]);
        }
    }

    #[test]
    fn enumerated_choices_become_every_usable_combination() {
        fn enumerated<T: spa::pod::CanonicalFixedSizedPod>(
            default: T,
            alternatives: Vec<T>,
        ) -> ChoiceEnum<T> {
            ChoiceEnum::Enum {
                default,
                alternatives,
            }
        }
        let pod = object(vec![
            id(FormatProperties::MediaType, MediaType::Video.as_raw()),
            id(FormatProperties::MediaSubtype, MediaSubtype::Raw.as_raw()),
            Property::new(
                FormatProperties::VideoFormat.as_raw(),
                Value::Choice(ChoiceValue::Id(Choice(
                    ChoiceFlags::empty(),
                    enumerated(
                        Id(VideoFormat::YUY2.as_raw()),
                        vec![
                            Id(VideoFormat::YUY2.as_raw()),
                            Id(VideoFormat::NV12.as_raw()),
                            Id(VideoFormat::RGB.as_raw()),
                        ],
                    ),
                ))),
            ),
            Property::new(
                FormatProperties::VideoSize.as_raw(),
                Value::Choice(ChoiceValue::Rectangle(Choice(
                    ChoiceFlags::empty(),
                    enumerated(
                        Rectangle {
                            width: 640,
                            height: 480,
                        },
                        vec![Rectangle {
                            width: 1280,
                            height: 720,
                        }],
                    ),
                ))),
            ),
            Property::new(
                FormatProperties::VideoFramerate.as_raw(),
                Value::Choice(ChoiceValue::Fraction(Choice(
                    ChoiceFlags::empty(),
                    enumerated(
                        Fraction { num: 30, denom: 1 },
                        vec![Fraction { num: 15, denom: 1 }],
                    ),
                ))),
            ),
        ]);

        let modes = parsed(&pod);

        assert_eq!(modes.len(), 8, "{modes:?}");
        assert!(modes.contains(&mode(RawFormat::Nv12, 1280, 720, 15)));
        assert!(modes.iter().all(|mode| mode.format != RawFormat::Mjpeg));
    }

    #[test]
    fn ranges_give_their_default_and_720p30_when_allowed() {
        let pod = object(vec![
            id(FormatProperties::MediaType, MediaType::Video.as_raw()),
            id(FormatProperties::MediaSubtype, MediaSubtype::Mjpg.as_raw()),
            Property::new(
                FormatProperties::VideoSize.as_raw(),
                Value::Choice(ChoiceValue::Rectangle(Choice(
                    ChoiceFlags::empty(),
                    ChoiceEnum::Range {
                        default: Rectangle {
                            width: 640,
                            height: 480,
                        },
                        min: Rectangle {
                            width: 1,
                            height: 1,
                        },
                        max: Rectangle {
                            width: 1920,
                            height: 1080,
                        },
                    },
                ))),
            ),
            Property::new(
                FormatProperties::VideoFramerate.as_raw(),
                Value::Choice(ChoiceValue::Fraction(Choice(
                    ChoiceFlags::empty(),
                    ChoiceEnum::Range {
                        default: Fraction { num: 25, denom: 1 },
                        min: Fraction { num: 1, denom: 1 },
                        max: Fraction { num: 60, denom: 1 },
                    },
                ))),
            ),
        ]);

        let modes = parsed(&pod);

        assert!(
            modes.contains(&mode(RawFormat::Mjpeg, 1280, 720, 30)),
            "{modes:?}"
        );
        assert!(modes.contains(&mode(RawFormat::Mjpeg, 640, 480, 25)));
    }

    #[test]
    fn other_media_and_codecs_give_no_modes() {
        let h264 = object(vec![
            id(FormatProperties::MediaType, MediaType::Video.as_raw()),
            id(FormatProperties::MediaSubtype, MediaSubtype::H264.as_raw()),
            Property::new(
                FormatProperties::VideoSize.as_raw(),
                Value::Rectangle(Rectangle {
                    width: 1280,
                    height: 720,
                }),
            ),
        ]);
        let audio = object(vec![
            id(FormatProperties::MediaType, MediaType::Audio.as_raw()),
            id(FormatProperties::MediaSubtype, MediaSubtype::Raw.as_raw()),
        ]);

        assert!(parsed(&h264).is_empty());
        assert!(parsed(&audio).is_empty());
    }

    #[test]
    fn strided_rows_are_packed() {
        let now = Instant::now();
        // YUYV 4×2 with 4 bytes of padding per row.
        let yuyv = [
            1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 0, 9, 10, 11, 12, 13, 14, 15, 16, 0, 0, 0, 0,
        ];
        let frame = pack(mode(RawFormat::Yuyv, 4, 2, 30), &yuyv, 12, now).unwrap();
        assert_eq!(frame.data, (1..=16).collect::<Vec<u8>>());

        // NV12 2×2: luma rows of stride 4, then one chroma row.
        let nv12 = [1, 2, 0, 0, 3, 4, 0, 0, 5, 6, 0, 0];
        let frame = pack(mode(RawFormat::Nv12, 2, 2, 30), &nv12, 4, now).unwrap();
        assert_eq!(frame.data, vec![1, 2, 3, 4, 5, 6]);

        // I420 2×2: luma stride 4, chroma planes stride 2.
        let i420 = [1, 2, 0, 0, 3, 4, 0, 0, 5, 0, 6, 0];
        let frame = pack(mode(RawFormat::I420, 2, 2, 30), &i420, 4, now).unwrap();
        assert_eq!(frame.data, vec![1, 2, 3, 4, 5, 6]);

        // Too short for its mode.
        assert!(pack(mode(RawFormat::Yuyv, 4, 2, 30), &yuyv[..10], 12, now).is_none());
    }
}
