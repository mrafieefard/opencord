//! Reading a screen or window from PipeWire (plan §9.3): the portal's node
//! on its remote, or a node named in PipeWire's own session. Frames come in
//! shared memory as BGRx, BGRA, RGBx or RGBA, only when the content
//! changed; the size follows the source (a window resized, a monitor's
//! resolution changed). When the source goes away (the window closed, the
//! monitor unplugged) the capture ends and its frame queue closes.
//! DMA-BUF straight into the encoder is V10's zero-copy work.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::mpsc::{self, SyncSender};
use std::thread::JoinHandle;
use std::time::Instant;

use pipewire as pw;
use pw::spa;
use spa::param::ParamType;
use spa::param::format::{FormatProperties, MediaSubtype, MediaType};
use spa::param::video::{VideoFormat, VideoInfoRaw};
use spa::pod::{Pod, property};
use spa::utils::{Direction, Fraction, Rectangle, SpaTypes};

use super::ScreenError;
use crate::video::camera::mode::Mode;
use crate::video::camera::pipewire::{ROUNDTRIP, Remote, serialize};
use crate::video::camera::{RawFormat, RawFrame, pack};

/// Which node to read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A portal stream's node, on the portal's remote.
    Id(u32),
    /// A node by name, in PipeWire's own session (checks, virtual screens).
    Named(String),
}

/// A running screen capture; stops when dropped.
pub struct ScreenCapture {
    /// The source's size when capture started.
    pub size: (u32, u32),
    quit: pw::channel::Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl ScreenCapture {
    /// Starts reading `target`, at most `max_fps` frames a second, into
    /// `frames`. Returns once PipeWire has agreed a format and size.
    pub fn start(
        remote: Remote,
        target: Target,
        max_fps: u32,
        frames: SyncSender<RawFrame>,
    ) -> Result<Self, ScreenError> {
        let (started, outcome) = mpsc::channel();
        let (quit, quit_receiver) = pw::channel::channel();
        let thread = std::thread::Builder::new()
            .name("opencord-screen".to_owned())
            .spawn(move || run(remote, target, max_fps, frames, quit_receiver, started))
            .map_err(|error| ScreenError::Failed(error.to_string()))?;
        match outcome.recv_timeout(ROUNDTRIP) {
            Ok(Ok(size)) => Ok(Self {
                size,
                quit,
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => {
                let _ = quit.send(());
                let _ = thread.join();
                Err(ScreenError::Failed(
                    "the screen's stream did not start".to_owned(),
                ))
            }
        }
    }

    /// Whether capture has ended: the source went away.
    pub fn finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }
}

impl Drop for ScreenCapture {
    fn drop(&mut self) {
        let _ = self.quit.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn failed(error: pw::Error) -> ScreenError {
    ScreenError::Failed(error.to_string())
}

/// The format and size agreed, as frames are packed.
type Agreed = Rc<Cell<Option<Mode>>>;

fn run(
    remote: Remote,
    target: Target,
    max_fps: u32,
    frames: SyncSender<RawFrame>,
    quit: pw::channel::Receiver<()>,
    started: mpsc::Sender<Result<(u32, u32), ScreenError>>,
) {
    let fail = |error: ScreenError| {
        let _ = started.send(Err(error));
    };
    pw::init();
    let Ok(mainloop) = pw::main_loop::MainLoopRc::new(None) else {
        return fail(ScreenError::Failed("no PipeWire loop".to_owned()));
    };
    let Ok(context) = pw::context::ContextRc::new(&mainloop, None) else {
        return fail(ScreenError::Failed("no PipeWire context".to_owned()));
    };
    let core = match remote {
        Remote::Portal(fd) => context.connect_fd_rc(fd, None),
        Remote::Session => context.connect_rc(None),
    };
    let core = match core {
        Ok(core) => core,
        Err(error) => return fail(failed(error)),
    };
    let mut properties = pw::properties::properties! {
        *pw::keys::MEDIA_TYPE => "Video",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Screen",
    };
    let node_id = match &target {
        Target::Id(id) => Some(*id),
        Target::Named(name) => {
            properties.insert(*pw::keys::TARGET_OBJECT, name.as_str());
            // Never another source (a camera) when the named one is gone.
            properties.insert("node.dont-fallback", "true");
            properties.insert("node.dont-reconnect", "true");
            None
        }
    };
    let stream = match pw::stream::StreamRc::new(core, "Opencord screen share", properties) {
        Ok(stream) => stream,
        Err(error) => return fail(failed(error)),
    };
    let agreed: Agreed = Rc::new(Cell::new(None));
    let started_once = Rc::new(Cell::new(Some(started.clone())));
    let listener = stream
        .add_local_listener_with_user_data(())
        .state_changed({
            let mainloop = mainloop.clone();
            let started_once = Rc::clone(&started_once);
            move |_, _, old, state| match state {
                pw::stream::StreamState::Error(message) => {
                    tracing::warn!(%message, "the screen stream failed");
                    if let Some(started) = started_once.take() {
                        let _ = started.send(Err(ScreenError::Failed(message)));
                    }
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
        .param_changed({
            let agreed = Rc::clone(&agreed);
            let started_once = Rc::clone(&started_once);
            move |_, _, id, param| {
                let Some(param) = param else {
                    return;
                };
                if id != ParamType::Format.as_raw() {
                    return;
                }
                let mut info = VideoInfoRaw::new();
                if info.parse(param).is_err() {
                    return;
                }
                let format = match info.format() {
                    VideoFormat::BGRx | VideoFormat::BGRA => RawFormat::Bgrx,
                    VideoFormat::RGBx | VideoFormat::RGBA => RawFormat::Rgbx,
                    _ => return,
                };
                let Rectangle { width, height } = info.size();
                agreed.set(Some(Mode {
                    format,
                    width,
                    height,
                    fps: max_fps,
                }));
                if let Some(started) = started_once.take() {
                    let _ = started.send(Ok((width, height)));
                }
            }
        })
        .process({
            let agreed = Rc::clone(&agreed);
            move |stream, _| {
                let Some(mut buffer) = stream.dequeue_buffer() else {
                    return;
                };
                let Some(mode) = agreed.get() else {
                    return;
                };
                let Some(data) = buffer.datas_mut().first_mut() else {
                    return;
                };
                let chunk = data.chunk();
                let corrupted =
                    chunk.flags().bits() & spa::sys::SPA_CHUNK_FLAG_CORRUPTED as i32 != 0;
                let (offset, size, stride) = (
                    chunk.offset() as usize,
                    chunk.size() as usize,
                    chunk.stride(),
                );
                // Nothing changed (only the cursor moved), or the frame is
                // damaged: nothing to send.
                if size == 0 || corrupted {
                    return;
                }
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
            }
        })
        .register();
    let Ok(_listener) = listener else {
        return fail(ScreenError::Failed("no stream listener".to_owned()));
    };
    let format = format_pod(max_fps);
    let Some(pod) = Pod::from_bytes(&format) else {
        return fail(ScreenError::Failed("no format".to_owned()));
    };
    let flags = pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS;
    if let Err(error) = stream.connect(Direction::Input, node_id, flags, &mut [pod]) {
        return fail(failed(error));
    }
    let _quit = quit.attach(mainloop.loop_(), {
        let mainloop = mainloop.clone();
        move |()| mainloop.quit()
    });
    mainloop.run();
    let _ = stream.disconnect();
}

/// What Opencord takes from a screen: 8-bit RGB in shared memory (no
/// modifiers, so no DMA-BUF), any size, up to `max_fps`.
fn format_pod(max_fps: u32) -> Vec<u8> {
    serialize(spa::pod::object!(
        SpaTypes::ObjectParamFormat,
        ParamType::EnumFormat,
        property!(FormatProperties::MediaType, Id, MediaType::Video),
        property!(FormatProperties::MediaSubtype, Id, MediaSubtype::Raw),
        property!(
            FormatProperties::VideoFormat,
            Choice,
            Enum,
            Id,
            VideoFormat::BGRx,
            VideoFormat::BGRx,
            VideoFormat::BGRA,
            VideoFormat::RGBx,
            VideoFormat::RGBA
        ),
        property!(
            FormatProperties::VideoSize,
            Choice,
            Range,
            Rectangle,
            Rectangle {
                width: 1920,
                height: 1080
            },
            Rectangle {
                width: 2,
                height: 2
            },
            Rectangle {
                width: 8192,
                height: 8192
            }
        ),
        property!(
            FormatProperties::VideoFramerate,
            Choice,
            Range,
            Fraction,
            Fraction {
                num: max_fps,
                denom: 1
            },
            Fraction { num: 0, denom: 1 },
            Fraction {
                num: max_fps.max(1),
                denom: 1
            }
        ),
    ))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::video::camera::virtual_camera::{VirtualCamera, VirtualFormat};
    use crate::video::codec::raw::RawDecoder;

    fn virtual_screen(width: u32, height: u32) -> Option<VirtualCamera> {
        VirtualCamera::start_with(VirtualFormat::Screen {
            width,
            height,
            fps: 30,
        })
    }

    fn next(received: &mpsc::Receiver<RawFrame>) -> RawFrame {
        received
            .recv_timeout(Duration::from_secs(3))
            .expect("a frame")
    }

    #[test]
    fn a_screen_is_read_at_its_size_and_follows_a_resize() {
        let Some(screen) = virtual_screen(640, 360) else {
            eprintln!("no PipeWire here");
            return;
        };
        let (frames, received) = mpsc::sync_channel(4);

        let capture = ScreenCapture::start(
            Remote::Session,
            Target::Named(screen.name.clone()),
            30,
            frames,
        )
        .unwrap();

        assert_eq!(capture.size, (640, 360));
        let frame = next(&received);
        assert_eq!(
            (frame.format, frame.width, frame.height),
            (RawFormat::Bgrx, 640, 360)
        );
        let picture = RawDecoder::new().picture(&frame).unwrap();
        assert_eq!((picture.width, picture.height), (640, 360));

        screen.resize(480, 270);
        let resized = (0..60)
            .map(|_| next(&received))
            .find(|frame| (frame.width, frame.height) == (480, 270));
        assert!(resized.is_some(), "frames at the new size");
    }

    #[test]
    fn a_screen_that_goes_away_ends_the_capture() {
        let Some(screen) = virtual_screen(320, 180) else {
            eprintln!("no PipeWire here");
            return;
        };
        let (frames, received) = mpsc::sync_channel(4);
        let capture = ScreenCapture::start(
            Remote::Session,
            Target::Named(screen.name.clone()),
            30,
            frames,
        )
        .unwrap();
        next(&received);

        drop(screen);

        let deadline = Instant::now() + Duration::from_secs(3);
        while !capture.finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(capture.finished());
        while received.try_recv().is_ok() {}
        assert!(received.recv_timeout(Duration::from_millis(100)).is_err());
    }

    #[test]
    fn a_missing_screen_is_an_error_not_a_hang() {
        let (frames, _received) = mpsc::sync_channel(4);
        let Err(error) = ScreenCapture::start(
            Remote::Session,
            Target::Named("opencord-no-such-node".to_owned()),
            30,
            frames,
        ) else {
            panic!("a capture of nothing");
        };
        assert!(matches!(error, ScreenError::Failed(_)), "{error:?}");
    }
}
