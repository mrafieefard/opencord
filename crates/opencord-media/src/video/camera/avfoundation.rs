//! macOS's cameras through AVFoundation (docs/plans/macos.md M5): the
//! built-in, external and Continuity cameras, NV12 at video range from an
//! `AVCaptureVideoDataOutput` whose delegate runs on a serial queue.
//!
//! The contract, as `linux.rs` keeps it: `SUPPORTED` says whether this
//! system has cameras at all; `cameras()` lists them with ids that stay the
//! same while a camera stays plugged in, and returns `CameraError::Denied`
//! when the user refused access (with where to allow it in the message);
//! `Capture::start` opens the camera with that id, or the first one, at up
//! to 1280×720 and 30 fps, and sends packed frames to `frames` with
//! `try_send`, dropping a frame when the queue is full. The queue closes
//! when the camera goes away; dropping the `Capture` stops it.

use std::ptr::NonNull;
use std::sync::mpsc::{SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use block2::RcBlock;
use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool, NSObjectProtocol, ProtocolObject};
use objc2::{AnyThread, DefinedClass, Message, available, define_class, msg_send};
#[allow(deprecated)]
use objc2_av_foundation::AVCaptureDeviceTypeExternalUnknown;
use objc2_av_foundation::{
    AVAuthorizationStatus, AVCaptureConnection, AVCaptureDevice, AVCaptureDeviceDiscoverySession,
    AVCaptureDeviceFormat, AVCaptureDeviceInput, AVCaptureDevicePosition, AVCaptureDeviceType,
    AVCaptureDeviceTypeBuiltInWideAngleCamera, AVCaptureDeviceTypeContinuityCamera,
    AVCaptureDeviceTypeExternal, AVCaptureDeviceWasDisconnectedNotification, AVCaptureOutput,
    AVCaptureSession, AVCaptureSessionRuntimeErrorNotification, AVCaptureVideoDataOutput,
    AVCaptureVideoDataOutputSampleBufferDelegate, AVMediaTypeVideo,
};
use objc2_core_media::{CMSampleBuffer, CMTime};
use objc2_foundation::{
    NSArray, NSDictionary, NSNotification, NSNotificationCenter, NSNumber, NSObject, NSString,
};

use super::mode::{self, Mode};
use super::{CameraError, CameraInfo, RawFormat, RawFrame};
use crate::video::codec::macos::{ffi, nv12_picture};

pub(super) const SUPPORTED: bool = true;

/// Where the user allows the camera, for the message when it was refused.
const DENIED: &str = "allow Opencord under System Settings → Privacy & Security → Camera";

/// The largest picture asked of a camera (plan §8).
const LARGEST: (u32, u32) = (1280, 720);

fn video() -> &'static NSString {
    // SAFETY: AVFoundation's constant.
    unsafe { AVMediaTypeVideo }.expect("AVFoundation defines AVMediaTypeVideo")
}

/// Asks for camera access if nobody has decided yet; `Denied` if refused.
async fn access() -> Result<(), CameraError> {
    // SAFETY: a class method with AVFoundation's constant.
    let status = unsafe { AVCaptureDevice::authorizationStatusForMediaType(video()) };
    match status {
        AVAuthorizationStatus::Authorized => Ok(()),
        AVAuthorizationStatus::NotDetermined => {
            let (answer, answered) = tokio::sync::oneshot::channel();
            {
                let answer = Mutex::new(Some(answer));
                let block = RcBlock::new(move |granted: Bool| {
                    if let Some(answer) = answer.lock().ok().and_then(|mut answer| answer.take()) {
                        let _ = answer.send(granted.as_bool());
                    }
                });
                // SAFETY: the block may run on any thread; it only sends.
                // AVFoundation keeps its own copy, so ours goes before the
                // wait.
                unsafe {
                    AVCaptureDevice::requestAccessForMediaType_completionHandler(video(), &block);
                }
            }
            match answered.await {
                Ok(true) => Ok(()),
                _ => Err(CameraError::Denied),
            }
        }
        _ => {
            tracing::info!("{DENIED}");
            Err(CameraError::Denied)
        }
    }
}

/// The video cameras there are: built-in, external and (on 14 and later)
/// Continuity cameras.
fn devices() -> Vec<Retained<AVCaptureDevice>> {
    // SAFETY: AVFoundation's constants.
    let types: Vec<&AVCaptureDeviceType> = unsafe {
        if available!(macos = 14.0) {
            vec![
                AVCaptureDeviceTypeBuiltInWideAngleCamera,
                AVCaptureDeviceTypeExternal,
                AVCaptureDeviceTypeContinuityCamera,
            ]
        } else {
            // Deprecated from 14 on, where the branch above runs; 13's name.
            #[allow(deprecated)]
            let external = AVCaptureDeviceTypeExternalUnknown;
            vec![AVCaptureDeviceTypeBuiltInWideAngleCamera, external]
        }
    };
    let types = NSArray::from_slice(&types);
    // SAFETY: a class method with valid arguments.
    let session = unsafe {
        AVCaptureDeviceDiscoverySession::discoverySessionWithDeviceTypes_mediaType_position(
            &types,
            Some(video()),
            AVCaptureDevicePosition::Unspecified,
        )
    };
    // SAFETY: a property read.
    unsafe { session.devices() }.to_vec()
}

pub(super) async fn cameras() -> Result<Vec<CameraInfo>, CameraError> {
    access().await?;
    tokio::task::spawn_blocking(|| {
        devices()
            .iter()
            .map(|device| CameraInfo {
                // SAFETY: property reads.
                id: unsafe { device.uniqueID() }.to_string(),
                name: unsafe { device.localizedName() }.to_string(),
            })
            .collect()
    })
    .await
    .map_err(|error| CameraError::Failed(error.to_string()))
}

/// A format's size and best frame rate, as a mode (NV12: what the output
/// is asked for).
fn mode_of(format: &AVCaptureDeviceFormat) -> Option<Mode> {
    // SAFETY: property reads; the description is the format's.
    let (dimensions, fps) = unsafe {
        let description = format.formatDescription();
        let dimensions = ffi::CMVideoFormatDescriptionGetDimensions(
            Retained::as_ptr(&description).cast_mut().cast(),
        );
        let fps = format
            .videoSupportedFrameRateRanges()
            .to_vec()
            .iter()
            .map(|range| range.maxFrameRate())
            .fold(0.0, f64::max);
        (dimensions, fps)
    };
    Some(Mode {
        format: RawFormat::Nv12,
        width: u32::try_from(dimensions.width).ok()?,
        height: u32::try_from(dimensions.height).ok()?,
        fps: fps.round() as u32,
    })
}

/// What the delegate needs: where frames go (taken when the camera goes
/// away, which closes the queue) and the size they come at.
struct Frames {
    sender: Arc<Mutex<Option<SyncSender<RawFrame>>>>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and the delegate
    // has no `Drop`.
    #[unsafe(super(NSObject))]
    #[name = "OpencordCameraDelegate"]
    #[ivars = Frames]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl AVCaptureVideoDataOutputSampleBufferDelegate for Delegate {
        #[unsafe(method(captureOutput:didOutputSampleBuffer:fromConnection:))]
        fn did_output(
            &self,
            _output: &AVCaptureOutput,
            sample: &CMSampleBuffer,
            _connection: &AVCaptureConnection,
        ) {
            let captured = Instant::now();
            let sample: *const CMSampleBuffer = sample;
            // SAFETY: the sample is valid for the call; its image buffer is
            // too.
            let picture = unsafe {
                let image = ffi::CMSampleBufferGetImageBuffer(sample.cast_mut().cast());
                nv12_picture(image, captured)
            };
            let Some(picture) = picture else {
                return;
            };
            let frame = RawFrame {
                format: RawFormat::Nv12,
                width: picture.width,
                height: picture.height,
                data: picture.data,
                captured,
            };
            let sender = self.ivars().sender.lock().ok();
            if let Some(Err(TrySendError::Disconnected(_))) = sender
                .as_ref()
                .and_then(|sender| sender.as_ref())
                .map(|sender| sender.try_send(frame))
            {
                tracing::debug!("the camera's frames are no longer wanted");
            }
        }
    }
);

impl Delegate {
    fn new(frames: Frames) -> Retained<Self> {
        let this = Self::alloc().set_ivars(frames);
        // SAFETY: NSObject's `init`.
        unsafe { msg_send![super(this), init] }
    }
}

pub(super) struct Capture {
    session: Retained<AVCaptureSession>,
    output: Retained<AVCaptureVideoDataOutput>,
    _delegate: Retained<Delegate>,
    /// The notification observers that close the queue.
    observers: Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
    mode: Mode,
}

// SAFETY: the session, output and observers are only stopped and removed
// from another thread, which AVFoundation and NSNotificationCenter allow;
// frames arrive on the output's own queue.
unsafe impl Send for Capture {}

impl Capture {
    pub(super) async fn start(
        camera: Option<String>,
        frames: SyncSender<RawFrame>,
    ) -> Result<Self, CameraError> {
        access().await?;
        tokio::task::spawn_blocking(move || Self::start_blocking(camera.as_deref(), frames))
            .await
            .map_err(|error| CameraError::Failed(error.to_string()))?
    }

    fn start_blocking(
        camera: Option<&str>,
        frames: SyncSender<RawFrame>,
    ) -> Result<Self, CameraError> {
        let device = match camera {
            Some(id) => {
                // SAFETY: a class method with a valid string.
                unsafe { AVCaptureDevice::deviceWithUniqueID(&NSString::from_str(id)) }
            }
            None => devices().into_iter().next(),
        }
        .ok_or(CameraError::NoCamera)?;
        // SAFETY: a property read.
        let formats = unsafe { device.formats() }.to_vec();
        let modes: Vec<(Mode, &Retained<AVCaptureDeviceFormat>)> = formats
            .iter()
            .filter_map(|format| mode_of(format).map(|mode| (mode, format)))
            .collect();
        let chosen = mode::choose(&modes.iter().map(|(mode, _)| *mode).collect::<Vec<_>>())
            .ok_or(CameraError::NoUsableMode)?;
        let format = modes
            .iter()
            .find(|(mode, _)| *mode == chosen)
            .map(|(_, format)| *format)
            .ok_or(CameraError::NoUsableMode)?;
        // Larger than 1280×720: the output scales it down, keeping its shape.
        let scale = (f64::from(LARGEST.0) / f64::from(chosen.width))
            .min(f64::from(LARGEST.1) / f64::from(chosen.height))
            .min(1.0);
        let even = |value: f64| ((value.round() as u32) & !1).max(2);
        let mode = Mode {
            width: even(f64::from(chosen.width) * scale),
            height: even(f64::from(chosen.height) * scale),
            fps: chosen.fps.min(30),
            ..chosen
        };

        // The frame duration to ask for: 1/30 s where the format's rates
        // allow it, else the shortest it has (exactly, as AVFoundation
        // throws on any duration outside its ranges).
        // SAFETY: property reads.
        let interval = unsafe {
            let target = f64::from(mode.fps);
            let ranges = format.videoSupportedFrameRateRanges().to_vec();
            let within = ranges
                .iter()
                .find(|range| range.minFrameRate() <= target && target < range.maxFrameRate());
            match within {
                Some(_) => Some(CMTime {
                    value: 1,
                    timescale: mode.fps.max(1) as i32,
                    flags: objc2_core_media::CMTimeFlags::Valid,
                    epoch: 0,
                }),
                None => ranges
                    .iter()
                    .max_by(|a, b| a.maxFrameRate().total_cmp(&b.maxFrameRate()))
                    .map(|range| range.minFrameDuration()),
            }
        };
        // An Objective-C exception (AVFoundation throws on what it refuses)
        // becomes an error rather than ending the process.
        let started = objc2::exception::catch(std::panic::AssertUnwindSafe(|| {
            // SAFETY: AVFoundation calls with valid arguments, on this
            // thread.
            unsafe { Self::configure(&device, format, mode, chosen, interval, frames) }
        }));
        match started {
            Ok(started) => started,
            Err(exception) => Err(CameraError::Failed(format!(
                "AVFoundation refused: {exception:?}"
            ))),
        }
    }

    /// Sets the session up and starts it.
    ///
    /// # Safety
    ///
    /// AVFoundation's rules: `format` is one of `device`'s formats.
    unsafe fn configure(
        device: &AVCaptureDevice,
        format: &AVCaptureDeviceFormat,
        mode: Mode,
        chosen: Mode,
        interval: Option<CMTime>,
        frames: SyncSender<RawFrame>,
    ) -> Result<Self, CameraError> {
        let failed = |what: &str, error: Option<Retained<objc2_foundation::NSError>>| {
            let detail = error.map(|error| error.localizedDescription().to_string());
            CameraError::Failed(format!(
                "{what}{}",
                detail.map(|d| format!(": {d}")).unwrap_or_default()
            ))
        };
        // SAFETY: the caller's promise.
        unsafe {
            let input = AVCaptureDeviceInput::deviceInputWithDevice_error(device)
                .map_err(|error| failed("the camera did not open", Some(error)))?;
            let session = AVCaptureSession::new();
            session.beginConfiguration();
            if !session.canAddInput(&input) {
                return Err(failed("the camera cannot be used", None));
            }
            session.addInput(&input);

            let output = AVCaptureVideoDataOutput::new();
            let nv12 = NSNumber::new_u32(ffi::NV12_VIDEO_RANGE);
            let (width, height) = (
                NSNumber::new_u32(mode.width),
                NSNumber::new_u32(mode.height),
            );
            let settings = NSDictionary::<NSString, AnyObject>::from_slices(
                &[
                    cf_key(ffi::kCVPixelBufferPixelFormatTypeKey),
                    cf_key(ffi::kCVPixelBufferWidthKey),
                    cf_key(ffi::kCVPixelBufferHeightKey),
                ],
                &[nv12.as_ref(), width.as_ref(), height.as_ref()],
            );
            output.setVideoSettings(Some(&settings));
            output.setAlwaysDiscardsLateVideoFrames(true);
            let sender = Arc::new(Mutex::new(Some(frames)));
            let delegate = Delegate::new(Frames {
                sender: Arc::clone(&sender),
            });
            let queue = DispatchQueue::new("opencord-camera", None);
            output.setSampleBufferDelegate_queue(
                Some(ProtocolObject::from_ref(&*delegate)),
                Some(&queue),
            );
            if !session.canAddOutput(&output) {
                return Err(failed("the camera's pictures cannot be read", None));
            }
            session.addOutput(&output);

            // The format and rate hold while the device stays locked into
            // the running session.
            device
                .lockForConfiguration()
                .map_err(|error| failed("the camera cannot be set up", Some(error)))?;
            device.setActiveFormat(format);
            if let Some(interval) = interval {
                device.setActiveVideoMinFrameDuration(interval);
                device.setActiveVideoMaxFrameDuration(interval);
            }
            session.commitConfiguration();
            session.startRunning();
            device.unlockForConfiguration();
            if !session.isRunning() {
                return Err(failed("the camera did not start", None));
            }

            // The camera going away, or the session failing, closes the
            // queue.
            let center = NSNotificationCenter::defaultCenter();
            let mut observers = Vec::new();
            for (name, object) in [
                (
                    AVCaptureDeviceWasDisconnectedNotification,
                    Retained::cast_unchecked::<AnyObject>(device.retain()),
                ),
                (
                    AVCaptureSessionRuntimeErrorNotification,
                    Retained::cast_unchecked::<AnyObject>(session.clone()),
                ),
            ] {
                let sender = Arc::clone(&sender);
                let block = RcBlock::new(move |_: NonNull<NSNotification>| {
                    tracing::info!("the camera went away");
                    if let Ok(mut sender) = sender.lock() {
                        sender.take();
                    }
                });
                observers.push(center.addObserverForName_object_queue_usingBlock(
                    Some(name),
                    Some(&object),
                    None,
                    &block,
                ));
            }
            tracing::debug!(?mode, ?chosen, "camera started");
            Ok(Self {
                session,
                output,
                _delegate: delegate,
                observers,
                mode,
            })
        }
    }

    pub(super) fn mode(&self) -> Mode {
        self.mode
    }
}

/// A Core Foundation string key as the `NSString` it is toll-free.
fn cf_key(key: &'static objc2_core_foundation::CFString) -> &'static NSString {
    let key: *const objc2_core_foundation::CFString = key;
    // SAFETY: CFString and NSString are toll-free bridged.
    unsafe { &*key.cast::<NSString>() }
}

impl Drop for Capture {
    fn drop(&mut self) {
        // SAFETY: AVFoundation and NSNotificationCenter calls on objects
        // this capture holds.
        unsafe {
            let center = NSNotificationCenter::defaultCenter();
            for observer in self.observers.drain(..) {
                let observer: &AnyObject = (*observer).as_ref();
                center.removeObserver(observer);
            }
            self.session.stopRunning();
            self.output.setSampleBufferDelegate_queue(None, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;

    /// Needs a camera and camera access for the terminal app, so it runs
    /// with the user present: `--ignored`.
    #[test]
    #[ignore = "needs a camera and camera access"]
    fn a_real_camera_starts_delivers_and_stops() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let listed = runtime.block_on(cameras()).unwrap();
        eprintln!("cameras: {listed:?}");
        assert!(!listed.is_empty(), "a camera is listed");

        let (sender, frames) = mpsc::sync_channel(4);
        let asked = Instant::now();
        let capture = runtime.block_on(Capture::start(None, sender)).unwrap();
        let started = asked.elapsed();
        let first = frames.recv_timeout(Duration::from_secs(1)).unwrap();
        let first_after = asked.elapsed();
        let mode = capture.mode();
        let mut count = 0;
        let counting = Instant::now();
        while counting.elapsed() < Duration::from_secs(2) {
            if frames.recv_timeout(Duration::from_millis(200)).is_ok() {
                count += 1;
            }
        }
        let stopping = Instant::now();
        drop(capture);
        let stopped = stopping.elapsed();
        std::thread::sleep(Duration::from_millis(300));
        while frames.try_recv().is_ok() {}
        std::thread::sleep(Duration::from_millis(500));
        let after = frames.try_iter().count();

        eprintln!(
            "mode {mode:?}; started in {started:?}, first frame after {first_after:?}, \
             {count} frames in 2 s, stopped in {stopped:?}"
        );
        assert_eq!(first.format, RawFormat::Nv12);
        assert_eq!((first.width, first.height), (mode.width, mode.height));
        // Plan §8 wants under 1 s; this Mac's FaceTime HD camera alone
        // takes about 0.65 s after the session starts (§7, M5).
        assert!(first_after < Duration::from_millis(1500));
        assert!(count >= 2 * mode.fps as usize * 3 / 4, "{count} frames");
        assert_eq!(after, 0, "frames stop after stopping");
    }
}

#[cfg(test)]
mod cost {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;
    use crate::video::codec::macos::scale::Scaler;
    use crate::video::layers::camera_layers;
    use crate::video::sender::VideoSender;

    /// This process's CPU time (user and system).
    fn cpu() -> Duration {
        let mut usage = std::mem::MaybeUninit::<libc_rusage>::zeroed();
        // SAFETY: getrusage fills the struct.
        unsafe { getrusage(0, usage.as_mut_ptr()) };
        // SAFETY: filled above.
        let usage = unsafe { usage.assume_init() };
        let time =
            |t: [i64; 2]| Duration::from_secs(t[0] as u64) + Duration::from_micros(t[1] as u64);
        time(usage.user) + time(usage.system)
    }

    #[repr(C)]
    struct libc_rusage {
        user: [i64; 2],
        system: [i64; 2],
        rest: [i64; 14],
    }

    unsafe extern "C" {
        fn getrusage(who: i32, usage: *mut libc_rusage) -> i32;
    }

    /// The camera's whole sending side for 10 s, as the app runs it: the
    /// camera, three layers scaled and encoded, and a 640×360 preview
    /// turned into RGBA. Its CPU goes in the log (M5, step 4); run in
    /// release, with a camera: `--release --ignored`.
    #[test]
    #[ignore = "needs a camera and camera access; measures CPU"]
    fn the_cameras_sending_side_costs() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let (sender, frames) = mpsc::sync_channel(4);
        let capture = runtime.block_on(Capture::start(None, sender)).unwrap();
        let mode = capture.mode();
        let sent = Arc::new(Mutex::new([0u64; 3]));
        let counted = Arc::clone(&sent);
        let mut scaler = Scaler::new();
        let previews = Arc::new(Mutex::new(0u64));
        let previewed = Arc::clone(&previews);
        let video = VideoSender::camera(
            "camera".to_owned(),
            camera_layers(mode.width, mode.height),
            frames,
            move |frame| {
                if let Some(count) = counted.lock().unwrap().get_mut(usize::from(frame.layer)) {
                    *count += 1;
                }
            },
            move |picture| {
                let _ = scaler.rgba(picture, 640, 360);
                *previewed.lock().unwrap() += 1;
            },
        );
        video.encode(
            &["l".to_owned(), "m".to_owned(), "h".to_owned()],
            &[150_000, 500_000, 1_500_000],
            1.0,
            1.0,
        );
        std::thread::sleep(Duration::from_secs(2));
        let (start, cpu_start) = (Instant::now(), cpu());
        let before = *sent.lock().unwrap();
        std::thread::sleep(Duration::from_secs(10));
        let (wall, used) = (start.elapsed(), cpu() - cpu_start);
        let after = *sent.lock().unwrap();
        drop(video);
        drop(capture);

        let rates: Vec<f64> = (0..3)
            .map(|layer| (after[layer] - before[layer]) as f64 / wall.as_secs_f64())
            .collect();
        eprintln!(
            "camera {mode:?}: layers at {:.1}/{:.1}/{:.1} fps, {} previews; \
             CPU {:.0} % of one core",
            rates[0],
            rates[1],
            rates[2],
            previews.lock().unwrap(),
            used.as_secs_f64() / wall.as_secs_f64() * 100.0
        );
    }
}
