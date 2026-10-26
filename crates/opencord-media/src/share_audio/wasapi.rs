//! Windows' screen sound (docs/plans/windows.md W7): WASAPI process
//! loopback, as Microsoft's ApplicationLoopback sample does it.
//!
//! The contract, as `pipewire.rs` keeps it: `support()` answers quickly
//! enough to call when the share dialog opens; `Capture::start` captures
//! `source` (an entire screen: everything but Opencord's own process and its
//! children; a window: that app's processes) as 48 kHz interleaved stereo
//! `f32` and sends [`Captured`] blocks to `frames`, `position` counting
//! frames on the capture's own clock and `captured` when its first frame
//! was; `share` switches the source while live; dropping it stops.
//!
//! Process loopback is a *virtual* audio device, activated by
//! `ActivateAudioInterfaceAsync` rather than opened from the device
//! enumerator. Microsoft documents it from build 20348, but it works from
//! Windows 10 2004 (build **19041**), which is where OBS and GStreamer turn
//! it on too. Below that `support()` says so and nothing here runs.
//!
//! A process loopback client cannot be retargeted, so `share` tears the
//! client down and starts another one.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

use windows::Wdk::System::SystemServices::RtlGetVersion;
use windows::Win32::Foundation::{CloseHandle, HANDLE, MAX_PATH, S_OK, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::{
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
    AUDCLNT_STREAMFLAGS_LOOPBACK, AUDIOCLIENT_ACTIVATION_PARAMS, AUDIOCLIENT_ACTIVATION_PARAMS_0,
    AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK, AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS,
    ActivateAudioInterfaceAsync, AudioSessionStateActive, IActivateAudioInterfaceAsyncOperation,
    IActivateAudioInterfaceCompletionHandler, IActivateAudioInterfaceCompletionHandler_Impl,
    IAudioCaptureClient, IAudioClient, IAudioSessionControl2, IAudioSessionManager2,
    IMMDeviceEnumerator, MMDeviceEnumerator, PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE,
    PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE, VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
    WAVEFORMATEX, WAVEFORMATEXTENSIBLE, WAVEFORMATEXTENSIBLE_0, eConsole, eRender,
};
use windows::Win32::Media::KernelStreaming::{KSDATAFORMAT_SUBTYPE_PCM, WAVE_FORMAT_EXTENSIBLE};
use windows::Win32::Media::Multimedia::KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;
use windows::Win32::System::Com::StructuredStorage::{PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::System::SystemInformation::OSVERSIONINFOW;
use windows::Win32::System::Threading::{
    CreateEventW, GetCurrentProcessId, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW, SetEvent, WaitForSingleObject,
};
use windows::Win32::System::Variant::VT_BLOB;
use windows::core::{Interface, PWSTR, implement};

use super::{AppInfo, CaptureError, Captured, ShareAudioSupport, Source};

/// Never offered as something to share the sound of: Opencord itself, in
/// any instance and not only this one, and Windows' own audio engine, which
/// holds a session but is not an app anybody means.
const NEVER: [&str; 2] = ["opencord", "audiodg"];

/// The first build process loopback works on: Windows 10 2004.
const NEEDS_BUILD: u32 = 19041;

/// What the app shows beside the disabled switch below that.
const TOO_OLD: &str = "Sharing sound needs Windows 10 version 2004 or newer";

/// Everything here is 48 kHz stereo, like the rest of the engine.
const RATE: u32 = 48_000;
const CHANNELS: u16 = 2;

/// How long a block of capture is. The device's own period is whatever it
/// is; this only sizes the buffer asked for.
const BUFFER: i64 = 20 * 10_000;

fn failed(error: windows::core::Error) -> CaptureError {
    CaptureError::Failed(format!("{error} ({:#010x})", error.code().0))
}

/// This Windows' build number. `RtlGetVersion` is used rather than
/// `GetVersionExW`, which lies about anything past 8.1 unless the exe
/// carries a compatibility manifest.
fn build() -> u32 {
    let mut info = OSVERSIONINFOW {
        dwOSVersionInfoSize: size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    // SAFETY: `info` is sized as the call requires and is only read after
    // the call says it succeeded.
    let status = unsafe { RtlGetVersion(&mut info) };
    if status.is_ok() {
        info.dwBuildNumber
    } else {
        0
    }
}

pub fn support() -> ShareAudioSupport {
    let build = build();
    let supported = build >= NEEDS_BUILD;
    ShareAudioSupport {
        supported,
        reason: (!supported).then(|| TOO_OLD.to_owned()),
    }
}

/// COM for as long as this is kept, on this thread.
struct Com {
    owns: bool,
}

impl Com {
    fn new() -> Result<Self, CaptureError> {
        // Multithreaded, on our own threads and never Flutter's UI thread
        // (windows.md §3, decision A).
        // SAFETY: undone in `Drop`, on this same thread.
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if result.is_err() {
            return Err(CaptureError::Failed(format!(
                "COM would not start ({:#010x})",
                result.0
            )));
        }
        Ok(Self {
            owns: result.is_ok(),
        })
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.owns {
            // SAFETY: this thread initialised COM in `new`.
            unsafe { CoUninitialize() };
        }
    }
}

/// A process's executable name, and what people call the app: the exe's
/// `FileDescription`, falling back to the file name without `.exe`.
fn app_of_process(pid: u32) -> Option<String> {
    // SAFETY: the handle is closed on every path out; the buffer is as long
    // as the length passed in.
    let path = unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut path = [0u16; MAX_PATH as usize];
        let mut length = path.len() as u32;
        let got = QueryFullProcessImageNameW(
            process,
            Default::default(),
            PWSTR(path.as_mut_ptr()),
            &mut length,
        );
        let _ = CloseHandle(process);
        got.ok()?;
        String::from_utf16_lossy(&path[..length as usize])
    };
    std::path::Path::new(&path)
        .file_stem()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
}

pub fn apps_with_audio() -> Result<Vec<AppInfo>, CaptureError> {
    let _com = Com::new()?;
    // SAFETY: COM objects taken and released by the wrappers; the session
    // list is walked by index within the count it reported.
    unsafe {
        let devices: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(failed)?;
        let speaker = devices
            .GetDefaultAudioEndpoint(eRender, eConsole)
            .map_err(failed)?;
        let manager: IAudioSessionManager2 = speaker.Activate(CLSCTX_ALL, None).map_err(failed)?;
        let sessions = manager.GetSessionEnumerator().map_err(failed)?;
        let count = sessions.GetCount().map_err(failed)?;
        let us = GetCurrentProcessId();
        let mut apps: Vec<AppInfo> = Vec::new();
        for index in 0..count {
            let Ok(session) = sessions.GetSession(index) else {
                continue;
            };
            let Ok(session) = session.cast::<IAudioSessionControl2>() else {
                continue;
            };
            // Only what is actually playing: an app that has stopped keeps
            // its session for a while.
            if session.GetState() != Ok(AudioSessionStateActive) {
                continue;
            }
            let Ok(pid) = session.GetProcessId() else {
                continue;
            };
            if pid == 0 || pid == us {
                continue;
            }
            let Some(name) = app_of_process(pid) else {
                continue;
            };
            // Never Opencord — sharing our own output is the feedback loop
            // §10.1 exists to avoid — and by name, not only by this
            // process's id: a second Opencord open beside this one would
            // otherwise be offered. Measured: it was.
            if NEVER.iter().any(|never| name.eq_ignore_ascii_case(never)) {
                continue;
            }
            // One row per app, not per session: a browser has one per tab.
            if apps.iter().any(|app| app.name == name) {
                continue;
            }
            apps.push(AppInfo {
                name,
                // Windows has no icon theme to name; the app shows its
                // initial instead.
                icon: None,
                pid: Some(pid),
            });
        }
        apps.sort_by_key(|app| app.name.to_lowercase());
        Ok(apps)
    }
}

/// Which process tree to capture, and whether to include or exclude it.
fn target_of(source: &Source) -> Option<(u32, bool)> {
    match source {
        // Everything but us: exclude our own tree, so the call's own sound
        // never goes back out.
        // SAFETY: a read of our own process id.
        Source::Everything => Some((excluded_pid(), false)),
        Source::Process(pid) => Some((*pid, true)),
        // A name, resolved to the process now playing under it.
        Source::App(name) => apps_with_audio()
            .ok()?
            .into_iter()
            .find(|app| app.name.eq_ignore_ascii_case(name))
            .and_then(|app| app.pid)
            .map(|pid| (pid, true)),
        Source::Nothing => None,
    }
}

/// Whose tree a whole-screen share leaves out: Opencord's own, always.
///
/// In a debug build `OPENCORD_TEST_EXCLUDE_PID` may name a different one.
/// That is W7 step 4's mutation check and nothing else: excluding the wrong
/// process must put the call's own voice back into the shared sound, and a
/// check that cannot be made to fail has proved nothing. It is compiled out
/// of release builds entirely.
fn excluded_pid() -> u32 {
    #[cfg(debug_assertions)]
    if let Ok(pid) = std::env::var("OPENCORD_TEST_EXCLUDE_PID")
        && let Ok(pid) = pid.trim().parse::<u32>()
        && pid != 0
    {
        tracing::warn!(pid, "excluding another process, for the W7 mutation check");
        return pid;
    }
    // SAFETY: a read of our own process id.
    unsafe { GetCurrentProcessId() }
}

/// Waits for `ActivateAudioInterfaceAsync`, which answers on a thread of
/// its own.
#[implement(IActivateAudioInterfaceCompletionHandler)]
struct Ready(HANDLE);

impl IActivateAudioInterfaceCompletionHandler_Impl for Ready_Impl {
    fn ActivateCompleted(
        &self,
        _operation: windows::core::Ref<'_, IActivateAudioInterfaceAsyncOperation>,
    ) -> windows::core::Result<()> {
        // Only a wake-up: whether it worked is read from the operation by
        // whoever was waiting.
        // SAFETY: the event outlives this handler, which `start` holds.
        unsafe { SetEvent(self.0) }
    }
}

/// The format to ask the virtual device for. Float first, as the engine
/// wants; the sample falls back to 16-bit PCM, and so does this.
fn format(float: bool) -> WAVEFORMATEXTENSIBLE {
    let bits = if float { 32 } else { 16 };
    let block = CHANNELS * bits / 8;
    WAVEFORMATEXTENSIBLE {
        Format: WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_EXTENSIBLE as u16,
            nChannels: CHANNELS,
            nSamplesPerSec: RATE,
            nAvgBytesPerSec: RATE * u32::from(block),
            nBlockAlign: block,
            wBitsPerSample: bits,
            cbSize: (size_of::<WAVEFORMATEXTENSIBLE>() - size_of::<WAVEFORMATEX>()) as u16,
        },
        Samples: WAVEFORMATEXTENSIBLE_0 {
            wValidBitsPerSample: bits,
        },
        // Front left and right.
        dwChannelMask: 0x3,
        SubFormat: if float {
            KSDATAFORMAT_SUBTYPE_IEEE_FLOAT
        } else {
            KSDATAFORMAT_SUBTYPE_PCM
        },
    }
}

/// An activated process-loopback client, ready to read.
struct Client {
    audio: IAudioClient,
    capture: IAudioCaptureClient,
    /// The device signals this when there is something to read.
    ready: HANDLE,
    /// Whether the device gave us float; 16-bit PCM otherwise.
    float: bool,
}

impl Client {
    /// Activates the virtual process-loopback device for `pid`.
    fn open(pid: u32, include: bool) -> Result<Self, CaptureError> {
        // SAFETY: the activation parameters live until the call returns and
        // Windows copies them; the completion event and handler outlive the
        // wait below. Every handle is closed on the way out.
        unsafe {
            let mut params = AUDIOCLIENT_ACTIVATION_PARAMS {
                ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
                Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
                    ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                        TargetProcessId: pid,
                        ProcessLoopbackMode: if include {
                            PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE
                        } else {
                            PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE
                        },
                    },
                },
            };
            // The parameters go in as a blob, which is how this API takes
            // anything beyond a device id. Never dropped: the `windows`
            // crate's `PROPVARIANT` clears itself on drop, which frees the
            // blob's data with `CoTaskMemFree`, and that data is `params`
            // on this stack. Freeing it corrupted the heap and killed the
            // app the moment a share's sound started.
            let blob = std::mem::ManuallyDrop::new(PROPVARIANT {
                Anonymous: PROPVARIANT_0 {
                    Anonymous: std::mem::ManuallyDrop::new(PROPVARIANT_0_0 {
                        vt: VT_BLOB,
                        wReserved1: 0,
                        wReserved2: 0,
                        wReserved3: 0,
                        Anonymous:
                            windows::Win32::System::Com::StructuredStorage::PROPVARIANT_0_0_0 {
                                blob: windows::Win32::System::Com::BLOB {
                                    cbSize: size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
                                    pBlobData: std::ptr::from_mut(&mut params).cast(),
                                },
                            },
                    }),
                },
            });

            let done = CreateEventW(None, false, false, None).map_err(failed)?;
            let handler: IActivateAudioInterfaceCompletionHandler = Ready(done).into();
            let operation = ActivateAudioInterfaceAsync(
                VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
                &IAudioClient::IID,
                Some(&*blob),
                &handler,
            );
            let operation = match operation {
                Ok(operation) => operation,
                Err(error) => {
                    let _ = CloseHandle(done);
                    return Err(failed(error));
                }
            };
            // The handler only wakes us; the result comes off the operation.
            let waited = WaitForSingleObject(done, 5_000);
            let _ = CloseHandle(done);
            if waited != WAIT_OBJECT_0 {
                return Err(CaptureError::Failed(
                    "the audio device did not activate in time".to_owned(),
                ));
            }
            let mut result = S_OK;
            let mut activated = None;
            operation
                .GetActivateResult(&mut result, &mut activated)
                .map_err(failed)?;
            result.ok().map_err(|error| {
                // The one refusal worth naming: an app that is not allowed
                // to listen to another's audio.
                if error.code() == windows::Win32::Foundation::E_ACCESSDENIED {
                    CaptureError::Denied
                } else {
                    failed(error)
                }
            })?;
            let audio: IAudioClient = activated
                .ok_or_else(|| CaptureError::Failed("no audio client".to_owned()))?
                .cast()
                .map_err(failed)?;

            // Float first; 16-bit PCM if the device refuses it, as the
            // sample does. `IsFormatSupported` is not reliable on this
            // virtual device, so this just tries `Initialize`.
            let mut float = true;
            let mut wave = format(float);
            let flags = AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK;
            let mut started = audio.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                flags,
                BUFFER,
                0,
                std::ptr::from_mut(&mut wave).cast::<WAVEFORMATEX>(),
                None,
            );
            if started.is_err() {
                float = false;
                wave = format(float);
                started = audio.Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    flags,
                    BUFFER,
                    0,
                    std::ptr::from_mut(&mut wave).cast::<WAVEFORMATEX>(),
                    None,
                );
            }
            started.map_err(failed)?;

            let ready = CreateEventW(None, false, false, None).map_err(failed)?;
            if let Err(error) = audio.SetEventHandle(ready) {
                let _ = CloseHandle(ready);
                return Err(failed(error));
            }
            let capture: IAudioCaptureClient = match audio.GetService() {
                Ok(capture) => capture,
                Err(error) => {
                    let _ = CloseHandle(ready);
                    return Err(failed(error));
                }
            };
            if let Err(error) = audio.Start() {
                let _ = CloseHandle(ready);
                return Err(failed(error));
            }
            tracing::info!(pid, include, float, "sharing a screen's sound");
            Ok(Self {
                audio,
                capture,
                ready,
                float,
            })
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        // SAFETY: both were started in `open`, on this object.
        unsafe {
            let _ = self.audio.Stop();
            let _ = CloseHandle(self.ready);
        }
    }
}

/// What the capture thread is told to do.
enum Command {
    Share(Source),
    Quit,
}

/// A running capture; stops when dropped.
pub struct Capture {
    commands: Mutex<std::sync::mpsc::Sender<Command>>,
    /// Woken so the thread looks at its commands without waiting for audio
    /// that may never come.
    poke: HANDLE,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

// SAFETY: the event handle is only signalled, which is thread-safe, and is
// closed once, in `Drop`.
unsafe impl Send for Capture {}
// SAFETY: as above; `commands` is behind a mutex.
unsafe impl Sync for Capture {}

impl Capture {
    pub fn start(source: Source, frames: SyncSender<Captured>) -> Result<Self, CaptureError> {
        let support = support();
        if !support.supported {
            return Err(CaptureError::Failed(
                support.reason.unwrap_or_else(|| TOO_OLD.to_owned()),
            ));
        }
        // SAFETY: closed in `Drop`.
        let poke = unsafe { CreateEventW(None, false, false, None) }.map_err(failed)?;
        let (commands, orders) = std::sync::mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let (started, outcome) = std::sync::mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("opencord-share-audio".to_owned())
            .spawn({
                let stop = Arc::clone(&stop);
                move || run(source, &frames, &orders, &stop, &started)
            })
            .map_err(|error| CaptureError::Failed(error.to_string()))?;
        match outcome.recv() {
            Ok(Ok(())) => Ok(Self {
                commands: Mutex::new(commands),
                poke,
                stop,
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                // SAFETY: made just above and never used.
                let _ = unsafe { CloseHandle(poke) };
                Err(error)
            }
            Err(_) => {
                let _ = thread.join();
                // SAFETY: as above.
                let _ = unsafe { CloseHandle(poke) };
                Err(CaptureError::Failed("the capture thread ended".to_owned()))
            }
        }
    }

    /// Shares another source. A process loopback client cannot be pointed
    /// somewhere else, so the thread opens a new one.
    pub fn share(&self, source: Source) {
        if let Ok(commands) = self.commands.lock() {
            let _ = commands.send(Command::Share(source));
        }
        // SAFETY: the handle is live until `Drop`.
        let _ = unsafe { SetEvent(self.poke) };
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Ok(commands) = self.commands.lock() {
            let _ = commands.send(Command::Quit);
        }
        // SAFETY: live until the handle is closed below.
        let _ = unsafe { SetEvent(self.poke) };
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        // SAFETY: made in `start`, and the thread that used it has ended.
        let _ = unsafe { CloseHandle(self.poke) };
    }
}

fn run(
    mut source: Source,
    frames: &SyncSender<Captured>,
    orders: &std::sync::mpsc::Receiver<Command>,
    stop: &AtomicBool,
    started: &std::sync::mpsc::Sender<Result<(), CaptureError>>,
) {
    let Ok(_com) = Com::new() else {
        let _ = started.send(Err(CaptureError::Failed("COM would not start".to_owned())));
        return;
    };
    // Opening the first source is what `start` waits on; a later `share`
    // that fails only stops the sound, because the share is already live.
    let mut client = match open_for(&source) {
        Ok(client) => client,
        Err(error) => {
            let _ = started.send(Err(error));
            return;
        }
    };
    if started.send(Ok(())).is_err() {
        return;
    }

    let mut position = 0u64;
    while !stop.load(Ordering::Relaxed) {
        for order in orders.try_iter() {
            match order {
                Command::Share(wanted) => {
                    source = wanted;
                    client = match open_for(&source) {
                        Ok(client) => client,
                        Err(error) => {
                            tracing::warn!(%error, "the new source's sound could not be shared");
                            None
                        }
                    };
                }
                Command::Quit => return,
            }
        }
        let Some(open) = &client else {
            // Nothing to capture (`Source::Nothing`, or a source that would
            // not open). The video goes on without it.
            std::thread::sleep(std::time::Duration::from_millis(50));
            continue;
        };
        // SAFETY: the buffer is released on every path, before the next
        // `GetBuffer`; the lengths come from the call itself.
        unsafe {
            if WaitForSingleObject(open.ready, 200) != WAIT_OBJECT_0 {
                continue;
            }
            loop {
                let Ok(available) = open.capture.GetNextPacketSize() else {
                    break;
                };
                if available == 0 {
                    break;
                }
                let mut data = std::ptr::null_mut();
                let mut count = 0;
                let mut flags = 0;
                if open
                    .capture
                    .GetBuffer(&mut data, &mut count, &mut flags, None, None)
                    .is_err()
                {
                    break;
                }
                let captured = Instant::now();
                let wanted = count as usize * CHANNELS as usize;
                let samples = if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null()
                {
                    // The device says this stretch is silent and may not
                    // have bothered to write it.
                    vec![0.0; wanted]
                } else if open.float {
                    std::slice::from_raw_parts(data.cast::<f32>(), wanted).to_vec()
                } else {
                    std::slice::from_raw_parts(data.cast::<i16>(), wanted)
                        .iter()
                        .map(|sample| f32::from(*sample) / f32::from(i16::MAX))
                        .collect()
                };
                let _ = open.capture.ReleaseBuffer(count);
                if !samples.is_empty() {
                    // Dropped while the encoder is behind, as the video is.
                    let _ = frames.try_send(Captured {
                        samples,
                        position,
                        captured,
                    });
                    position += u64::from(count);
                }
            }
        }
    }
}

/// A client for `source`, or `None` where there is nothing to capture.
fn open_for(source: &Source) -> Result<Option<Client>, CaptureError> {
    match target_of(source) {
        Some((pid, include)) => Client::open(pid, include).map(Some),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_windows_says_whether_it_can_share_sound() {
        let build = build();
        assert!(build > 10_000, "a real build number, not {build}");
        let support = support();
        assert_eq!(
            support.supported,
            build >= NEEDS_BUILD,
            "build {build} against the {NEEDS_BUILD} process loopback needs"
        );
        match support.supported {
            true => assert!(support.reason.is_none()),
            // The disabled switch shows this, so it has to name the version
            // rather than say "not supported" (windows.md §3, decision C).
            false => assert_eq!(support.reason.as_deref(), Some(TOO_OLD)),
        }
    }

    #[test]
    fn a_whole_screen_excludes_opencord_and_a_window_includes_its_app() {
        // SAFETY: a read of our own process id.
        let us = unsafe { GetCurrentProcessId() };

        assert_eq!(
            target_of(&Source::Everything),
            Some((us, false)),
            "a screen captures everything *but* this process tree"
        );
        // The mutation hook, which W7 step 4's check turns on to prove the
        // check can fail. Set and unset here so no other test sees it.
        // SAFETY: single-threaded within this test, and the variable is
        // removed before it returns.
        unsafe { std::env::set_var("OPENCORD_TEST_EXCLUDE_PID", "4321") };
        let mutated = target_of(&Source::Everything);
        unsafe { std::env::remove_var("OPENCORD_TEST_EXCLUDE_PID") };
        assert_eq!(
            mutated,
            Some((4321, false)),
            "the mutation hook excludes the process it names instead"
        );
        assert_eq!(
            target_of(&Source::Everything),
            Some((us, false)),
            "and goes back to Opencord's own when it is unset"
        );
        assert_eq!(
            target_of(&Source::Process(4321)),
            Some((4321, true)),
            "a window captures that process tree and no other"
        );
        assert_eq!(target_of(&Source::Nothing), None, "nothing is nothing");
    }

    #[test]
    fn an_app_nobody_is_playing_resolves_to_nothing() {
        assert_eq!(
            target_of(&Source::App("no such app is playing".to_owned())),
            None
        );
    }

    #[test]
    fn the_format_asked_for_is_48_khz_stereo() {
        for float in [true, false] {
            let wave = format(float);
            // Copied out first: `WAVEFORMATEXTENSIBLE` is packed, so a
            // reference to a field of it would be unaligned.
            let (rate, channels, bits) = (
                wave.Format.nSamplesPerSec,
                wave.Format.nChannels,
                wave.Format.wBitsPerSample,
            );
            let (block, bytes) = (wave.Format.nBlockAlign, wave.Format.nAvgBytesPerSec);
            let mask = wave.dwChannelMask;
            assert_eq!(rate, RATE);
            assert_eq!(channels, CHANNELS);
            assert_eq!(bits, if float { 32 } else { 16 });
            // The block and byte rates have to agree with the rest, or
            // WASAPI refuses the format outright.
            assert_eq!(block, channels * bits / 8);
            assert_eq!(bytes, rate * u32::from(block));
            assert_eq!(mask, 0x3, "front left and right");
        }
    }

    #[test]
    fn starting_below_the_build_it_needs_says_so_rather_than_hanging() {
        if support().supported {
            eprintln!("this Windows has process loopback; nothing to check here");
            return;
        }
        let (frames, _) = std::sync::mpsc::sync_channel(4);
        // Not `expect_err`: `Capture` has no `Debug`, and a capture that
        // somehow started should be stopped rather than printed.
        match Capture::start(Source::Everything, frames) {
            Ok(_) => panic!("capture started on a build without process loopback"),
            Err(error) => assert!(
                format!("{error}").contains("version 2004"),
                "the reason reaches the caller: {error}"
            ),
        }
    }

    /// Opening a client hands Windows its parameters in a `PROPVARIANT`
    /// blob that points at them on the stack, which once got freed and
    /// killed the app. The process has to live through it, whether this
    /// machine's audio lets the client open or not: twice, and then the
    /// heap is used, since a bad free may only show on a later allocation.
    #[test]
    fn opening_a_client_leaves_the_process_alive() {
        if !support().supported {
            eprintln!("this Windows has no process loopback; nothing to check here");
            return;
        }
        let _com = Com::new().expect("COM");
        // SAFETY: a read of our own process id.
        let us = unsafe { GetCurrentProcessId() };
        for _ in 0..2 {
            match Client::open(us, false) {
                Ok(_) => eprintln!("a loopback client opened"),
                Err(error) => eprintln!("no loopback client here: {error}"),
            }
        }
        let used: Vec<Vec<u8>> = (0..1_000).map(|size| vec![1; size]).collect();
        assert_eq!(used.iter().map(Vec::len).sum::<usize>(), 499_500);
    }

    /// `apps_with_audio` talks to the ordinary session API, which works on
    /// every Windows — so this runs here even though capture does not.
    #[test]
    fn the_apps_playing_now_are_listed_without_opencord() {
        let apps = match apps_with_audio() {
            Ok(apps) => apps,
            Err(error) => {
                eprintln!("no audio sessions to list here: {error}");
                return;
            }
        };
        // SAFETY: a read of our own process id.
        let us = unsafe { GetCurrentProcessId() };
        for app in &apps {
            eprintln!("{} (pid {:?})", app.name, app.pid);
            assert!(!app.name.is_empty(), "every app has a name");
            assert_ne!(app.pid, Some(us), "never this process");
            // And never Opencord under any id: a second one open beside
            // this one is still Opencord. Nor Windows' own audio engine,
            // which holds a session but is not an app anybody means.
            for never in NEVER {
                assert!(
                    !app.name.eq_ignore_ascii_case(never),
                    "{never} should not be offered as something to share"
                );
            }
        }
        let mut names: Vec<&str> = apps.iter().map(|app| app.name.as_str()).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "one row per app, not per session");
    }
}
