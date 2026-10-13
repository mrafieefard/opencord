//! macOS's screen sound (docs/plans/macos.md M7): Core Audio process taps,
//! macOS 14.2 and later. A tap of everything but Opencord, or of one app's
//! processes, runs in a private aggregate device whose IOProc hands the
//! sound on as 48 kHz stereo. The tap is unmuted: the user still hears it.
//!
//! The contract, as `pipewire.rs` keeps it: `support()` answers quickly
//! enough to call when the share dialog opens; `Capture::start` captures
//! `source` (an entire screen: everything but Opencord's own process and its
//! children; a window: that app's processes) as 48 kHz interleaved stereo
//! `f32` and sends [`Captured`] blocks to `frames`, `position` counting
//! frames on the capture's own clock and `captured` when its first frame
//! was; `share` switches the source while live; dropping it stops.
//!
//! An app's processes are those whose bundle ID is the app's or starts
//! with it (helpers), and those macOS holds the app responsible for
//! (Safari's and other browsers' web content processes), or the process
//! and its children for `Source::Process`. The first tap asks the user for
//! audio capture (`NSAudioCaptureUsageDescription`); a refusal gives
//! silence, not an error, which is only logged.

use std::collections::BTreeMap;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use audioadapter_buffers::direct::InterleavedSlice;
use block2::RcBlock;
use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, available};
use objc2_app_kit::NSRunningApplication;
use objc2_core_audio::{
    AudioDeviceCreateIOProcIDWithBlock, AudioDeviceDestroyIOProcID, AudioDeviceIOProcID,
    AudioDeviceStart, AudioDeviceStop, AudioHardwareCreateAggregateDevice,
    AudioHardwareCreateProcessTap, AudioHardwareDestroyAggregateDevice,
    AudioHardwareDestroyProcessTap, AudioObjectAddPropertyListenerBlock,
    AudioObjectGetPropertyData, AudioObjectGetPropertyDataSize, AudioObjectID,
    AudioObjectPropertyAddress, AudioObjectRemovePropertyListenerBlock, CATapDescription,
    kAudioAggregateDeviceIsPrivateKey, kAudioAggregateDeviceMainSubDeviceKey,
    kAudioAggregateDeviceNameKey, kAudioAggregateDeviceSubDeviceListKey,
    kAudioAggregateDeviceTapAutoStartKey, kAudioAggregateDeviceTapListKey,
    kAudioAggregateDeviceUIDKey, kAudioDevicePropertyDeviceUID,
    kAudioHardwarePropertyDefaultOutputDevice, kAudioHardwarePropertyProcessObjectList,
    kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject,
    kAudioProcessPropertyBundleID, kAudioProcessPropertyIsRunningOutput, kAudioProcessPropertyPID,
    kAudioSubDeviceUIDKey, kAudioSubTapDriftCompensationKey, kAudioSubTapUIDKey,
    kAudioTapPropertyFormat, kAudioTapPropertyUID,
};
use objc2_core_audio_types::{
    AudioBuffer, AudioBufferList, AudioStreamBasicDescription, AudioTimeStamp,
};
use objc2_core_foundation::{CFRetained, CFString};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString};
use rubato::{Fft, FixedSync, Resampler};

use super::{AppInfo, CaptureError, Captured, ShareAudioSupport, Source};

const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: usize = 2;
/// Set to 1, a screen's sound keeps Opencord's own in: only for
/// `app/tool/macos/check_v7.sh --mutation`, which must then fail.
const INCLUDE_OPENCORD: &str = "OPENCORD_SHARE_AUDIO_INCLUDE_SELF";
/// A tap silent this long while apps play is logged: it may not be allowed.
const SILENT_FOR: Duration = Duration::from_secs(2);

pub fn support() -> ShareAudioSupport {
    if available!(macos = 14.2) {
        ShareAudioSupport {
            supported: true,
            reason: None,
        }
    } else {
        ShareAudioSupport {
            supported: false,
            reason: Some("Sharing sound needs macOS 14.2 or later".to_owned()),
        }
    }
}

fn address(selector: u32) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    }
}

/// A fixed-size property of `object`: a number, an ID, a pointer or a
/// plain C struct, for which all zero bytes are a value.
fn property<T: Copy>(object: AudioObjectID, selector: u32) -> Option<T> {
    // SAFETY: see above.
    let mut value: T = unsafe { std::mem::zeroed() };
    let mut size = std::mem::size_of::<T>() as u32;
    let mut address = address(selector);
    // SAFETY: `value` has room for `size` bytes.
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&mut address),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::from(&mut value).cast(),
        )
    };
    (status == 0).then_some(value)
}

/// A list of object IDs.
fn objects(object: AudioObjectID, selector: u32) -> Vec<AudioObjectID> {
    let mut address = address(selector);
    let mut size = 0u32;
    // SAFETY: asks only the size.
    let status = unsafe {
        AudioObjectGetPropertyDataSize(
            object,
            NonNull::from(&mut address),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
        )
    };
    if status != 0 || size == 0 {
        return Vec::new();
    }
    let mut list = vec![0 as AudioObjectID; size as usize / std::mem::size_of::<AudioObjectID>()];
    // SAFETY: `list` has room for `size` bytes.
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&mut address),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::new(list.as_mut_ptr())
                .expect("a vector's pointer")
                .cast(),
        )
    };
    if status != 0 {
        return Vec::new();
    }
    list.truncate(size as usize / std::mem::size_of::<AudioObjectID>());
    list
}

/// A string property (the caller owns the CFString it copies).
fn string(object: AudioObjectID, selector: u32) -> Option<String> {
    let value: *const CFString =
        property(object, selector).filter(|value: &*const CFString| !value.is_null())?;
    // SAFETY: a +1 CFString from a "copy" property.
    let value = unsafe { CFRetained::from_raw(NonNull::new(value.cast_mut())?) };
    Some(value.to_string())
}

/// A process with Core Audio, as the system lists them.
#[derive(Debug, Clone)]
struct Process {
    object: AudioObjectID,
    pid: i32,
    bundle: Option<String>,
    playing: bool,
}

fn processes() -> Vec<Process> {
    objects(
        kAudioObjectSystemObject as AudioObjectID,
        kAudioHardwarePropertyProcessObjectList,
    )
    .into_iter()
    .filter_map(|object| {
        Some(Process {
            object,
            pid: property::<i32>(object, kAudioProcessPropertyPID)?,
            bundle: string(object, kAudioProcessPropertyBundleID).filter(|b| !b.is_empty()),
            playing: property::<u32>(object, kAudioProcessPropertyIsRunningOutput)
                .is_some_and(|running| running != 0),
        })
    })
    .collect()
}

unsafe extern "C" {
    fn proc_pidinfo(pid: i32, flavor: i32, arg: u64, buffer: *mut c_void, size: i32) -> i32;
    fn proc_name(pid: i32, buffer: *mut c_void, size: u32) -> i32;
    fn dlsym(handle: *mut c_void, symbol: *const std::ffi::c_char) -> *mut c_void;
    fn getpid() -> i32;
}

/// `pid`'s parent, from `proc_pidinfo(PROC_PIDTBSDINFO)`.
fn parent(pid: i32) -> Option<i32> {
    // struct proc_bsdinfo: four u32 fields, then pbi_ppid; 136 bytes.
    let mut info = [0u32; 34];
    // SAFETY: the buffer is as big as `struct proc_bsdinfo`.
    let size = unsafe { proc_pidinfo(pid, 3, 0, info.as_mut_ptr().cast(), 136) };
    (size == 136).then(|| info[4] as i32)
}

/// `pid`'s executable name (for command-line players, which are no app).
fn executable(pid: i32) -> Option<String> {
    let mut name = [0u8; 256];
    // SAFETY: the buffer is as long as said.
    let length = unsafe { proc_name(pid, name.as_mut_ptr().cast(), name.len() as u32) };
    (length > 0).then(|| String::from_utf8_lossy(&name[..length as usize]).into_owned())
}

/// The process macOS holds responsible for `pid` (the app that launched an
/// XPC service such as a browser's web content process), if it says.
fn responsible(pid: i32) -> Option<i32> {
    type Responsible = unsafe extern "C" fn(i32) -> i32;
    static FUNCTION: std::sync::OnceLock<Option<Responsible>> = std::sync::OnceLock::new();
    let function = FUNCTION.get_or_init(|| {
        // SAFETY: looks a symbol up in the loaded images (RTLD_DEFAULT).
        let symbol = unsafe {
            dlsym(
                -2isize as *mut c_void,
                c"responsibility_get_pid_responsible_for_pid".as_ptr(),
            )
        };
        // SAFETY: the symbol's C type, when it is there.
        (!symbol.is_null())
            .then(|| unsafe { std::mem::transmute::<*mut c_void, Responsible>(symbol) })
    });
    // SAFETY: takes and returns a pid.
    let owner = unsafe { (*function)?(pid) };
    (owner > 0).then_some(owner)
}

/// An app as the Dock knows it: its bundle ID and name.
fn app(pid: i32) -> Option<(Option<String>, String)> {
    let app = NSRunningApplication::runningApplicationWithProcessIdentifier(pid)?;
    let name = app.localizedName()?.to_string();
    Some((app.bundleIdentifier().map(|id| id.to_string()), name))
}

/// Whether `process` belongs to Opencord: this process, its children, or
/// what it is responsible for.
fn opencord(process: &Process, own: i32) -> bool {
    process.pid == own || parent(process.pid) == Some(own) || responsible(process.pid) == Some(own)
}

/// Whether `process` is part of what `source` shares (`Everything` and
/// `Nothing` aside).
fn belongs(process: &Process, source: &Source) -> bool {
    match source {
        Source::App(wanted) => {
            let by_bundle =
                |bundle: &str| bundle == wanted || bundle.starts_with(&format!("{wanted}."));
            if process.bundle.as_deref().is_some_and(by_bundle)
                || executable(process.pid).is_some_and(|name| name == *wanted)
            {
                return true;
            }
            let owner = responsible(process.pid).unwrap_or(process.pid);
            app(owner).is_some_and(|(bundle, name)| {
                name == *wanted || bundle.as_deref().is_some_and(by_bundle)
            })
        }
        Source::Process(pid) => {
            let pid = *pid as i32;
            process.pid == pid
                || parent(process.pid) == Some(pid)
                || responsible(process.pid) == Some(pid)
        }
        Source::Everything | Source::Nothing => false,
    }
}

/// The apps playing now, Opencord left out, each once, by the app macOS
/// holds responsible.
pub fn apps_with_audio() -> Result<Vec<AppInfo>, CaptureError> {
    if !support().supported {
        return Ok(Vec::new());
    }
    // SAFETY: no arguments.
    let own = unsafe { getpid() };
    let mut apps: BTreeMap<String, AppInfo> = BTreeMap::new();
    for process in processes() {
        if !process.playing || opencord(&process, own) {
            continue;
        }
        let owner = responsible(process.pid).unwrap_or(process.pid);
        let Some((_, name)) = app(owner) else {
            continue;
        };
        apps.entry(name.clone()).or_insert(AppInfo {
            name,
            icon: None,
            pid: Some(owner as u32),
        });
    }
    Ok(apps.into_values().collect())
}

/// Two channels from one rate to 48 kHz, fed any amount at a time.
struct Resample {
    resampler: Fft<f32>,
    chunk: usize,
    pending: Vec<f32>,
    output: Vec<f32>,
}

impl Resample {
    fn new(from: u32) -> Option<Self> {
        let chunk = (from / 100).max(1) as usize;
        let resampler = Fft::<f32>::new(
            from as usize,
            SAMPLE_RATE as usize,
            chunk,
            CHANNELS,
            FixedSync::Input,
        )
        .ok()?;
        let output = vec![0.0; resampler.output_frames_max() * CHANNELS];
        Some(Self {
            resampler,
            chunk,
            pending: Vec::with_capacity(chunk * CHANNELS),
            output,
        })
    }

    /// Resamples interleaved stereo, appending what comes out to `out`.
    fn process(&mut self, samples: &[f32], out: &mut Vec<f32>) {
        for frame in samples.as_chunks::<CHANNELS>().0 {
            self.pending.extend_from_slice(frame);
            if self.pending.len() < self.chunk * CHANNELS {
                continue;
            }
            let frames_out = self.output.len() / CHANNELS;
            let input = InterleavedSlice::new(&self.pending[..], CHANNELS, self.chunk);
            let output = InterleavedSlice::new_mut(&mut self.output[..], CHANNELS, frames_out);
            if let (Ok(input), Ok(mut output)) = (input, output)
                && let Ok((_, produced)) =
                    self.resampler
                        .process_into_buffer(&input, &mut output, None)
            {
                out.extend_from_slice(&self.output[..produced * CHANNELS]);
            }
            self.pending.clear();
        }
    }
}

/// What the IOProc shares with the capture.
struct Output {
    frames: SyncSender<Captured>,
    /// Frames sent so far, at 48 kHz: the capture's own clock.
    position: AtomicU64,
    /// The tap's rate, when it isn't 48 kHz.
    resample: Mutex<Option<Resample>>,
    /// When the sound went all zero, and whether that was logged.
    silent_since: Mutex<Option<Instant>>,
    silence_logged: AtomicBool,
}

impl Output {
    /// One IOProc call's input: stereo, at 48 kHz, sent on.
    fn push(&self, input: &AudioBufferList) {
        let captured_at = Instant::now();
        // SAFETY: the list has `mNumberBuffers` buffers, each with
        // `mDataByteSize` bytes of f32 samples.
        let buffers: &[AudioBuffer] = unsafe {
            std::slice::from_raw_parts(input.mBuffers.as_ptr(), input.mNumberBuffers as usize)
        };
        let channel = |buffer: &AudioBuffer| -> &[f32] {
            if buffer.mData.is_null() {
                return &[];
            }
            // SAFETY: see above.
            unsafe {
                std::slice::from_raw_parts(
                    buffer.mData.cast::<f32>(),
                    buffer.mDataByteSize as usize / 4,
                )
            }
        };
        let mut stereo = Vec::new();
        match buffers {
            [] => return,
            [one] if one.mNumberChannels >= 2 => {
                let count = one.mNumberChannels as usize;
                for frame in channel(one).chunks_exact(count) {
                    stereo.extend_from_slice(&frame[..2]);
                }
            }
            [one] => {
                for sample in channel(one) {
                    stereo.extend_from_slice(&[*sample, *sample]);
                }
            }
            [left, right, ..] => {
                for (l, r) in channel(left).iter().zip(channel(right)) {
                    stereo.extend_from_slice(&[*l, *r]);
                }
            }
        }
        let samples = match self.resample.lock().ok().as_deref_mut() {
            Some(Some(resample)) => {
                let mut out = Vec::with_capacity(stereo.len() * 2);
                resample.process(&stereo, &mut out);
                out
            }
            _ => stereo,
        };
        if samples.is_empty() {
            return;
        }
        self.watch_silence(&samples, captured_at);
        let frames = samples.len() / CHANNELS;
        let length = Duration::from_secs_f64(frames as f64 / f64::from(SAMPLE_RATE));
        let position = self.position.fetch_add(frames as u64, Ordering::Relaxed);
        let _ = self.frames.try_send(Captured {
            samples,
            position,
            captured: captured_at.checked_sub(length).unwrap_or(captured_at),
        });
    }

    /// Logs once when the tap has been silent for [`SILENT_FOR`] while
    /// apps play: the user may have refused audio capture, which can't be
    /// told apart otherwise.
    fn watch_silence(&self, samples: &[f32], now: Instant) {
        let Ok(mut since) = self.silent_since.lock() else {
            return;
        };
        if samples.iter().any(|sample| *sample != 0.0) {
            *since = None;
            return;
        }
        let started = *since.get_or_insert(now);
        if now.duration_since(started) >= SILENT_FOR
            && !self.silence_logged.swap(true, Ordering::Relaxed)
        {
            std::thread::spawn(|| {
                if apps_with_audio().is_ok_and(|apps| !apps.is_empty()) {
                    tracing::warn!(
                        "the shared sound has been silent for 2 s while apps play; \
                         audio capture may not be allowed for Opencord"
                    );
                }
            });
        }
    }
}

/// What Core Audio calls with each IO cycle's buffers.
type IoBlock = dyn Fn(
    NonNull<AudioTimeStamp>,
    NonNull<AudioBufferList>,
    NonNull<AudioTimeStamp>,
    NonNull<AudioBufferList>,
    NonNull<AudioTimeStamp>,
);

/// A tap in its private aggregate device, running.
struct Tap {
    tap: AudioObjectID,
    device: AudioObjectID,
    proc_id: AudioDeviceIOProcID,
    _block: RcBlock<IoBlock>,
}

// SAFETY: the IDs are plain numbers and the block is only kept to stay
// alive; Core Audio calls it on its own thread.
unsafe impl Send for Tap {}

impl Drop for Tap {
    fn drop(&mut self) {
        // SAFETY: in the order the plan gives: stop, then destroy the
        // IOProc, the aggregate device and the tap.
        unsafe {
            AudioDeviceStop(self.device, self.proc_id);
            AudioDeviceDestroyIOProcID(self.device, self.proc_id);
            AudioHardwareDestroyAggregateDevice(self.device);
            AudioHardwareDestroyProcessTap(self.tap);
        }
    }
}

/// The default output device's UID: the aggregate device's main device.
fn default_output_uid() -> Option<String> {
    let device: AudioObjectID = property(
        kAudioObjectSystemObject as AudioObjectID,
        kAudioHardwarePropertyDefaultOutputDevice,
    )?;
    string(device, kAudioDevicePropertyDeviceUID)
}

/// Taps everything but `excluded`, or a mixdown of `included`, into a
/// private aggregate device whose IOProc feeds `output`.
fn tap(
    everything: bool,
    processes: &[AudioObjectID],
    output: &Arc<Output>,
) -> Result<Tap, CaptureError> {
    let failed =
        |what: &str, status: i32| CaptureError::Failed(format!("{what} (OSStatus {status})"));
    let list: Vec<Retained<NSNumber>> = processes
        .iter()
        .map(|object| NSNumber::new_u32(*object))
        .collect();
    let list = NSArray::from_retained_slice(&list);
    // SAFETY: CoreAudio's tap description (14.2+, checked by the caller).
    let description = unsafe {
        let description = if everything {
            CATapDescription::initStereoGlobalTapButExcludeProcesses(
                CATapDescription::alloc(),
                &list,
            )
        } else {
            CATapDescription::initStereoMixdownOfProcesses(CATapDescription::alloc(), &list)
        };
        description.setPrivate(true);
        description.setName(&NSString::from_str("Opencord screen sound"));
        description
    };
    let mut tap_id: AudioObjectID = 0;
    // SAFETY: a valid description; the out-pointer is valid.
    let status = unsafe { AudioHardwareCreateProcessTap(Some(&description), &mut tap_id) };
    if status != 0 {
        return Err(failed("the sound tap could not be made", status));
    }
    let destroy_tap = |error: CaptureError| {
        // SAFETY: the tap just made.
        unsafe { AudioHardwareDestroyProcessTap(tap_id) };
        error
    };
    let tap_uid = string(tap_id, kAudioTapPropertyUID)
        .ok_or_else(|| destroy_tap(CaptureError::Failed("the tap has no UID".to_owned())))?;
    let format: AudioStreamBasicDescription = property(tap_id, kAudioTapPropertyFormat).unwrap_or(
        // SAFETY: a plain C struct.
        unsafe { std::mem::zeroed() },
    );
    let rate = format.mSampleRate.round() as u32;
    if let Ok(mut resample) = output.resample.lock() {
        *resample = (rate != 0 && rate != SAMPLE_RATE)
            .then(|| Resample::new(rate))
            .flatten();
    }

    let key = |key: &std::ffi::CStr| NSString::from_str(key.to_str().unwrap_or_default());
    let uid = format!("dev.opencord.screen-sound.{tap_uid}");
    let yes = NSNumber::new_bool(true);
    let tap_entry: Retained<NSDictionary<NSString, AnyObject>> =
        NSDictionary::from_retained_objects(
            &[
                &*key(kAudioSubTapUIDKey),
                &*key(kAudioSubTapDriftCompensationKey),
            ],
            &[any(NSString::from_str(&tap_uid)), any(yes.clone())],
        );
    let mut keys = vec![
        key(kAudioAggregateDeviceUIDKey),
        key(kAudioAggregateDeviceNameKey),
        key(kAudioAggregateDeviceIsPrivateKey),
        key(kAudioAggregateDeviceTapAutoStartKey),
        key(kAudioAggregateDeviceTapListKey),
    ];
    let mut values: Vec<Retained<AnyObject>> = vec![
        any(NSString::from_str(&uid)),
        any(NSString::from_str("Opencord screen sound")),
        any(yes.clone()),
        any(yes),
        any(NSArray::from_retained_slice(&[tap_entry])),
    ];
    if let Some(main) = default_output_uid() {
        let sub: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_retained_objects(
            &[&*key(kAudioSubDeviceUIDKey)],
            &[any(NSString::from_str(&main))],
        );
        keys.push(key(kAudioAggregateDeviceMainSubDeviceKey));
        values.push(any(NSString::from_str(&main)));
        keys.push(key(kAudioAggregateDeviceSubDeviceListKey));
        values.push(any(NSArray::from_retained_slice(&[sub])));
    }
    let key_refs: Vec<&NSString> = keys.iter().map(|key| &**key).collect();
    let settings = NSDictionary::<NSString, AnyObject>::from_retained_objects(&key_refs, &values);
    let mut device: AudioObjectID = 0;
    // SAFETY: NSDictionary and CFDictionary are toll-free bridged.
    let status = unsafe {
        let settings: *const NSDictionary<NSString, AnyObject> = &*settings;
        AudioHardwareCreateAggregateDevice(&*settings.cast(), NonNull::from(&mut device))
    };
    if status != 0 {
        return Err(destroy_tap(failed(
            "the sound tap's device could not be made",
            status,
        )));
    }
    let destroy_both = |error: CaptureError| {
        // SAFETY: the device and tap just made.
        unsafe { AudioHardwareDestroyAggregateDevice(device) };
        destroy_tap(error)
    };
    let fed = Arc::clone(output);
    let block = RcBlock::new(
        move |_now: NonNull<AudioTimeStamp>,
              input: NonNull<AudioBufferList>,
              _input_time: NonNull<AudioTimeStamp>,
              _output: NonNull<AudioBufferList>,
              _output_time: NonNull<AudioTimeStamp>| {
            // SAFETY: valid for the call.
            fed.push(unsafe { input.as_ref() });
        },
    );
    let mut proc_id: AudioDeviceIOProcID = None;
    // SAFETY: the block lives in the returned `Tap` until the IOProc is
    // destroyed; a null queue has Core Audio call it on its IO thread.
    let status = unsafe {
        AudioDeviceCreateIOProcIDWithBlock(
            NonNull::from(&mut proc_id),
            device,
            None,
            RcBlock::as_ptr(&block),
        )
    };
    if status != 0 {
        return Err(destroy_both(failed(
            "the sound tap could not be read",
            status,
        )));
    }
    // SAFETY: the IOProc just made.
    let status = unsafe { AudioDeviceStart(device, proc_id) };
    if status != 0 {
        // SAFETY: as above.
        unsafe { AudioDeviceDestroyIOProcID(device, proc_id) };
        return Err(destroy_both(failed("the sound tap did not start", status)));
    }
    tracing::debug!(
        rate,
        everything,
        processes = processes.len(),
        "sound tap started"
    );
    Ok(Tap {
        tap: tap_id,
        device,
        proc_id,
        _block: block,
    })
}

/// What the capture shares now, and the tap doing it.
struct Inner {
    source: Source,
    processes: Vec<AudioObjectID>,
    tap: Option<Tap>,
}

impl Inner {
    /// The tap for `source` as the processes are now; rebuilt only when
    /// they changed.
    fn follow(&mut self, source: Source, output: &Arc<Output>) {
        // SAFETY: no arguments.
        let own = unsafe { getpid() };
        let listed = processes();
        // The V7 check's mutation: leave Opencord in, which must fail it.
        let keep_own = std::env::var_os(INCLUDE_OPENCORD).is_some_and(|value| value == "1");
        let (everything, wanted) = match &source {
            Source::Everything => (
                true,
                listed
                    .iter()
                    .filter(|p| !keep_own && opencord(p, own))
                    .map(|p| p.object)
                    .collect(),
            ),
            Source::Nothing => (false, Vec::new()),
            _ => (
                false,
                listed
                    .iter()
                    .filter(|p| !opencord(p, own) && belongs(p, &source))
                    .map(|p| p.object)
                    .collect(),
            ),
        };
        let unchanged = source == self.source && wanted == self.processes;
        if unchanged && (self.tap.is_some() || (!everything && wanted.is_empty())) {
            return;
        }
        self.tap = None;
        self.source = source;
        self.processes = wanted;
        if !everything && self.processes.is_empty() {
            return;
        }
        match tap(everything, &self.processes, output) {
            Ok(tap) => self.tap = Some(tap),
            Err(error) => tracing::warn!(%error, "screen sound could not be tapped"),
        }
    }
}

pub struct Capture {
    inner: Arc<Mutex<Inner>>,
    output: Arc<Output>,
    listener: AudioObjectPropertyListenerBlockHolder,
}

/// The process-list listener, kept to remove it.
struct AudioObjectPropertyListenerBlockHolder {
    block: RcBlock<dyn Fn(u32, NonNull<AudioObjectPropertyAddress>)>,
    queue: dispatch2::DispatchRetained<DispatchQueue>,
}

// SAFETY: the tap's IDs and the listener are only touched under the mutex
// or to be removed; Core Audio calls the blocks on its own threads.
unsafe impl Send for Capture {}
unsafe impl Sync for Capture {}

impl Capture {
    pub fn start(source: Source, frames: SyncSender<Captured>) -> Result<Self, CaptureError> {
        if !support().supported {
            return Err(CaptureError::NotSupported);
        }
        let output = Arc::new(Output {
            frames,
            position: AtomicU64::new(0),
            resample: Mutex::new(None),
            silent_since: Mutex::new(None),
            silence_logged: AtomicBool::new(false),
        });
        let inner = Arc::new(Mutex::new(Inner {
            source: Source::Nothing,
            processes: Vec::new(),
            tap: None,
        }));
        lock(&inner).follow(source, &output);
        // An app's new helpers (a browser tab starting to play): follow
        // the process list.
        let (watched, fed) = (Arc::clone(&inner), Arc::clone(&output));
        let block = RcBlock::new(move |_: u32, _: NonNull<AudioObjectPropertyAddress>| {
            let mut inner = lock(&watched);
            let source = inner.source.clone();
            inner.follow(source, &fed);
        });
        let queue = DispatchQueue::new("opencord-screen-sound", None);
        let mut address = address(kAudioHardwarePropertyProcessObjectList);
        // SAFETY: the block and queue live in the capture until removed.
        unsafe {
            AudioObjectAddPropertyListenerBlock(
                kAudioObjectSystemObject as AudioObjectID,
                NonNull::from(&mut address),
                Some(&queue),
                RcBlock::as_ptr(&block),
            );
        }
        Ok(Self {
            inner,
            output,
            listener: AudioObjectPropertyListenerBlockHolder { block, queue },
        })
    }

    pub fn share(&self, source: Source) {
        lock(&self.inner).follow(source, &self.output);
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let mut address = address(kAudioHardwarePropertyProcessObjectList);
        // SAFETY: removes the listener added in `start`.
        unsafe {
            AudioObjectRemovePropertyListenerBlock(
                kAudioObjectSystemObject as AudioObjectID,
                NonNull::from(&mut address),
                Some(&self.listener.queue),
                RcBlock::as_ptr(&self.listener.block),
            );
        }
        lock(&self.inner).tap = None;
    }
}

/// An Objective-C object as `AnyObject`, for dictionaries of mixed values.
fn any(object: Retained<impl objc2::Message>) -> Retained<AnyObject> {
    // SAFETY: every Objective-C object is an AnyObject.
    unsafe { Retained::cast_unchecked(object) }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_needs_macos_14_2() {
        assert_eq!(support().supported, available!(macos = 14.2));
    }

    #[test]
    fn this_process_is_opencord_and_its_parent_is_known() {
        // SAFETY: no arguments.
        let own = unsafe { getpid() };
        let me = Process {
            object: 0,
            pid: own,
            bundle: None,
            playing: false,
        };

        assert!(opencord(&me, own));
        assert!(parent(own).is_some_and(|parent| parent > 0));
        assert!(belongs(&me, &Source::Process(own as u32)));
        assert!(!belongs(
            &me,
            &Source::App("com.example.nothing".to_owned())
        ));
    }

    #[test]
    fn a_resampler_turns_44_1_khz_into_48() {
        let mut resample = Resample::new(44_100).unwrap();
        let input: Vec<f32> = (0..44_100)
            .flat_map(|n| {
                let value = (n as f32 * 0.01).sin();
                [value, -value]
            })
            .collect();
        let mut out = Vec::new();

        resample.process(&input, &mut out);

        let frames = out.len() / CHANNELS;
        assert!((47_000..=48_000).contains(&frames), "{frames} frames");
    }
}
