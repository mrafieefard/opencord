//! Windows' cameras (docs/plans/windows.md W5): Media Foundation's source
//! reader, read synchronously on a thread of its own.
//!
//! The contract, as `linux.rs` keeps it: `SUPPORTED` says whether this
//! system has cameras at all; `cameras()` lists them with ids that stay the
//! same while a camera stays plugged in, and returns `CameraError::Denied`
//! when the user refused access (with where to allow it in the message);
//! `Capture::start` opens the camera with that id, or the first one, at up
//! to 1280×720 and 30 fps, and sends packed frames ([`super::pack`]) to
//! `frames` with `try_send`, dropping a frame when the queue is full. The
//! queue closes when the camera goes away; dropping the `Capture` stops it.
//! MJPEG frames go to the shared `RawDecoder`, FFmpeg's from W3.
//!
//! The id is the device's symbolic link, which Windows keeps the same for
//! as long as the camera stays plugged into the same port.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::thread::JoinHandle;
use std::time::Instant;

use windows::Win32::Foundation::E_ACCESSDENIED;
use windows::Win32::Media::MediaFoundation::{
    IMF2DBuffer, IMFActivate, IMFMediaSource, IMFMediaType, IMFSample, IMFSourceReader,
    MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
    MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
    MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE,
    MF_MT_SUBTYPE, MF_READWRITE_DISABLE_CONVERTERS, MF_SOURCE_READER_FIRST_VIDEO_STREAM,
    MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READERF_ERROR, MFCreateAttributes,
    MFCreateSourceReaderFromMediaSource, MFEnumDeviceSources, MFSTARTUP_NOSOCKET, MFShutdown,
    MFStartup, MFVideoFormat_I420, MFVideoFormat_MJPG, MFVideoFormat_NV12, MFVideoFormat_YUY2,
};
use windows::Win32::System::Com::{
    COINIT_MULTITHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize,
};
use windows::core::{GUID, Interface, PWSTR};

use super::mode::{self, Mode};
use super::{CameraError, CameraInfo, RawFormat, RawFrame, pack};

pub(super) const SUPPORTED: bool = true;

/// Camera ids from Media Foundation start with this, then the device's
/// symbolic link.
const ID_PREFIX: &str = "mf:";

/// What to tell the user when Windows refuses the camera.
const DENIED: &str = "Allow camera access for desktop apps in \
                      Settings → Privacy & security → Camera";

/// The video stream, whichever index it is on.
const VIDEO_STREAM: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

fn failed(error: windows::core::Error) -> CameraError {
    // A refusal is its own thing, and the message says where to fix it.
    if error.code() == E_ACCESSDENIED {
        tracing::warn!("the camera was refused: {DENIED}");
        return CameraError::Denied;
    }
    CameraError::Failed(format!("{error} ({:#010x})", error.code().0))
}

/// COM and Media Foundation, started for as long as this is kept. Both are
/// per thread, so every thread that touches a camera makes one.
struct Started {
    /// Whether this one initialised COM, and so should undo it. A thread
    /// already in an apartment is left as it was.
    owns_com: bool,
}

impl Started {
    fn new() -> Result<Self, CameraError> {
        // Multithreaded, on our own threads and never Flutter's UI thread
        // (windows.md §3, decision A).
        // SAFETY: undone in `Drop`, on this same thread.
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if result.is_err() {
            return Err(CameraError::Failed(format!(
                "COM would not start ({:#010x})",
                result.0
            )));
        }
        // S_FALSE means the thread was already in this apartment, and then
        // undoing it is someone else's to do.
        let owns_com = result.is_ok();
        // SAFETY: matched by `MFShutdown` in `Drop`.
        if let Err(error) = unsafe { MFStartup(mf_version(), MFSTARTUP_NOSOCKET) } {
            if owns_com {
                // SAFETY: this thread initialised COM just above.
                unsafe { CoUninitialize() };
            }
            return Err(failed(error));
        }
        Ok(Self { owns_com })
    }
}

impl Drop for Started {
    fn drop(&mut self) {
        // SAFETY: both were started on this thread in `new`.
        unsafe {
            let _ = MFShutdown();
            if self.owns_com {
                CoUninitialize();
            }
        }
    }
}

/// `MF_VERSION`: the SDK version in the high half, the API version in the
/// low one. The crate does not expose the macro that builds it.
const fn mf_version() -> u32 {
    const MF_SDK_VERSION: u32 = 0x0002;
    const MF_API_VERSION: u32 = 0x0070;
    (MF_SDK_VERSION << 16) | MF_API_VERSION
}

/// The video capture devices Media Foundation knows about.
fn device_sources() -> Result<Vec<IMFActivate>, CameraError> {
    // SAFETY: the attributes live until the call returns; the array
    // `MFEnumDeviceSources` allocates is freed here whatever happens, once
    // its elements have been cloned into Rust's own reference counting.
    unsafe {
        let mut attributes = None;
        MFCreateAttributes(&mut attributes, 1).map_err(failed)?;
        let attributes =
            attributes.ok_or_else(|| CameraError::Failed("no attributes".to_owned()))?;
        attributes
            .SetGUID(
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
            )
            .map_err(failed)?;
        let mut found = std::ptr::null_mut();
        let mut count = 0;
        MFEnumDeviceSources(&attributes, &mut found, &mut count).map_err(failed)?;
        let devices = if found.is_null() {
            Vec::new()
        } else {
            std::slice::from_raw_parts(found, count as usize)
                .iter()
                .filter_map(Clone::clone)
                .collect()
        };
        if !found.is_null() {
            CoTaskMemFree(Some(found.cast()));
        }
        Ok(devices)
    }
}

/// One of a device's string attributes.
fn string_of(device: &IMFActivate, key: &GUID) -> Option<String> {
    // SAFETY: `GetAllocatedString` fills in a pointer Media Foundation
    // allocated, which is freed here once it has been copied into a
    // `String`.
    unsafe {
        let mut text = PWSTR::null();
        let mut length = 0;
        device
            .GetAllocatedString(key, &mut text, &mut length)
            .ok()?;
        if text.is_null() {
            return None;
        }
        let value = text.to_string().ok();
        CoTaskMemFree(Some(text.as_ptr().cast()));
        value
    }
}

fn info(device: &IMFActivate) -> Option<CameraInfo> {
    let link = string_of(
        device,
        &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
    )?;
    Some(CameraInfo {
        id: format!("{ID_PREFIX}{link}"),
        name: string_of(device, &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME)
            .unwrap_or_else(|| "Camera".to_owned()),
    })
}

/// Looked up rather than matched: Media Foundation's subtype constants are
/// not upper case, and a `match` on them reads as a pattern binding.
fn raw_format(subtype: &GUID) -> Option<RawFormat> {
    [
        (MFVideoFormat_NV12, RawFormat::Nv12),
        (MFVideoFormat_YUY2, RawFormat::Yuyv),
        (MFVideoFormat_I420, RawFormat::I420),
        (MFVideoFormat_MJPG, RawFormat::Mjpeg),
    ]
    .into_iter()
    .find(|(known, _)| known == subtype)
    .map(|(_, format)| format)
}

/// A `MF_MT_FRAME_SIZE` or `MF_MT_FRAME_RATE`: two 32-bit halves in one
/// 64-bit attribute, the first in the high half.
fn pair(media_type: &IMFMediaType, key: &GUID) -> Option<(u32, u32)> {
    // SAFETY: reading one attribute of a media type we hold.
    let packed = unsafe { media_type.GetUINT64(key) }.ok()?;
    Some(((packed >> 32) as u32, packed as u32))
}

/// What one of the camera's native types offers.
fn mode_of(media_type: &IMFMediaType) -> Option<Mode> {
    // SAFETY: reading one attribute of a media type we hold.
    let subtype = unsafe { media_type.GetGUID(&MF_MT_SUBTYPE) }.ok()?;
    let (width, height) = pair(media_type, &MF_MT_FRAME_SIZE)?;
    // The rate is a ratio, so a camera may well say 30000/1001.
    let (numerator, denominator) = pair(media_type, &MF_MT_FRAME_RATE)?;
    Some(Mode {
        format: raw_format(&subtype)?,
        width,
        height,
        fps: (denominator > 0).then(|| (numerator + denominator / 2) / denominator)?,
    })
}

/// Every native type the reader offers that Opencord can use, with its
/// index, so the chosen one can be asked for again.
fn modes(reader: &IMFSourceReader) -> Vec<(u32, Mode)> {
    let mut modes = Vec::new();
    for index in 0.. {
        // SAFETY: asking for types until Media Foundation says there are no
        // more, which it does with an error.
        let Ok(media_type) = (unsafe { reader.GetNativeMediaType(VIDEO_STREAM, index) }) else {
            break;
        };
        if let Some(mode) = mode_of(&media_type) {
            modes.push((index, mode));
        }
    }
    modes
}

pub(super) async fn cameras() -> Result<Vec<CameraInfo>, CameraError> {
    // Enumeration talks to the driver stack, so it does not belong on a
    // runtime thread.
    tokio::task::spawn_blocking(|| {
        let _started = Started::new()?;
        Ok(device_sources()?.iter().filter_map(info).collect())
    })
    .await
    .unwrap_or_else(|error| Err(CameraError::Failed(error.to_string())))
}

/// A running capture; stops when dropped.
pub(super) struct Capture {
    mode: Mode,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Capture {
    pub(super) async fn start(
        camera: Option<String>,
        frames: SyncSender<RawFrame>,
    ) -> Result<Self, CameraError> {
        let stop = Arc::new(AtomicBool::new(false));
        let (started, outcome) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("opencord-camera".to_owned())
            .spawn({
                let stop = Arc::clone(&stop);
                move || run(camera.as_deref(), &frames, &stop, &started)
            })
            .map_err(|error| CameraError::Failed(error.to_string()))?;
        // The reader is made on the thread that reads it, so whether it
        // opened — and at what — has to come back from there.
        match outcome.recv() {
            Ok(Ok(mode)) => Ok(Self {
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

    pub(super) fn mode(&self) -> Mode {
        self.mode
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

/// Opens the camera, says what it opened, then reads it until told to stop
/// or the camera goes away.
fn run(
    camera: Option<&str>,
    frames: &SyncSender<RawFrame>,
    stop: &AtomicBool,
    started: &mpsc::Sender<Result<Mode, CameraError>>,
) {
    let _mf = match Started::new() {
        Ok(ready) => ready,
        Err(error) => {
            let _ = started.send(Err(error));
            return;
        }
    };
    let (reader, mode) = match open(camera) {
        Ok(opened) => opened,
        Err(error) => {
            let _ = started.send(Err(error));
            return;
        }
    };
    if started.send(Ok(mode)).is_err() {
        return;
    }
    read(&reader, mode, frames, stop);
}

fn open(camera: Option<&str>) -> Result<(IMFSourceReader, Mode), CameraError> {
    let devices = device_sources()?;
    let device = match camera {
        Some(id) => {
            let link = id.strip_prefix(ID_PREFIX).ok_or(CameraError::NoCamera)?;
            devices
                .iter()
                .find(|device| {
                    string_of(
                        device,
                        &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
                    )
                    .is_some_and(|found| found.eq_ignore_ascii_case(link))
                })
                .ok_or(CameraError::NoCamera)?
        }
        None => devices.first().ok_or(CameraError::NoCamera)?,
    };

    // SAFETY: the device came from `MFEnumDeviceSources` and is still held;
    // the reader takes its own reference to the source.
    let reader = unsafe {
        let source: IMFMediaSource = device.ActivateObject().map_err(failed)?;
        // No converters: the source reader would otherwise quietly insert a
        // decoder or a colour converter, and the engine does that itself,
        // into the picture it actually wants.
        let mut attributes = None;
        MFCreateAttributes(&mut attributes, 1).map_err(failed)?;
        let attributes =
            attributes.ok_or_else(|| CameraError::Failed("no attributes".to_owned()))?;
        attributes
            .SetUINT32(&MF_READWRITE_DISABLE_CONVERTERS, 1)
            .map_err(failed)?;
        MFCreateSourceReaderFromMediaSource(&source, &attributes).map_err(failed)?
    };

    let offered = modes(&reader);
    let wanted = mode::choose(&offered.iter().map(|(_, mode)| *mode).collect::<Vec<_>>())
        .ok_or(CameraError::NoUsableMode)?;
    let (index, _) = offered
        .iter()
        .find(|(_, mode)| *mode == wanted)
        .ok_or(CameraError::NoUsableMode)?;

    // SAFETY: the index came from this same reader's list of native types.
    unsafe {
        let media_type = reader
            .GetNativeMediaType(VIDEO_STREAM, *index)
            .map_err(failed)?;
        reader
            .SetCurrentMediaType(VIDEO_STREAM, None, &media_type)
            .map_err(failed)?;
        reader
            .SetStreamSelection(VIDEO_STREAM, true)
            .map_err(failed)?;
    }
    tracing::info!(?wanted, "the camera opened");
    Ok((reader, wanted))
}

fn read(reader: &IMFSourceReader, mode: Mode, frames: &SyncSender<RawFrame>, stop: &AtomicBool) {
    while !stop.load(Ordering::Relaxed) {
        let mut flags = 0u32;
        let mut sample = None;
        // SAFETY: a synchronous read of the stream this reader selected;
        // `sample` is only used where the call filled it in.
        let result = unsafe {
            reader.ReadSample(
                VIDEO_STREAM,
                0,
                None,
                Some(&mut flags),
                None,
                Some(&mut sample),
            )
        };
        if let Err(error) = result {
            tracing::warn!(%error, "the camera stopped");
            return;
        }
        // Unplugged, or the stream ended. Returning drops the sender, which
        // closes the queue — how the engine hears about it.
        if flags & (MF_SOURCE_READERF_ERROR.0 as u32 | MF_SOURCE_READERF_ENDOFSTREAM.0 as u32) != 0
        {
            tracing::info!(flags, "the camera went away");
            return;
        }
        // No sample and no flag is a gap the reader may report; ask again.
        let Some(sample) = sample else { continue };
        if let Some(frame) = frame_of(&sample, mode) {
            // Dropped while the encoder is behind.
            let _ = frames.try_send(frame);
        }
    }
}

/// One sample's bytes, packed. `IMF2DBuffer` gives the real stride, which a
/// plain lock does not: a camera's rows are usually padded.
fn frame_of(sample: &IMFSample, mode: Mode) -> Option<RawFrame> {
    // SAFETY: the buffer stays locked for exactly as long as its bytes are
    // read, and is unlocked on every path out; the lengths come from the
    // lock itself.
    unsafe {
        let buffer = sample.ConvertToContiguousBuffer().ok()?;
        let captured = Instant::now();
        if let Ok(two_d) = buffer.cast::<IMF2DBuffer>() {
            let mut scanline = std::ptr::null_mut();
            let mut stride = 0;
            two_d.Lock2D(&mut scanline, &mut stride).ok()?;
            let rows = match mode.format {
                // Only the planar formats have chroma rows after the luma.
                RawFormat::Nv12 | RawFormat::I420 => mode.height as usize * 3 / 2,
                _ => mode.height as usize,
            };
            let length = stride.unsigned_abs() as usize * rows;
            let frame = pack(
                mode,
                std::slice::from_raw_parts(scanline, length),
                stride,
                captured,
            );
            let _ = two_d.Unlock2D();
            return frame;
        }
        let mut data = std::ptr::null_mut();
        let mut length = 0;
        buffer.Lock(&mut data, None, Some(&mut length)).ok()?;
        let frame = pack(
            mode,
            std::slice::from_raw_parts(data, length as usize),
            0,
            captured,
        );
        let _ = buffer.Unlock();
        frame
    }
}
