//! Windows' screens and windows (docs/plans/windows.md W6):
//! Windows.Graphics.Capture, with Opencord's own picker in front of it, and
//! GDI where the capture API is missing or does not work
//! (`windows_gdi_capture.rs`).
//!
//! The contract is in `mod.rs`. `start` shares `request.source_id`, one of
//! `sources()`'s ids; frames go to `frames` as packed [`RawFrame`]s (BGRx)
//! with `try_send`, dropping one when the queue is full; a window share's
//! `Picked.sound` is `Source::Process` of the window's process.
//!
//! Windows has no system picker worth putting in front of this, so
//! `own_picker` is true and this file lists the sources itself: every
//! monitor, then the visible windows in the order Windows stacks them,
//! which puts whatever the user was last in at the top. `windows_gdi.rs`
//! next door draws the small pictures the picker shows.
//!
//! Capture runs on a thread of its own, pulling from a free-threaded frame
//! pool rather than waiting on its event: the rate is then ours to set,
//! which is what `request.fps` asks for. Each frame is copied into a
//! staging texture and mapped, because the engine wants rows in memory;
//! converting and scaling on the GPU instead is W6 step 7's, and only if
//! the measurement says so.
//!
//! The capture API needs Windows 10 1903, and some Windows 10 installs that
//! have it still refuse it: N editions, stripped-down builds, and machines
//! whose session opens but never sends a frame. As WebRTC does, the share
//! then falls back to GDI, which every Windows has, so screens can always
//! be shared.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows::Foundation::TimeSpan;
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{
    Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureAccess,
    GraphicsCaptureAccessKind, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::SizeInt32;
use windows::Security::Authorization::AppCapabilityAccess::AppCapabilityAccessStatus;
use windows::Win32::Foundation::{HMODULE, HWND, RECT};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ,
    D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::Graphics::Gdi::HMONITOR;
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::core::Interface;

use super::windows_gdi as gdi;
use super::windows_gdi_capture::{Grabber, Target};
use super::{
    Picked, ScreenError, ScreenSource, ScreenSupport, SourceKind, StartRequest, WindowSound,
};
use crate::share_audio::Source;
use crate::video::camera::{RawFormat, RawFrame};

/// At most this many windows in the picker, newest first (W6 step 1).
const MOST_WINDOWS: usize = 30;

/// Ids: `screen:\\.\DISPLAY1` and `window:<hwnd>`.
const SCREEN_PREFIX: &str = "screen:";
const WINDOW_PREFIX: &str = "window:";

/// How long the capture thread waits before asking the pool again, when
/// there was nothing new.
const POLL: Duration = Duration::from_millis(8);

/// How often to look at whether the shared source is still there.
const GONE_CHECK: Duration = Duration::from_millis(250);

/// How long a screen's capture session has to send its first frame before
/// the share falls back to GDI. A working session sends one at once; a
/// window may rightly send none while it is minimised, so this is only for
/// screens.
const FIRST_FRAME: Duration = Duration::from_secs(3);

/// How long to wait for Windows to answer whether the border may go.
const BORDERLESS_ANSWER: Duration = Duration::from_secs(2);

/// Whether Windows lets captures go without the yellow border, asked once
/// per run. It has to be asked before the border can be turned off; an app
/// that is not packaged is answered at once, without a prompt. Windows 10
/// has no such request, and the answer is no. The wait is bounded, so a
/// share never hangs on it.
fn borderless_allowed() -> bool {
    static ALLOWED: OnceLock<bool> = OnceLock::new();
    *ALLOWED.get_or_init(|| {
        let Ok(request) =
            GraphicsCaptureAccess::RequestAccessAsync(GraphicsCaptureAccessKind::Borderless)
        else {
            return false;
        };
        // `join` cannot be given a timeout, so it waits on a thread of its
        // own, which is left to finish by itself if Windows never answers.
        let (answer, answered) = mpsc::channel();
        let asked = std::thread::Builder::new()
            .name("opencord-capture-border".to_owned())
            .spawn(move || {
                let _ = answer.send(request.join());
            });
        if asked.is_err() {
            return false;
        }
        let allowed = match answered.recv_timeout(BORDERLESS_ANSWER) {
            Ok(status) => status == Ok(AppCapabilityAccessStatus::Allowed),
            Err(_) => {
                tracing::warn!("no answer about the capture border; keeping it");
                false
            }
        };
        tracing::info!(allowed, "asked to share without the capture border");
        allowed
    })
}

fn failed(error: windows::core::Error) -> ScreenError {
    ScreenError::Failed(format!("{error} ({:#010x})", error.code().0))
}

pub(super) fn support() -> ScreenSupport {
    // Where the capture API is missing, GDI shares instead.
    ScreenSupport {
        supported: true,
        reason: None,
        own_picker: true,
    }
}

/// Whether to try Windows.Graphics.Capture. It arrived in Windows 10 1903;
/// asking it is better than reading a build number, since a machine can
/// have the API and still refuse it.
pub(super) fn capture_api_works() -> bool {
    GraphicsCaptureSession::IsSupported().unwrap_or(false)
}

pub(super) async fn sources() -> Result<Vec<ScreenSource>, ScreenError> {
    // Drawing thumbnails is GDI work over every window on the desktop, so
    // it does not belong on a runtime thread.
    tokio::task::spawn_blocking(|| {
        let mut sources = Vec::new();
        for (number, screen) in gdi::monitors().into_iter().enumerate() {
            let (width, height) = size_of_rect(screen.rect);
            sources.push(ScreenSource {
                id: format!("{SCREEN_PREFIX}{}", screen.device),
                kind: SourceKind::Screen,
                title: format!("Screen {} · {width}×{height}", number + 1),
                app: None,
                thumbnail: gdi::thumbnail_of_screen(screen.rect),
            });
        }
        for window in gdi::shareable_windows().into_iter().take(MOST_WINDOWS) {
            sources.push(ScreenSource {
                id: format!("{WINDOW_PREFIX}{}", gdi::handle_id(window.handle)),
                kind: SourceKind::Window,
                title: window.title,
                app: window.app,
                thumbnail: gdi::thumbnail_of_window(window.handle),
            });
        }
        Ok(sources)
    })
    .await
    .unwrap_or_else(|error| Err(ScreenError::Failed(error.to_string())))
}

fn size_of_rect(rect: RECT) -> (i32, i32) {
    (rect.right - rect.left, rect.bottom - rect.top)
}

/// What a source id names. Only ever used on the capture thread.
#[derive(Clone, Copy)]
enum Chosen {
    Screen(HMONITOR),
    Window(HWND),
}

// SAFETY: both are plain handles with no thread affinity for what is done
// with them here — the capture API takes them from any thread. One is made
// on the caller's thread and handed to the capture thread, and only one
// thread ever holds it.
unsafe impl Send for Chosen {}

fn chosen(source_id: Option<&str>) -> Result<Chosen, ScreenError> {
    let id = source_id.ok_or(ScreenError::Cancelled)?;
    if let Some(device) = id.strip_prefix(SCREEN_PREFIX) {
        // Found again by device name, so a list from a moment ago still
        // works after the monitors have been rearranged.
        let gone = || ScreenError::Failed(format!("{device} is not there any more"));
        let screen = gdi::monitors()
            .into_iter()
            .find(|screen| screen.device == device)
            .ok_or_else(gone)?;
        return Ok(Chosen::Screen(screen.handle));
    }
    if let Some(handle) = id.strip_prefix(WINDOW_PREFIX) {
        let window = gdi::window_of_id(handle)
            .ok_or_else(|| ScreenError::Failed(format!("{id} is not a window")))?;
        if !gdi::still_a_window(window) {
            return Err(ScreenError::Failed(
                "that window has been closed".to_owned(),
            ));
        }
        return Ok(Chosen::Window(window));
    }
    Err(ScreenError::Failed(format!("{id} is not a source")))
}

/// A capture in progress; stops when dropped.
pub(super) struct Capture {
    size: (u32, u32),
    /// The source went away: the window was closed, or the monitor
    /// unplugged.
    finished: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Capture {
    pub(super) fn size(&self) -> (u32, u32) {
        self.size
    }

    pub(super) fn finished(&self) -> bool {
        self.finished.load(Ordering::Relaxed)
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub(super) async fn start(
    request: StartRequest,
    frames: SyncSender<RawFrame>,
) -> Result<(Capture, Picked), ScreenError> {
    start_with(request, frames, capture_api_works()).await
}

/// `start`, saying whether to try the capture API before GDI.
async fn start_with(
    request: StartRequest,
    frames: SyncSender<RawFrame>,
    capture_api: bool,
) -> Result<(Capture, Picked), ScreenError> {
    let what = chosen(request.source_id.as_deref())?;
    let picked = Picked {
        kind: match what {
            Chosen::Screen(_) => SourceKind::Screen,
            Chosen::Window(_) => SourceKind::Window,
        },
        // Windows needs none: the id *is* the source, and it keeps working.
        restore_token: None,
        sound: match what {
            Chosen::Screen(_) => None,
            Chosen::Window(window) => window_sound(window),
        },
    };

    let fps = request.fps.max(1);
    match what {
        Chosen::Screen(_) => tracing::info!(kind = "screen", fps, capture_api, "share starting"),
        // The window's app, never its title: titles can hold private text.
        Chosen::Window(window) => tracing::info!(
            kind = "window",
            fps,
            capture_api,
            app = gdi::sound_process_of(window)
                .and_then(gdi::app_of_process)
                .unwrap_or_else(|| "unknown".to_owned()),
            "share starting"
        ),
    }
    let stop = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let (started, outcome) = mpsc::channel();
    let thread = std::thread::Builder::new()
        .name("opencord-screen".to_owned())
        .spawn({
            let (stop, finished) = (Arc::clone(&stop), Arc::clone(&finished));
            move || run(what, fps, capture_api, &frames, &stop, &finished, &started)
        })
        .map_err(|error| ScreenError::Failed(error.to_string()))?;

    match outcome.recv() {
        Ok(Ok(size)) => Ok((
            Capture {
                size,
                finished,
                stop,
                thread: Some(thread),
            },
            picked,
        )),
        Ok(Err(error)) => {
            let _ = thread.join();
            Err(error)
        }
        Err(_) => {
            let _ = thread.join();
            Err(ScreenError::Failed("the capture thread ended".to_owned()))
        }
    }
}

/// Whose sound a window share sends (windows.md §3, decision E).
fn window_sound(window: HWND) -> Option<WindowSound> {
    let pid = gdi::sound_process_of(window)?;
    Some(WindowSound {
        source: Source::Process(pid),
        app: gdi::app_of_process(pid).unwrap_or_else(|| "this window".to_owned()),
    })
}

/// A D3D11 device, and the Direct3D device WinRT wants for the frame pool.
struct Graphics {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    winrt: IDirect3DDevice,
}

fn graphics() -> Result<Graphics, ScreenError> {
    // BGRA support is required by the capture API. Hardware first, then
    // WARP, so a machine without a usable GPU can still share its screen.
    for driver in [D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP] {
        let mut device = None;
        let mut context = None;
        // SAFETY: both out parameters are filled on success and read only
        // then.
        let made = unsafe {
            D3D11CreateDevice(
                None,
                driver,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
        };
        let error = made.as_ref().err().map(ToString::to_string);
        let (Ok(()), Some(device), Some(context)) = (made, device, context) else {
            tracing::info!(?driver, ?error, "no Direct3D 11 device of this kind");
            continue;
        };
        if driver == D3D_DRIVER_TYPE_WARP {
            tracing::warn!("no usable GPU for capture; using WARP, in software");
        }
        // SAFETY: a D3D11 device is a DXGI device; the WinRT wrapper takes
        // its own reference.
        let winrt = unsafe {
            let dxgi: IDXGIDevice = device.cast().map_err(failed)?;
            let inspectable = CreateDirect3D11DeviceFromDXGIDevice(&dxgi).map_err(failed)?;
            inspectable.cast().map_err(failed)?
        };
        return Ok(Graphics {
            device,
            context,
            winrt,
        });
    }
    Err(ScreenError::Failed(
        "no Direct3D 11 device with BGRA support".to_owned(),
    ))
}

fn item_for(what: &Chosen) -> Result<GraphicsCaptureItem, ScreenError> {
    let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()
        .map_err(failed)?;
    // SAFETY: the handle is live — `chosen` has just looked at the window,
    // and the monitor came from an enumeration a moment ago.
    unsafe {
        match what {
            Chosen::Screen(monitor) => interop.CreateForMonitor(*monitor),
            Chosen::Window(window) => interop.CreateForWindow(*window),
        }
    }
    .map_err(failed)
}

fn run(
    what: Chosen,
    fps: u32,
    capture_api: bool,
    frames: &SyncSender<RawFrame>,
    stop: &AtomicBool,
    finished: &AtomicBool,
    started: &mpsc::Sender<Result<(u32, u32), ScreenError>>,
) {
    let trying = Instant::now();
    let session = capture_api
        .then(|| {
            Session::open(what, fps)
                .inspect_err(|error| {
                    tracing::warn!(
                        method = "wgc",
                        %error,
                        after_ms = trying.elapsed().as_millis(),
                        "the capture API refused; using GDI"
                    );
                })
                .ok()
        })
        .flatten();
    if !capture_api {
        tracing::info!(method = "gdi", "the capture API isn't there; using GDI");
    }
    let mut told = false;
    if let Some(session) = session {
        if started.send(Ok(session.size)).is_err() {
            return;
        }
        told = true;
        match session.read(frames, stop, finished) {
            Ended::Done => return,
            Ended::Silent => tracing::warn!(
                method = "wgc",
                waited_s = FIRST_FRAME.as_secs(),
                "the capture API sent no frame; using GDI"
            ),
        }
    }
    read_with_gdi(
        what,
        fps,
        frames,
        stop,
        finished,
        (!told).then_some(started),
    );
}

/// Why a capture session's loop ended.
enum Ended {
    /// Stopped, or the source went away.
    Done,
    /// A screen's session sent nothing: GDI takes over.
    Silent,
}

/// Captures with GDI until stopped or the source goes away. `started` is
/// told the size unless the capture API already did.
fn read_with_gdi(
    what: Chosen,
    fps: u32,
    frames: &SyncSender<RawFrame>,
    stop: &AtomicBool,
    finished: &AtomicBool,
    started: Option<&mpsc::Sender<Result<(u32, u32), ScreenError>>>,
) {
    let mut grabber = Grabber::new(match what {
        Chosen::Screen(monitor) => Target::Screen(monitor),
        Chosen::Window(window) => Target::Window(window),
    });
    if let Some(started) = started {
        let Some(size) = grabber.size() else {
            let _ = started.send(Err(ScreenError::Failed(
                "the source has no size to share".to_owned(),
            )));
            return;
        };
        tracing::info!(?size, fps, "sharing a screen with GDI");
        if started.send(Ok(size)).is_err() {
            return;
        }
    }
    let gap = Duration::from_secs_f64(1.0 / f64::from(fps));
    let mut next = Instant::now();
    let mut looked = Instant::now();
    let mut counts = Counts::new("gdi");
    while !stop.load(Ordering::Relaxed) {
        if looked.elapsed() >= GONE_CHECK {
            looked = Instant::now();
            if gone(&what) {
                tracing::info!(method = "gdi", cause = "source closed", "capture stopped");
                finished.store(true, Ordering::Relaxed);
                return;
            }
        }
        // Nothing while minimised: the sender keeps the last picture going
        // (§9.4).
        match grabber.grab() {
            // Dropped while the encoder is behind.
            Some(raw) => counts.grabbed(frames.try_send(raw).is_ok()),
            None => counts.empty(),
        }
        counts.log_when_due();
        next = pace(next, gap);
    }
    tracing::info!(method = "gdi", cause = "stopped", "capture stopped");
}

/// A capture loop's counters, logged every few seconds (D82).
struct Counts {
    method: &'static str,
    since: Instant,
    first: Option<Instant>,
    grabbed: u32,
    queue_full: u32,
    empty: u32,
}

impl Counts {
    fn new(method: &'static str) -> Self {
        Self {
            method,
            since: Instant::now(),
            first: Some(Instant::now()),
            grabbed: 0,
            queue_full: 0,
            empty: 0,
        }
    }

    /// A frame captured; `queued` unless the sender was still busy.
    fn grabbed(&mut self, queued: bool) {
        if let Some(opened) = self.first.take() {
            tracing::info!(
                method = self.method,
                after_ms = opened.elapsed().as_millis(),
                "first frame captured"
            );
        }
        self.grabbed += 1;
        self.queue_full += u32::from(!queued);
    }

    /// Nothing to capture this time (a minimised window).
    fn empty(&mut self) {
        self.empty += 1;
    }

    fn log_when_due(&mut self) {
        if self.since.elapsed() < crate::video::stats::EVERY {
            return;
        }
        tracing::info!(
            method = self.method,
            grabbed = self.grabbed,
            queue_full = self.queue_full,
            nothing_new = self.empty,
            "capturing"
        );
        *self = Self {
            first: None,
            ..Self::new(self.method)
        };
    }
}

/// Sleeps until the next frame is due, and says when the one after is.
fn pace(next: Instant, gap: Duration) -> Instant {
    let next = next + gap;
    let now = Instant::now();
    if next > now {
        std::thread::sleep(next - now);
        next
    } else {
        // Behind: let the lost time go rather than catching up in a burst.
        now
    }
}

/// Everything one share needs, torn down together.
struct Session {
    /// Kept so the loop can ask whether the source is still there.
    what: Chosen,
    graphics: Graphics,
    item: GraphicsCaptureItem,
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    closed: i64,
    gone: Arc<AtomicBool>,
    size: (u32, u32),
    /// Whether Windows holds the frame rate down itself.
    paced: bool,
    fps: u32,
}

impl Session {
    fn open(what: Chosen, fps: u32) -> Result<Self, ScreenError> {
        let graphics = graphics()?;
        let item = item_for(&what)?;
        let size = item.Size().map_err(failed)?;

        // The source going away is an event, not something polling would
        // see: a closed window simply stops producing frames, and so does
        // a minimised one.
        let gone = Arc::new(AtomicBool::new(false));
        let closed = item
            .Closed(&TypedEventHandler::new({
                let gone = Arc::clone(&gone);
                move |_: windows::core::Ref<'_, GraphicsCaptureItem>, _| {
                    gone.store(true, Ordering::Relaxed);
                    Ok(())
                }
            }))
            .map_err(failed)?;

        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &graphics.winrt,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            2,
            size,
        )
        .map_err(failed)?;
        let session = pool.CreateCaptureSession(&item).map_err(failed)?;
        // The cursor belongs in a screen share (§9).
        let _ = session.SetIsCursorCaptureEnabled(true);
        // No yellow border around what is shared, where Windows lets it go
        // (Windows 11); Windows 10 always draws it.
        let asked = Instant::now();
        if borderless_allowed() {
            let _ = session.SetIsBorderRequired(false);
        }
        tracing::debug!(
            after_ms = asked.elapsed().as_millis(),
            "the border was settled"
        );
        // Windows 11 can hold the rate down itself, which saves the copy a
        // frame we would only drop costs. Where it cannot, `read` paces.
        let paced = session
            .SetMinUpdateInterval(TimeSpan {
                // 100-nanosecond units.
                Duration: 10_000_000 / i64::from(fps),
            })
            .is_ok();
        session.StartCapture().map_err(failed)?;

        let size = (size.Width.max(0) as u32, size.Height.max(0) as u32);
        tracing::info!(?size, fps, paced, "sharing a screen");
        Ok(Self {
            what,
            graphics,
            item,
            pool,
            session,
            closed,
            gone,
            size,
            paced,
            fps,
        })
    }

    fn read(
        &self,
        frames: &SyncSender<RawFrame>,
        stop: &AtomicBool,
        finished: &AtomicBool,
    ) -> Ended {
        let opened = Instant::now();
        let mut any = false;
        let mut staging: Option<(ID3D11Texture2D, (u32, u32))> = None;
        let mut pool_size = SizeInt32 {
            Width: self.size.0 as i32,
            Height: self.size.1 as i32,
        };
        let gap = Duration::from_secs_f64(1.0 / f64::from(self.fps));
        let mut next = Instant::now();
        let mut looked = Instant::now();
        let mut counts = Counts::new("wgc");
        while !stop.load(Ordering::Relaxed) {
            counts.log_when_due();
            // Whether the source is still there is asked two ways. The
            // `Closed` event is the tidy one, but it is a WinRT event and
            // this thread pumps no messages, so it never arrives — measured:
            // a closed Notepad left the share running for the full ten
            // seconds the test waited. So the handle is also looked at
            // directly, which is what actually ends the share.
            if looked.elapsed() >= GONE_CHECK {
                looked = Instant::now();
                if gone(&self.what) {
                    tracing::info!(method = "wgc", cause = "source closed", "capture stopped");
                    finished.store(true, Ordering::Relaxed);
                    return Ended::Done;
                }
            }
            if self.gone.load(Ordering::Relaxed) {
                tracing::info!(method = "wgc", cause = "window closed", "capture stopped");
                finished.store(true, Ordering::Relaxed);
                return Ended::Done;
            }
            // No frame yet is an error here, not an empty option: a
            // minimised window produces nothing at all, and the sender
            // keeps its last picture going at 1 fps (§9.4), so there is
            // nothing to do but wait. The source actually going away is
            // the `Closed` event above, not this.
            let Ok(frame) = self.pool.TryGetNextFrame() else {
                if !any && matches!(self.what, Chosen::Screen(_)) && opened.elapsed() >= FIRST_FRAME
                {
                    return Ended::Silent;
                }
                counts.empty();
                std::thread::sleep(POLL);
                continue;
            };
            any = true;

            // The content can be resized under us, and the pool has to
            // follow or every frame after it comes out the old size.
            match frame.ContentSize() {
                Ok(content)
                    if (content.Width, content.Height) != (pool_size.Width, pool_size.Height)
                        && content.Width > 0
                        && content.Height > 0 =>
                {
                    pool_size = content;
                    staging = None;
                    let _ = self.pool.Recreate(
                        &self.graphics.winrt,
                        DirectXPixelFormat::B8G8R8A8UIntNormalized,
                        2,
                        content,
                    );
                    continue;
                }
                _ => {}
            }

            if let Some(raw) = copy_out(&self.graphics, &frame, &mut staging) {
                // Dropped while the encoder is behind.
                counts.grabbed(frames.try_send(raw).is_ok());
            }

            if !self.paced {
                next = pace(next, gap);
            }
        }
        tracing::info!(method = "wgc", cause = "stopped", "capture stopped");
        Ended::Done
    }
}

/// Whether what is being shared has gone: the window closed, or the monitor
/// was unplugged.
fn gone(what: &Chosen) -> bool {
    match what {
        Chosen::Window(window) => !gdi::still_a_window(*window),
        Chosen::Screen(monitor) => !gdi::monitors()
            .iter()
            .any(|screen| screen.handle == *monitor),
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.session.Close();
        let _ = self.pool.Close();
        let _ = self.item.RemoveClosed(self.closed);
    }
}

/// One captured frame's pixels, in memory. The texture the capture API
/// hands over lives on the GPU and cannot be read directly, so it is copied
/// into a staging texture, which can be.
fn copy_out(
    graphics: &Graphics,
    frame: &Direct3D11CaptureFrame,
    staging: &mut Option<(ID3D11Texture2D, (u32, u32))>,
) -> Option<RawFrame> {
    let captured = Instant::now();
    let surface = frame.Surface().ok()?;
    let access: IDirect3DDxgiInterfaceAccess = surface.cast().ok()?;
    // SAFETY: the surface wraps a texture, which is what this asks for;
    // the reference is ours to hold.
    let texture: ID3D11Texture2D = unsafe { access.GetInterface() }.ok()?;
    let mut description = D3D11_TEXTURE2D_DESC::default();
    // SAFETY: filled in by the texture just taken.
    unsafe { texture.GetDesc(&mut description) };
    let size = (description.Width, description.Height);
    if size.0 == 0 || size.1 == 0 {
        return None;
    }

    if staging.as_ref().is_none_or(|(_, had)| *had != size) {
        let wanted = D3D11_TEXTURE2D_DESC {
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
            ..description
        };
        let mut made = None;
        // SAFETY: a plain staging texture of the same size and format.
        unsafe {
            graphics
                .device
                .CreateTexture2D(&wanted, None, Some(&mut made))
        }
        .ok()?;
        *staging = Some((made?, size));
    }
    let (into, _) = staging.as_ref()?;

    // SAFETY: both textures have the same size and format, so the copy is
    // valid; the map is released before this returns, on every path.
    unsafe {
        graphics.context.CopyResource(into, &texture);
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        graphics
            .context
            .Map(into, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
            .ok()?;
        let rows = size.1 as usize;
        let stride = mapped.RowPitch as usize;
        let row = size.0 as usize * 4;
        let mut data = Vec::with_capacity(row * rows);
        if stride >= row {
            let bytes = std::slice::from_raw_parts(mapped.pData.cast::<u8>(), stride * rows);
            for line in 0..rows {
                let from = line * stride;
                data.extend_from_slice(&bytes[from..from + row]);
            }
        }
        graphics.context.Unmap(into, 0);
        (data.len() == row * rows).then_some(RawFrame {
            format: RawFormat::Bgrx,
            width: size.0,
            height: size.1,
            data,
            captured,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_can_share_its_screens() {
        let support = support();
        assert!(support.supported, "GDI shares where the capture API cannot");
        assert!(support.own_picker, "Opencord picks the source on Windows");
        assert!(support.reason.is_none());
    }

    /// The fallback end to end: the first screen shared as a Windows
    /// without the capture API would share it.
    #[tokio::test]
    async fn a_screen_is_shared_with_gdi_when_the_capture_api_is_not_there() {
        let screen = gdi::monitors().into_iter().next().expect("a monitor");
        let (frames, arriving) = mpsc::sync_channel(4);
        let request = StartRequest {
            fps: 30,
            max_pixels: 1920 * 1080,
            restore_token: None,
            source_id: Some(format!("{SCREEN_PREFIX}{}", screen.device)),
        };
        let (capture, picked) = start_with(request, frames, false)
            .await
            .expect("GDI starts");
        assert_eq!(picked.kind, SourceKind::Screen);
        let (width, height) = size_of_rect(screen.rect);
        assert_eq!(capture.size(), (width as u32, height as u32));

        let frame = arriving
            .recv_timeout(Duration::from_secs(5))
            .expect("a frame within five seconds");
        assert_eq!(frame.format, RawFormat::Bgrx);
        assert_eq!((frame.width, frame.height), capture.size());
        assert_eq!(
            frame.data.len(),
            frame.width as usize * frame.height as usize * 4
        );
        assert!(
            arriving.recv_timeout(Duration::from_secs(2)).is_ok(),
            "and they keep coming"
        );
        assert!(!capture.finished());
    }

    #[tokio::test]
    async fn the_sources_are_screens_then_windows_with_ids_that_parse_back() {
        let found = sources().await.expect("the sources");
        let screens = found
            .iter()
            .filter(|source| source.kind == SourceKind::Screen)
            .count();
        assert!(screens >= 1, "a PC has a monitor");
        assert!(
            found[..screens]
                .iter()
                .all(|source| source.kind == SourceKind::Screen),
            "screens come first"
        );
        assert!(
            found.len() - screens <= MOST_WINDOWS,
            "at most {MOST_WINDOWS} windows, newest first"
        );
        for source in &found {
            assert!(!source.title.is_empty(), "every source has a title");
            match source.kind {
                SourceKind::Screen => {
                    assert!(source.id.starts_with(SCREEN_PREFIX));
                    assert!(source.app.is_none(), "a screen belongs to no app");
                    assert!(source.title.contains('·'), "{}", source.title);
                }
                SourceKind::Window => assert!(source.id.starts_with(WINDOW_PREFIX)),
            }
            if let Some(thumbnail) = &source.thumbnail {
                assert!(thumbnail.width <= 320 && thumbnail.height <= 180);
                assert_eq!(
                    thumbnail.rgba.len(),
                    thumbnail.width as usize * thumbnail.height as usize * 4
                );
            }
            // Every id has to survive the round trip, or the picker's
            // choice could not be started.
            assert!(
                chosen(Some(&source.id)).is_ok(),
                "{} does not name anything",
                source.id
            );
        }
    }

    #[test]
    fn asking_to_lose_the_border_answers_in_time() {
        let asking = Instant::now();
        let allowed = borderless_allowed();
        eprintln!("borderless capture allowed: {allowed}");
        assert!(
            asking.elapsed() < BORDERLESS_ANSWER + Duration::from_secs(1),
            "a share never waits long on the border"
        );
        assert_eq!(borderless_allowed(), allowed, "asked once per run");
    }

    #[test]
    fn nothing_chosen_and_nonsense_are_told_apart() {
        assert!(matches!(chosen(None), Err(ScreenError::Cancelled)));
        assert!(matches!(
            chosen(Some("nonsense")),
            Err(ScreenError::Failed(_))
        ));
        assert!(matches!(
            chosen(Some("window:not-a-number")),
            Err(ScreenError::Failed(_))
        ));
        assert!(matches!(
            chosen(Some(r"screen:\\.\NOSUCHDISPLAY")),
            Err(ScreenError::Failed(_))
        ));
    }
}
