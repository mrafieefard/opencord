//! Cameras straight from V4L2 (plan §8): the fallback where the Camera
//! portal is missing or PipeWire shows no cameras. Never used when the
//! portal said no. Capture runs on a thread of its own and polls with a
//! timeout, so it notices when to stop.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use v4l::buffer::Type;
use v4l::capability::Flags;
use v4l::frameinterval::FrameIntervalEnum;
use v4l::framesize::FrameSizeEnum;
use v4l::io::mmap::Stream;
use v4l::io::traits::CaptureStream;
use v4l::video::Capture as _;
use v4l::video::capture::Parameters;
use v4l::{Device, Format, FourCC};

use super::mode::{self, Mode};
use super::{CameraError, CameraInfo, RawFormat, RawFrame, pack};

/// Camera ids from V4L2 start with this, then the device's path.
const ID_PREFIX: &str = "v4l2:";
/// How long a wait for a frame may take before the thread looks whether
/// it should stop.
const POLL: Duration = Duration::from_millis(200);
/// Buffers the driver fills in turn.
const BUFFERS: u32 = 4;

/// Capture devices that offer something Opencord can use.
pub fn cameras() -> Vec<CameraInfo> {
    devices()
        .into_iter()
        .filter_map(|path| {
            let device = Device::with_path(&path).ok()?;
            let caps = device.query_caps().ok()?;
            if !caps.capabilities.contains(Flags::VIDEO_CAPTURE) || modes(&device).is_empty() {
                return None;
            }
            Some(CameraInfo {
                id: format!("{ID_PREFIX}{}", stable_path(&path).display()),
                name: caps.card,
            })
        })
        .collect()
}

/// `/dev/video*`, in order.
fn devices() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir("/dev")
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("video"))
                })
                .collect()
        })
        .unwrap_or_default();
    paths.sort();
    paths
}

/// The device's name under `/dev/v4l/by-id`, which stays the same when it
/// is plugged in elsewhere; the path itself without one.
fn stable_path(path: &Path) -> PathBuf {
    let target = path.canonicalize().ok();
    std::fs::read_dir("/dev/v4l/by-id")
        .ok()
        .and_then(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .find(|link| link.canonicalize().ok() == target)
        })
        .unwrap_or_else(|| path.to_owned())
}

fn raw_format(fourcc: FourCC) -> Option<RawFormat> {
    match &fourcc.repr {
        b"MJPG" => Some(RawFormat::Mjpeg),
        b"YUYV" => Some(RawFormat::Yuyv),
        b"NV12" => Some(RawFormat::Nv12),
        b"YU12" => Some(RawFormat::I420),
        _ => None,
    }
}

fn fourcc(format: RawFormat) -> FourCC {
    FourCC::new(match format {
        RawFormat::Mjpeg => b"MJPG",
        RawFormat::Yuyv => b"YUYV",
        RawFormat::Nv12 => b"NV12",
        RawFormat::I420 => b"YU12",
    })
}

/// Every format, size and frame rate the device offers that Opencord can
/// use.
fn modes(device: &Device) -> Vec<Mode> {
    let mut modes = Vec::new();
    for description in device.enum_formats().unwrap_or_default() {
        let Some(format) = raw_format(description.fourcc) else {
            continue;
        };
        for size in device
            .enum_framesizes(description.fourcc)
            .unwrap_or_default()
        {
            let sizes: Vec<(u32, u32)> = match size.size {
                FrameSizeEnum::Discrete(size) => vec![(size.width, size.height)],
                FrameSizeEnum::Stepwise(range) => {
                    let ideal = (1280, 720);
                    let fits = (range.min_width..=range.max_width).contains(&ideal.0)
                        && (range.min_height..=range.max_height).contains(&ideal.1);
                    let mut sizes = vec![(range.max_width, range.max_height)];
                    if fits {
                        sizes.push(ideal);
                    }
                    sizes
                }
            };
            for (width, height) in sizes {
                let intervals = device
                    .enum_frameintervals(description.fourcc, width, height)
                    .unwrap_or_default();
                for fps in intervals
                    .into_iter()
                    .flat_map(|interval| rates(interval.interval))
                {
                    let mode = Mode {
                        format,
                        width,
                        height,
                        fps,
                    };
                    if !modes.contains(&mode) {
                        modes.push(mode);
                    }
                }
            }
        }
    }
    modes
}

/// Frames a second for a frame interval: discrete ones as they are, a
/// range by its fastest and 30 when it allows it.
fn rates(interval: FrameIntervalEnum) -> Vec<u32> {
    let fps = |numerator: u32, denominator: u32| {
        (numerator > 0).then(|| (denominator + numerator / 2) / numerator)
    };
    match interval {
        FrameIntervalEnum::Discrete(fraction) => fps(fraction.numerator, fraction.denominator)
            .into_iter()
            .collect(),
        FrameIntervalEnum::Stepwise(range) => {
            let fastest = fps(range.min.numerator, range.min.denominator);
            let slowest = fps(range.max.numerator, range.max.denominator);
            let mut rates: Vec<u32> = fastest.into_iter().collect();
            if let (Some(fastest), Some(slowest)) = (fastest, slowest)
                && (slowest..=fastest).contains(&30)
            {
                rates.push(30);
            }
            rates
        }
    }
}

/// A running capture; stops when dropped.
pub struct Capture {
    pub mode: Mode,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Capture {
    /// Starts the camera with this id (or the first one there is), sending
    /// its frames to `frames`. Returns once frames can flow.
    pub fn start(camera: Option<&str>, frames: SyncSender<RawFrame>) -> Result<Self, CameraError> {
        let path = match camera {
            Some(id) => PathBuf::from(id.strip_prefix(ID_PREFIX).ok_or(CameraError::NoCamera)?),
            None => {
                let first = cameras().into_iter().next().ok_or(CameraError::NoCamera)?;
                PathBuf::from(first.id.trim_start_matches(ID_PREFIX))
            }
        };
        let device = Device::with_path(&path).map_err(|_| CameraError::NoCamera)?;
        let mode = mode::choose(&modes(&device)).ok_or(CameraError::NoUsableMode)?;
        let failed = |error: std::io::Error| CameraError::Failed(error.to_string());
        let format = device
            .set_format(&Format::new(mode.width, mode.height, fourcc(mode.format)))
            .map_err(failed)?;
        if (format.width, format.height, raw_format(format.fourcc))
            != (mode.width, mode.height, Some(mode.format))
        {
            return Err(CameraError::NoUsableMode);
        }
        device
            .set_params(&Parameters::with_fps(mode.fps))
            .map_err(failed)?;
        let stride = i32::try_from(format.stride).unwrap_or(0);
        let stop = Arc::new(AtomicBool::new(false));
        let (started, outcome) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("opencord-camera".to_owned())
            .spawn({
                let stop = Arc::clone(&stop);
                move || run(&device, mode, stride, &frames, &stop, &started)
            })
            .map_err(failed)?;
        match outcome.recv() {
            Ok(Ok(())) => Ok(Self {
                mode,
                stop,
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
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(
    device: &Device,
    mode: Mode,
    stride: i32,
    frames: &SyncSender<RawFrame>,
    stop: &AtomicBool,
    started: &mpsc::Sender<Result<(), CameraError>>,
) {
    let mut stream = match Stream::with_buffers(device, Type::VideoCapture, BUFFERS) {
        Ok(stream) => stream,
        Err(error) => {
            let _ = started.send(Err(CameraError::Failed(error.to_string())));
            return;
        }
    };
    stream.set_timeout(POLL);
    let _ = started.send(Ok(()));
    while !stop.load(Ordering::Relaxed) {
        match stream.next() {
            Ok((data, metadata)) => {
                let used = (metadata.bytesused as usize).min(data.len());
                if let Some(frame) = pack(mode, &data[..used], stride, Instant::now()) {
                    // Dropped while the encoder is behind.
                    let _ = frames.try_send(frame);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {}
            Err(error) => {
                tracing::warn!(%error, "the camera stopped");
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use v4l::fraction::Fraction;
    use v4l::frameinterval::Stepwise;

    use super::*;

    #[test]
    fn the_formats_opencord_uses_have_fourccs_both_ways() {
        for format in [
            RawFormat::Mjpeg,
            RawFormat::Yuyv,
            RawFormat::Nv12,
            RawFormat::I420,
        ] {
            assert_eq!(raw_format(fourcc(format)), Some(format));
        }
        assert_eq!(raw_format(FourCC::new(b"H264")), None);
    }

    #[test]
    fn frame_intervals_become_rates() {
        assert_eq!(
            rates(FrameIntervalEnum::Discrete(Fraction::new(1, 30))),
            vec![30]
        );
        assert_eq!(
            rates(FrameIntervalEnum::Discrete(Fraction::new(1001, 30_000))),
            vec![30]
        );
        let range = FrameIntervalEnum::Stepwise(Stepwise {
            min: Fraction::new(1, 60),
            max: Fraction::new(1, 5),
            step: Fraction::new(1, 1),
        });
        assert_eq!(rates(range), vec![60, 30]);
    }

    #[test]
    fn listing_cameras_never_fails() {
        // Whatever this machine has; opening devices read-only to list them
        // does not start them.
        let cameras = cameras();
        assert!(
            cameras
                .iter()
                .all(|camera| camera.id.starts_with(ID_PREFIX))
        );
    }
}
