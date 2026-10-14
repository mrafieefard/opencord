//! macOS's screens (docs/plans/macos.md M6): ScreenCaptureKit, with the
//! system's `SCContentSharingPicker` choosing a display or a window (macOS
//! 14 and later). `OPENCORD_SCREEN_DISPLAY=main` shares the main display
//! without the picker, for checks with nobody to pick; that path needs
//! Screen Recording for the app (or the terminal that started it).
//!
//! The contract is in `mod.rs`. Frames go to `frames` as packed
//! [`RawFrame`]s (NV12 from ScreenCaptureKit) with `try_send`, dropping one
//! when the queue is full; `Picked.sound` names the window's app when the
//! picker's filter says which it is (macOS 15.2 and later).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use block2::RcBlock;
use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2::runtime::{NSObjectProtocol, ProtocolObject};
use objc2::{AnyThread, DefinedClass, Message, available, define_class, msg_send};
use objc2_core_media::{CMSampleBuffer, CMTime, CMTimeFlags};
use objc2_foundation::{NSArray, NSBundle, NSDictionary, NSError, NSNumber, NSObject, NSString};
use objc2_screen_capture_kit::{
    SCContentFilter, SCContentSharingPicker, SCContentSharingPickerMode,
    SCContentSharingPickerObserver, SCFrameStatus, SCShareableContent, SCShareableContentStyle,
    SCStream, SCStreamConfiguration, SCStreamDelegate, SCStreamFrameInfoContentRect,
    SCStreamFrameInfoScaleFactor, SCStreamFrameInfoStatus, SCStreamOutput, SCStreamOutputType,
};

use super::{
    Picked, ScreenError, ScreenSource, ScreenSupport, SourceKind, StartRequest, WindowSound,
};
use crate::share_audio::Source;
use crate::video::camera::{RawFormat, RawFrame};
use crate::video::codec::macos::{ffi, nv12_picture};

/// Shares the main display without the picker when set to `main`.
const DISPLAY_VARIABLE: &str = "OPENCORD_SCREEN_DISPLAY";

/// The most pixels captured: the largest preset's (`Source`, 3840×2160).
const MOST_PIXELS: u32 = 3840 * 2160;
/// The fewest pixels a share's capture budget gets: 1080p's, so a share
/// whose quality changes between 480p and 1080p while live loses nothing.
/// ScreenCaptureKit scales on the GPU; the sender's own scaling of a Retina
/// display's pixels cost 3½ times the CPU and a fifth of the frame rate
/// (§7, M6).
const FEWEST_PIXELS: u32 = 1920 * 1080;

/// How long `start` waits for the first picture before going on anyway.
const FIRST_PICTURE: Duration = Duration::from_secs(3);

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGMainDisplayID() -> u32;
}

pub(super) fn support() -> ScreenSupport {
    if available!(macos = 14.0) {
        ScreenSupport {
            supported: true,
            reason: None,
            own_picker: false,
        }
    } else {
        ScreenSupport {
            supported: false,
            reason: Some("Screen sharing needs macOS 14 or later".to_owned()),
            own_picker: false,
        }
    }
}

pub(super) async fn sources() -> Result<Vec<ScreenSource>, ScreenError> {
    Ok(Vec::new())
}

/// The pixel budget for a share's `max_pixels`.
fn budget(max_pixels: u32) -> u32 {
    max_pixels.clamp(FEWEST_PIXELS, MOST_PIXELS)
}

/// The capture size for content of `width` × `height` points at `scale`
/// pixels a point: its pixels, at most `budget`, its shape kept, even.
fn capture_size(width: f64, height: f64, scale: f64, budget: u32) -> (u32, u32) {
    let pixels = |points: f64| (points * scale).round().max(2.0) as u32;
    let (width, height) =
        opencord_common::video::scaled_to_fit(pixels(width), pixels(height), budget);
    (width.max(2), height.max(2))
}

/// What the stream's callbacks share with the capture.
struct Shared {
    /// Where frames go; taken when the stream stops, which closes the queue.
    sender: Mutex<Option<SyncSender<RawFrame>>>,
    finished: AtomicBool,
    /// The size asked of the stream now.
    size: Mutex<(u32, u32)>,
    /// The most pixels to capture.
    budget: u32,
    /// Told once the first complete picture came.
    first: Mutex<Option<std::sync::mpsc::Sender<()>>>,
}

impl Shared {
    fn finish(&self) {
        self.finished.store(true, Ordering::Relaxed);
        if let Ok(mut sender) = self.sender.lock() {
            sender.take();
        }
    }
}

struct HandlerIvars {
    shared: Arc<Shared>,
    configuration: Retained<SCStreamConfiguration>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; no `Drop`.
    #[unsafe(super(NSObject))]
    #[name = "OpencordScreenHandler"]
    #[ivars = HandlerIvars]
    struct Handler;

    unsafe impl NSObjectProtocol for Handler {}

    unsafe impl SCStreamOutput for Handler {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        fn did_output(&self, stream: &SCStream, sample: &CMSampleBuffer, kind: SCStreamOutputType) {
            if kind == SCStreamOutputType::Screen {
                self.picture(stream, sample);
            }
        }
    }

    unsafe impl SCStreamDelegate for Handler {
        #[unsafe(method(stream:didStopWithError:))]
        fn did_stop(&self, _stream: &SCStream, error: &NSError) {
            tracing::info!(error = %error.localizedDescription(), "the screen share's stream stopped");
            self.ivars().shared.finish();
        }
    }
);

impl Handler {
    fn new(shared: Arc<Shared>, configuration: Retained<SCStreamConfiguration>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(HandlerIvars {
            shared,
            configuration,
        });
        // SAFETY: NSObject's `init`.
        unsafe { msg_send![super(this), init] }
    }

    /// A screen sample: only complete pictures go on; a new content size
    /// (a window resized) asks the stream for that size.
    fn picture(&self, stream: &SCStream, sample: &CMSampleBuffer) {
        let captured = Instant::now();
        let shared = &self.ivars().shared;
        let sample: *const CMSampleBuffer = sample;
        let sample = sample.cast_mut().cast::<std::ffi::c_void>();
        // SAFETY: the sample is valid for the call.
        let Some(info) = (unsafe { frame_info(sample) }) else {
            return;
        };
        if info.status != SCFrameStatus::Complete {
            return;
        }
        if let Some((width, height, scale)) = info.content {
            let wanted = capture_size(width, height, scale, shared.budget);
            let mut size = shared.size.lock().unwrap_or_else(|e| e.into_inner());
            if wanted != *size {
                *size = wanted;
                let configuration = &self.ivars().configuration;
                // SAFETY: the stream's own configuration object, updated in
                // place and handed back.
                unsafe {
                    configuration.setWidth(wanted.0 as usize);
                    configuration.setHeight(wanted.1 as usize);
                    stream.updateConfiguration_completionHandler(configuration, None);
                }
                tracing::debug!(?wanted, "the shared content changed size");
            }
        }
        // SAFETY: the sample's image buffer, valid for the call.
        let picture = unsafe { nv12_picture(ffi::CMSampleBufferGetImageBuffer(sample), captured) };
        let Some(picture) = picture else {
            return;
        };
        if let Some(first) = shared.first.lock().ok().and_then(|mut first| first.take()) {
            let _ = first.send(());
        }
        let frame = RawFrame {
            format: RawFormat::Nv12,
            width: picture.width,
            height: picture.height,
            data: picture.data,
            captured,
        };
        let sender = shared.sender.lock().ok();
        if let Some(Err(TrySendError::Disconnected(_))) = sender
            .as_ref()
            .and_then(|sender| sender.as_ref())
            .map(|sender| sender.try_send(frame))
        {
            tracing::debug!("the screen's frames are no longer wanted");
        }
    }
}

/// What ScreenCaptureKit says about a picture.
struct FrameInfo {
    status: SCFrameStatus,
    /// The content's size in points, and pixels a point.
    content: Option<(f64, f64, f64)>,
}

/// A screen sample's frame info, from its attachments.
///
/// # Safety
///
/// `sample` is a valid sample buffer.
unsafe fn frame_info(sample: ffi::Ref) -> Option<FrameInfo> {
    // SAFETY: the caller's promise; the array is the sample's (CFArray and
    // NSArray, CFDictionary and NSDictionary are toll-free bridged).
    unsafe {
        let attachments = ffi::CMSampleBufferGetSampleAttachmentsArray(sample, 0);
        let attachments = attachments
            .cast::<NSArray<NSDictionary<NSString, objc2::runtime::AnyObject>>>()
            .as_ref()?;
        let info = attachments.firstObject()?;
        let number = |key: &NSString| -> Option<Retained<NSNumber>> {
            info.objectForKey(key)?.downcast::<NSNumber>().ok()
        };
        let status = SCFrameStatus(number(SCStreamFrameInfoStatus)?.integerValue());
        let rect = info
            .objectForKey(SCStreamFrameInfoContentRect)
            .and_then(|rect| rect.downcast::<NSDictionary>().ok());
        let side = |rect: &NSDictionary, name: &str| -> Option<f64> {
            let key = NSString::from_str(name);
            let value: Option<Retained<objc2::runtime::AnyObject>> =
                msg_send![rect, objectForKey: &*key];
            Some(value?.downcast::<NSNumber>().ok()?.doubleValue())
        };
        let content = rect.and_then(|rect| {
            let scale = number(SCStreamFrameInfoScaleFactor)?.doubleValue();
            Some((side(&rect, "Width")?, side(&rect, "Height")?, scale))
        });
        Some(FrameInfo { status, content })
    }
}

/// The picker's answer.
enum Choice {
    Picked(Sendable<Retained<SCContentFilter>>),
    Cancelled,
    Failed(String),
}

struct ObserverIvars {
    answer: Mutex<Option<tokio::sync::oneshot::Sender<Choice>>>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; no `Drop`.
    #[unsafe(super(NSObject))]
    #[name = "OpencordScreenPickerObserver"]
    #[ivars = ObserverIvars]
    struct PickerObserver;

    unsafe impl NSObjectProtocol for PickerObserver {}

    unsafe impl SCContentSharingPickerObserver for PickerObserver {
        #[unsafe(method(contentSharingPicker:didCancelForStream:))]
        fn did_cancel(&self, _picker: &SCContentSharingPicker, _stream: Option<&SCStream>) {
            self.answer(Choice::Cancelled);
        }

        #[unsafe(method(contentSharingPicker:didUpdateWithFilter:forStream:))]
        fn did_pick(
            &self,
            _picker: &SCContentSharingPicker,
            filter: &SCContentFilter,
            _stream: Option<&SCStream>,
        ) {
            self.answer(Choice::Picked(Sendable(filter.retain())));
        }

        #[unsafe(method(contentSharingPickerStartDidFailWithError:))]
        fn did_fail(&self, error: &NSError) {
            self.answer(Choice::Failed(error.localizedDescription().to_string()));
        }
    }
);

impl PickerObserver {
    fn new(answer: tokio::sync::oneshot::Sender<Choice>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(ObserverIvars {
            answer: Mutex::new(Some(answer)),
        });
        // SAFETY: NSObject's `init`.
        unsafe { msg_send![super(this), init] }
    }

    fn answer(&self, choice: Choice) {
        if let Some(answer) = self.ivars().answer.lock().ok().and_then(|mut a| a.take()) {
            let _ = answer.send(choice);
        }
    }
}

/// Something only sent between threads: an Objective-C object used on one
/// thread at a time, as ScreenCaptureKit allows for these.
struct Sendable<T>(T);

// SAFETY: see above; each use is on one thread at a time.
unsafe impl<T> Send for Sendable<T> {}

/// The system picker, on the main thread: one display or one window, never
/// Opencord's own.
async fn pick() -> Result<Retained<SCContentFilter>, ScreenError> {
    let (answer, answered) = tokio::sync::oneshot::channel();
    let observer = Sendable(PickerObserver::new(answer));
    let shown = Sendable(observer.0.clone());
    DispatchQueue::main().exec_async(move || {
        let observer = shown;
        // SAFETY: the shared picker, configured and shown on the main
        // thread.
        unsafe {
            let picker = SCContentSharingPicker::sharedPicker();
            let configuration = picker.defaultConfiguration();
            configuration.setAllowedPickerModes(
                SCContentSharingPickerMode::SingleDisplay
                    | SCContentSharingPickerMode::SingleWindow,
            );
            if let Some(own) = NSBundle::mainBundle().bundleIdentifier() {
                configuration.setExcludedBundleIDs(&NSArray::from_retained_slice(&[own]));
            }
            picker.setDefaultConfiguration(&configuration);
            picker.addObserver(ProtocolObject::from_ref(&*observer.0));
            picker.setActive(true);
            picker.present();
        }
    });
    let choice = answered.await;
    let done = observer;
    DispatchQueue::main().exec_async(move || {
        let observer = done;
        // SAFETY: as above.
        unsafe {
            let picker = SCContentSharingPicker::sharedPicker();
            picker.removeObserver(ProtocolObject::from_ref(&*observer.0));
            picker.setActive(false);
        }
    });
    match choice {
        Ok(Choice::Picked(filter)) => Ok(filter.0),
        Ok(Choice::Cancelled) => Err(ScreenError::Cancelled),
        Ok(Choice::Failed(message)) => Err(ScreenError::Failed(message)),
        Err(_) => Err(ScreenError::Failed("the picker went away".to_owned())),
    }
}

/// The main display, without the picker.
async fn main_display() -> Result<Retained<SCContentFilter>, ScreenError> {
    let (answer, answered) = tokio::sync::oneshot::channel();
    {
        let answer = Mutex::new(Some(answer));
        let block = RcBlock::new(
            move |content: *mut SCShareableContent, error: *mut NSError| {
                // SAFETY: valid for the call, or null.
                let result = match unsafe { (content.as_ref(), error.as_ref()) } {
                    (Some(content), _) => {
                        // SAFETY: property reads.
                        let main = unsafe { CGMainDisplayID() };
                        let display = unsafe { content.displays() }
                            .to_vec()
                            .into_iter()
                            .find(|display| unsafe { display.displayID() } == main);
                        match display {
                            Some(display) => {
                                // SAFETY: a filter of a display the system gave.
                                let filter = unsafe {
                                    SCContentFilter::initWithDisplay_excludingWindows(
                                        SCContentFilter::alloc(),
                                        &display,
                                        &NSArray::new(),
                                    )
                                };
                                Ok(Sendable(filter))
                            }
                            None => Err(ScreenError::Failed("no main display".to_owned())),
                        }
                    }
                    (None, Some(error)) if error.code() == -3801 => Err(ScreenError::Denied),
                    (None, Some(error)) => Err(ScreenError::Failed(
                        error.localizedDescription().to_string(),
                    )),
                    (None, None) => Err(ScreenError::Failed("no shareable content".to_owned())),
                };
                if let Some(answer) = answer.lock().ok().and_then(|mut a| a.take()) {
                    let _ = answer.send(result);
                }
            },
        );
        // SAFETY: the block may run on any thread; it only sends.
        unsafe { SCShareableContent::getShareableContentWithCompletionHandler(&block) };
    }
    match answered.await {
        Ok(result) => result.map(|filter| filter.0),
        Err(_) => Err(ScreenError::Failed(
            "no answer about the displays".to_owned(),
        )),
    }
}

/// A window filter's app, as its sound source (macOS 15.2 and later).
fn window_sound(filter: &SCContentFilter) -> Option<WindowSound> {
    if !available!(macos = 15.2) {
        return None;
    }
    // SAFETY: property reads on 15.2 and later.
    unsafe {
        let window = filter.includedWindows().firstObject()?;
        let app = window.owningApplication()?;
        Some(WindowSound {
            source: Source::App(app.bundleIdentifier().to_string()),
            app: app.applicationName().to_string(),
        })
    }
}

pub(super) struct Capture {
    stream: Retained<SCStream>,
    _handler: Retained<Handler>,
    shared: Arc<Shared>,
    size: (u32, u32),
}

// SAFETY: the stream is only stopped from another thread, which
// ScreenCaptureKit allows; its callbacks run on their own queue.
unsafe impl Send for Capture {}

impl Capture {
    pub(super) fn size(&self) -> (u32, u32) {
        self.size
    }

    pub(super) fn finished(&self) -> bool {
        self.shared.finished.load(Ordering::Relaxed)
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let (done, stopped) = std::sync::mpsc::channel();
        let block = RcBlock::new(move |_: *mut NSError| {
            let _ = done.send(());
        });
        // SAFETY: a running stream; the block only sends.
        unsafe { self.stream.stopCaptureWithCompletionHandler(Some(&block)) };
        let _ = stopped.recv_timeout(Duration::from_secs(1));
        self.shared.finish();
    }
}

pub(super) async fn start(
    request: StartRequest,
    frames: SyncSender<RawFrame>,
) -> Result<(Capture, Picked), ScreenError> {
    if !support().supported {
        return Err(ScreenError::NotSupported);
    }
    let filter = if std::env::var(DISPLAY_VARIABLE).is_ok_and(|value| value == "main") {
        main_display().await?
    } else {
        pick().await?
    };
    let filter = Sendable(filter);
    tokio::task::spawn_blocking(move || {
        let filter = filter;
        capture(&filter.0, request.fps, budget(request.max_pixels), frames)
    })
    .await
    .map_err(|error| ScreenError::Failed(error.to_string()))?
}

/// Captures what `filter` shows, once it has a first picture or after
/// [`FIRST_PICTURE`].
fn capture(
    filter: &SCContentFilter,
    fps: u32,
    budget: u32,
    frames: SyncSender<RawFrame>,
) -> Result<(Capture, Picked), ScreenError> {
    // SAFETY: property reads of a filter (14.0 and later).
    let (style, rect, scale) = unsafe {
        (
            filter.style(),
            filter.contentRect(),
            filter.pointPixelScale(),
        )
    };
    let size = capture_size(rect.size.width, rect.size.height, f64::from(scale), budget);
    let kind = if style == SCShareableContentStyle::Window {
        SourceKind::Window
    } else {
        SourceKind::Screen
    };
    let sound = (kind == SourceKind::Window)
        .then(|| window_sound(filter))
        .flatten();

    let (first, first_seen) = std::sync::mpsc::channel();
    let shared = Arc::new(Shared {
        sender: Mutex::new(Some(frames)),
        finished: AtomicBool::new(false),
        size: Mutex::new(size),
        budget,
        first: Mutex::new(Some(first)),
    });
    // SAFETY: ScreenCaptureKit calls with valid arguments.
    let (stream, handler) = unsafe {
        let configuration = SCStreamConfiguration::new();
        configuration.setWidth(size.0 as usize);
        configuration.setHeight(size.1 as usize);
        configuration.setMinimumFrameInterval(CMTime {
            value: 1,
            timescale: fps.max(1) as i32,
            flags: CMTimeFlags::Valid,
            epoch: 0,
        });
        configuration.setPixelFormat(ffi::NV12_VIDEO_RANGE);
        configuration.setShowsCursor(true);
        configuration.setQueueDepth(5);
        let handler = Handler::new(Arc::clone(&shared), configuration.clone());
        let stream = SCStream::initWithFilter_configuration_delegate(
            SCStream::alloc(),
            filter,
            &configuration,
            Some(ProtocolObject::from_ref(&*handler)),
        );
        let queue = DispatchQueue::new("opencord-screen", None);
        stream
            .addStreamOutput_type_sampleHandlerQueue_error(
                ProtocolObject::from_ref(&*handler),
                SCStreamOutputType::Screen,
                Some(&queue),
            )
            .map_err(|error| ScreenError::Failed(error.localizedDescription().to_string()))?;
        (stream, handler)
    };

    let (done, started) = std::sync::mpsc::channel();
    let block = RcBlock::new(move |error: *mut NSError| {
        // SAFETY: valid for the call, or null.
        let result = match unsafe { error.as_ref() } {
            None => Ok(()),
            Some(error) if error.code() == -3801 => Err(ScreenError::Denied),
            Some(error) => Err(ScreenError::Failed(
                error.localizedDescription().to_string(),
            )),
        };
        let _ = done.send(result);
    });
    // SAFETY: a stream with its output; the block only sends.
    unsafe { stream.startCaptureWithCompletionHandler(Some(&block)) };
    started
        .recv_timeout(Duration::from_secs(10))
        .map_err(|_| ScreenError::Failed("the screen capture did not start".to_owned()))??;
    if first_seen.recv_timeout(FIRST_PICTURE).is_err() {
        tracing::debug!("no picture yet from the screen capture; going on");
    }
    tracing::debug!(?size, ?kind, fps, "screen capture started");
    Ok((
        Capture {
            stream,
            _handler: handler,
            shared,
            size,
        },
        Picked {
            kind,
            restore_token: None,
            sound,
        },
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    #[test]
    fn content_is_captured_at_its_pixels_within_the_shares_budget() {
        let source = budget(3840 * 2160);
        assert_eq!(capture_size(1440.0, 900.0, 2.0, source), (2880, 1800));
        assert_eq!(capture_size(2560.0, 1440.0, 2.0, source), (3840, 2160));
        assert_eq!(
            capture_size(1680.0, 1050.0, 2.0, budget(1920 * 1080)),
            (1820, 1138)
        );
        assert_eq!(
            capture_size(1680.0, 1050.0, 2.0, budget(1280 * 720)),
            (1820, 1138),
            "never under 1080p's budget"
        );
        assert_eq!(capture_size(1440.0, 900.0, 1.0, source), (1440, 900));
        assert_eq!(capture_size(801.0, 601.0, 1.0, source), (800, 600), "even");
    }

    #[test]
    fn screens_need_macos_14() {
        let support = support();

        assert_eq!(support.supported, available!(macos = 14.0));
        assert!(!support.own_picker, "the system picks");
    }

    /// Shares the main display for 3 s: needs Screen Recording for the
    /// terminal app. `--ignored`.
    #[test]
    #[ignore = "needs Screen Recording for the terminal app"]
    fn the_main_display_is_captured_at_its_frame_rate() {
        // SAFETY: setting a variable before any thread reads it here.
        unsafe { std::env::set_var(DISPLAY_VARIABLE, "main") };
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let (sender, frames) = mpsc::sync_channel(8);
        let asked = Instant::now();
        let request = StartRequest {
            fps: 30,
            max_pixels: 1920 * 1080,
            restore_token: None,
            source_id: None,
        };
        let (capture, picked) = runtime.block_on(start(request, sender)).unwrap();
        let started = asked.elapsed();
        let size = capture.size();
        let mut count = 0;
        let mut sizes = std::collections::HashSet::new();
        let counting = Instant::now();
        while counting.elapsed() < Duration::from_secs(3) {
            if let Ok(frame) = frames.recv_timeout(Duration::from_millis(100)) {
                assert_eq!(frame.format, RawFormat::Nv12);
                sizes.insert((frame.width, frame.height));
                count += 1;
            }
        }
        drop(capture);
        std::thread::sleep(Duration::from_millis(300));
        while frames.try_recv().is_ok() {}
        let closed = matches!(
            frames.recv_timeout(Duration::from_millis(500)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        );

        eprintln!(
            "{picked:?}, size {size:?}, started in {started:?}, {count} frames in 3 s, sizes {sizes:?}"
        );
        assert_eq!(picked.kind, SourceKind::Screen);
        assert!(sizes.contains(&size));
        assert!(
            count >= 1,
            "a still screen gives at least its first picture"
        );
        assert!(closed, "stopping closes the queue");
    }
}

#[cfg(test)]
mod cost {
    use std::sync::mpsc;

    use super::*;
    use crate::video::codec::macos::process_cpu;
    use crate::video::layers::{ScreenShape, screen_layers};
    use crate::video::sender::VideoSender;

    /// A 1080p30 share of the main display for 10 s, captured, scaled and
    /// encoded as the app does; its CPU goes in the log (M6, step 7). Run
    /// in release, with Screen Recording for the terminal app:
    /// `--release --ignored`.
    #[test]
    #[ignore = "needs Screen Recording for the terminal app; measures CPU"]
    fn a_1080p30_share_costs() {
        // SAFETY: setting a variable before any thread reads it here.
        unsafe { std::env::set_var(DISPLAY_VARIABLE, "main") };
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let (sender, frames) = mpsc::sync_channel(4);
        let request = StartRequest {
            fps: 30,
            max_pixels: 1920 * 1080,
            restore_token: None,
            source_id: None,
        };
        let (capture, _) = runtime.block_on(start(request, sender)).unwrap();
        let (width, height) = capture.size();
        let shape = ScreenShape {
            max_pixels: 1920 * 1080,
            fps: 30,
        };
        let sent = Arc::new(Mutex::new(Vec::<(u8, u16, u16)>::new()));
        let counted = Arc::clone(&sent);
        let video = VideoSender::screen(
            "screen".to_owned(),
            screen_layers(width, height, shape),
            shape,
            frames,
            move |frame| {
                counted
                    .lock()
                    .unwrap()
                    .push((frame.layer, frame.width, frame.height));
            },
            |_| {},
        );
        std::thread::sleep(Duration::from_secs(2));
        let (start_at, cpu_start) = (Instant::now(), process_cpu());
        let before = sent.lock().unwrap().len();
        std::thread::sleep(Duration::from_secs(10));
        let (wall, used) = (start_at.elapsed(), process_cpu() - cpu_start);
        let frames: Vec<_> = sent.lock().unwrap()[before..].to_vec();
        drop(video);
        drop(capture);

        let main = frames.iter().filter(|frame| frame.0 == 1).count();
        let size = frames.iter().rev().find(|frame| frame.0 == 1);
        eprintln!(
            "display {width}x{height}: main layer {size:?} at {:.1} fps, \
             {} pictures in all; CPU {:.0} % of one core",
            main as f64 / wall.as_secs_f64(),
            frames.len(),
            used.as_secs_f64() / wall.as_secs_f64() * 100.0
        );
    }
}
