//! A camera that exists only for tests and measurements: a PipeWire video
//! source with the Camera role at 1280×720 and 30 fps. In YUYV its luma
//! counts frames; in MJPEG it cycles through 30 pictures of the moving
//! scene, encoded once at the start, as a webcam would send them. As a
//! screen (BGRx, any size, no Camera role) its colour changes each frame,
//! and it can change size as a resized window does.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
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
    /// A screen: BGRx of this size at this frame rate.
    Screen {
        width: u32,
        height: u32,
        fps: u32,
    },
}

enum Control {
    Quit,
    Resize(u32, u32),
}

pub struct VirtualCamera {
    pub name: String,
    control: pw::channel::Sender<Control>,
    thread: Option<JoinHandle<()>>,
}

impl VirtualCamera {
    pub const NICK: &str = "Opencord test camera";
    pub const SCREEN_NICK: &str = "Opencord test screen";

    /// A YUYV camera; `None` without a PipeWire server.
    pub fn start() -> Option<Self> {
        Self::start_with(VirtualFormat::Yuyv)
    }

    pub fn start_with(format: VirtualFormat) -> Option<Self> {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let number = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let name = format!("opencord-test-camera-{}-{number}", std::process::id());
        let (ready, readiness) = mpsc::channel();
        let (control, control_receiver) = pw::channel::channel();
        let thread = std::thread::Builder::new()
            .name("virtual-camera".to_owned())
            .spawn({
                let name = name.clone();
                move || serve(&name, format, &ready, control_receiver)
            })
            .ok()?;
        if readiness.recv_timeout(ROUNDTRIP) == Ok(true) {
            Some(Self {
                name,
                control,
                thread: Some(thread),
            })
        } else {
            let _ = control.send(Control::Quit);
            None
        }
    }

    /// A screen changes size, as a resized window does: its format is
    /// agreed again.
    pub fn resize(&self, width: u32, height: u32) {
        let _ = self.control.send(Control::Resize(width, height));
    }

    /// Its id among `cameras()`.
    pub fn id(&self) -> String {
        format!("{ID_PREFIX}{}", self.name)
    }
}

impl Drop for VirtualCamera {
    fn drop(&mut self) {
        let _ = self.control.send(Control::Quit);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve(
    name: &str,
    format: VirtualFormat,
    ready: &mpsc::Sender<bool>,
    control: pw::channel::Receiver<Control>,
) {
    let fail = || {
        let _ = ready.send(false);
    };
    let jpegs: Vec<Vec<u8>> = match format {
        VirtualFormat::Mjpeg => (0..PICTURES)
            .map(|number| encode_jpeg(&scene(WIDTH, HEIGHT, number, Instant::now())))
            .collect(),
        _ => Vec::new(),
    };
    let (width, height, fps) = match format {
        VirtualFormat::Screen { width, height, fps } => (width, height, fps),
        _ => (WIDTH, HEIGHT, 30),
    };
    // Width and height now; a screen may change them.
    let dims = Rc::new(Cell::new((width, height)));
    let largest_jpeg = u32::try_from(jpegs_max(&jpegs)).unwrap_or(u32::MAX);
    // Bytes in a buffer, and in a row (0 when compressed).
    let layout = move |(width, height): (u32, u32)| match format {
        VirtualFormat::Yuyv => (width * height * 2, width as i32 * 2),
        VirtualFormat::Mjpeg => (largest_jpeg, 0),
        VirtualFormat::Screen { .. } => (width * height * 4, width as i32 * 4),
    };
    let raw = match format {
        VirtualFormat::Yuyv => RawFormat::Yuyv,
        VirtualFormat::Mjpeg => RawFormat::Mjpeg,
        VirtualFormat::Screen { .. } => RawFormat::Bgrx,
    };
    let offer = move |(width, height): (u32, u32)| {
        format_pod(Mode {
            format: raw,
            width,
            height,
            fps,
        })
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
    let mut properties = pw::properties::properties! {
        *pw::keys::MEDIA_CLASS => "Video/Source",
        *pw::keys::NODE_NAME => name,
    };
    match format {
        VirtualFormat::Screen { .. } => {
            properties.insert("node.nick", VirtualCamera::SCREEN_NICK);
        }
        _ => {
            properties.insert(*pw::keys::MEDIA_ROLE, "Camera");
            properties.insert("node.nick", VirtualCamera::NICK);
        }
    }
    let Ok(stream) = pw::stream::StreamRc::new(core, name, properties) else {
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
        .param_changed({
            let dims = Rc::clone(&dims);
            move |stream, _, id, param| {
                if id != ParamType::Format.as_raw() || param.is_none() {
                    return;
                }
                let (size, stride) = layout(dims.get());
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
            }
        })
        .process({
            let dims = Rc::clone(&dims);
            move |stream, _| {
                let Some(mut buffer) = stream.dequeue_buffer() else {
                    return;
                };
                let Some(data) = buffer.datas_mut().first_mut() else {
                    return;
                };
                let number = frame.get();
                frame.set(number + 1);
                let (size, stride) = layout(dims.get());
                // Buffers come before the sizes asked for apply; those stay
                // empty. Nothing in a PipeWire callback may panic.
                let written = match format {
                    VirtualFormat::Yuyv => {
                        let Some(bytes) =
                            data.data().and_then(|bytes| bytes.get_mut(..size as usize))
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
                    VirtualFormat::Screen { .. } => {
                        let Some(bytes) =
                            data.data().and_then(|bytes| bytes.get_mut(..size as usize))
                        else {
                            return;
                        };
                        let shade = (number % 256) as u8;
                        for pixel in bytes.as_chunks_mut::<4>().0 {
                            *pixel = [shade, 255 - shade, 128, 0];
                        }
                        size
                    }
                };
                let chunk = data.chunk_mut();
                *chunk.offset_mut() = 0;
                *chunk.size_mut() = written;
                *chunk.stride_mut() = stride;
            }
        })
        .register()
    else {
        return fail();
    };
    let offered = offer(dims.get());
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
    let interval = Duration::from_secs(1) / fps.max(1);
    timer.update_timer(Some(interval), Some(interval));
    let _control = control.attach(mainloop.loop_(), {
        let (mainloop, stream, dims) = (mainloop.clone(), stream.clone(), Rc::clone(&dims));
        move |command| match command {
            Control::Quit => mainloop.quit(),
            // As PipeWire's video-src-reneg example: offer the new size,
            // and the format is agreed again.
            Control::Resize(width, height) => {
                dims.set((width, height));
                let offered = offer((width, height));
                if let Some(pod) = Pod::from_bytes(&offered) {
                    let _ = stream.update_params(&mut [pod]);
                }
            }
        }
    });
    mainloop.run();
    let _ = stream.disconnect();
}

fn jpegs_max(jpegs: &[Vec<u8>]) -> usize {
    jpegs.iter().map(Vec::len).max().unwrap_or(0)
}
