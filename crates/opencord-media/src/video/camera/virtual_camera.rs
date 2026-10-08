//! A camera that exists only for tests and measurements: a PipeWire video
//! source with the Camera role at 1280×720 and 30 fps. In YUYV its luma
//! counts frames; in MJPEG it cycles through 30 pictures of the moving
//! scene, encoded once at the start, as a webcam would send them.

use std::cell::{Cell, RefCell};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use pipewire as pw;
use pw::spa;
use spa::param::ParamType;
use spa::pod::{Object, Pod, Property, Value};
use spa::utils::{Direction, SpaTypes};

use super::RawFormat;
use super::mode::Mode;
use super::pipewire::{ID_PREFIX, ROUNDTRIP, format_pod, serialize};
use crate::video::codec::raw::encode_jpeg;
use crate::video::pattern::scene;

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;
/// Pictures the MJPEG camera cycles through.
const PICTURES: u64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtualFormat {
    Yuyv,
    Mjpeg,
}

pub struct VirtualCamera {
    pub name: String,
    quit: pw::channel::Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl VirtualCamera {
    pub const NICK: &str = "Opencord test camera";

    /// A YUYV camera; `None` without a PipeWire server.
    pub fn start() -> Option<Self> {
        Self::start_with(VirtualFormat::Yuyv)
    }

    pub fn start_with(format: VirtualFormat) -> Option<Self> {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let number = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let name = format!("opencord-test-camera-{}-{number}", std::process::id());
        let (ready, readiness) = mpsc::channel();
        let (quit, quit_receiver) = pw::channel::channel();
        let thread = std::thread::Builder::new()
            .name("virtual-camera".to_owned())
            .spawn({
                let name = name.clone();
                move || serve(&name, format, &ready, quit_receiver)
            })
            .ok()?;
        if readiness.recv_timeout(ROUNDTRIP) == Ok(true) {
            Some(Self {
                name,
                quit,
                thread: Some(thread),
            })
        } else {
            let _ = quit.send(());
            None
        }
    }

    /// Its id among `cameras()`.
    pub fn id(&self) -> String {
        format!("{ID_PREFIX}{}", self.name)
    }
}

impl Drop for VirtualCamera {
    fn drop(&mut self) {
        let _ = self.quit.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve(
    name: &str,
    format: VirtualFormat,
    ready: &mpsc::Sender<bool>,
    quit: pw::channel::Receiver<()>,
) {
    let fail = || {
        let _ = ready.send(false);
    };
    let jpegs: Vec<Vec<u8>> = match format {
        VirtualFormat::Yuyv => Vec::new(),
        VirtualFormat::Mjpeg => (0..PICTURES)
            .map(|number| encode_jpeg(&scene(WIDTH, HEIGHT, number, Instant::now())))
            .collect(),
    };
    let size = match format {
        VirtualFormat::Yuyv => WIDTH * HEIGHT * 2,
        VirtualFormat::Mjpeg => {
            u32::try_from(jpegs.iter().map(Vec::len).max().unwrap_or(0)).unwrap_or(u32::MAX)
        }
    };
    let stride = match format {
        VirtualFormat::Yuyv => WIDTH as i32 * 2,
        VirtualFormat::Mjpeg => 0,
    };
    pw::init();
    let Ok(mainloop) = pw::main_loop::MainLoopRc::new(None) else {
        return fail();
    };
    let Ok(context) = pw::context::ContextRc::new(&mainloop, None) else {
        return fail();
    };
    let Ok(core) = context.connect_rc(None) else {
        return fail();
    };
    let Ok(stream) = pw::stream::StreamRc::new(
        core,
        name,
        pw::properties::properties! {
            *pw::keys::MEDIA_CLASS => "Video/Source",
            *pw::keys::MEDIA_ROLE => "Camera",
            *pw::keys::NODE_NAME => name,
            "node.nick" => VirtualCamera::NICK,
        },
    ) else {
        return fail();
    };
    let signal = RefCell::new(Some(ready.clone()));
    let frame = Cell::new(0u64);
    let Ok(_listener) = stream
        .add_local_listener_with_user_data(())
        .state_changed(move |_, _, _, state| {
            let ok = match state {
                pw::stream::StreamState::Paused | pw::stream::StreamState::Streaming => true,
                pw::stream::StreamState::Error(_) => false,
                _ => return,
            };
            if let Some(signal) = signal.take() {
                let _ = signal.send(ok);
            }
        })
        .param_changed(move |stream, _, id, param| {
            if id != ParamType::Format.as_raw() || param.is_none() {
                return;
            }
            let buffers = serialize(Object {
                type_: SpaTypes::ObjectParamBuffers.as_raw(),
                id: ParamType::Buffers.as_raw(),
                properties: vec![
                    Property::new(spa::sys::SPA_PARAM_BUFFERS_buffers, Value::Int(4)),
                    Property::new(spa::sys::SPA_PARAM_BUFFERS_blocks, Value::Int(1)),
                    Property::new(spa::sys::SPA_PARAM_BUFFERS_size, Value::Int(size as i32)),
                    Property::new(spa::sys::SPA_PARAM_BUFFERS_stride, Value::Int(stride)),
                ],
            });
            if let Some(pod) = Pod::from_bytes(&buffers) {
                let _ = stream.update_params(&mut [pod]);
            }
        })
        .process(move |stream, _| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let Some(data) = buffer.datas_mut().first_mut() else {
                return;
            };
            let number = frame.get();
            frame.set(number + 1);
            // Buffers come before the sizes asked for apply; those stay
            // empty. Nothing in a PipeWire callback may panic.
            let written = match format {
                VirtualFormat::Yuyv => {
                    let Some(bytes) = data.data().and_then(|bytes| bytes.get_mut(..size as usize))
                    else {
                        return;
                    };
                    let luma = 16 + (number % 200) as u8;
                    for pixel in bytes.as_chunks_mut::<4>().0 {
                        *pixel = [luma, 128, luma, 128];
                    }
                    size
                }
                VirtualFormat::Mjpeg => {
                    let jpeg = &jpegs[(number % PICTURES) as usize];
                    let Some(bytes) = data.data().and_then(|bytes| bytes.get_mut(..jpeg.len()))
                    else {
                        return;
                    };
                    bytes.copy_from_slice(jpeg);
                    jpeg.len() as u32
                }
            };
            let chunk = data.chunk_mut();
            *chunk.offset_mut() = 0;
            *chunk.size_mut() = written;
            *chunk.stride_mut() = stride;
        })
        .register()
    else {
        return fail();
    };
    let offered = format_pod(Mode {
        format: match format {
            VirtualFormat::Yuyv => RawFormat::Yuyv,
            VirtualFormat::Mjpeg => RawFormat::Mjpeg,
        },
        width: WIDTH,
        height: HEIGHT,
        fps: 30,
    });
    let Some(pod) = Pod::from_bytes(&offered) else {
        return fail();
    };
    // PipeWire allocates the buffers (ALLOC_BUFFERS would leave that to
    // us), as in its video-src example.
    let flags = pw::stream::StreamFlags::DRIVER | pw::stream::StreamFlags::MAP_BUFFERS;
    if stream
        .connect(Direction::Output, None, flags, &mut [pod])
        .is_err()
    {
        return fail();
    }
    let timer = mainloop.loop_().add_timer({
        let stream = stream.clone();
        move |_| {
            let _ = stream.trigger_process();
        }
    });
    let interval = Duration::from_nanos(33_333_333);
    timer.update_timer(Some(interval), Some(interval));
    let _quit = quit.attach(mainloop.loop_(), {
        let mainloop = mainloop.clone();
        move |()| mainloop.quit()
    });
    mainloop.run();
    let _ = stream.disconnect();
}
