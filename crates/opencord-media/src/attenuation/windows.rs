//! Windows: every other app's audio session on the default speaker gets
//! its volume (`ISimpleAudioVolume`) scaled down, and put back as it was.
//! Sessions that start while others are lowered are found within a second
//! and lowered too. Windows' system sounds and Opencord's own sessions are
//! left alone.

use std::collections::HashMap;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::Duration;

use windows::Win32::Foundation::S_OK;
use windows::Win32::Media::Audio::{
    IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, ISimpleAudioVolume,
    MMDeviceEnumerator, eConsole, eRender,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize,
};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::core::Interface;

pub const SUPPORTED: bool = true;

/// How often sessions are looked for while others are lowered.
const RESCAN_EVERY: Duration = Duration::from_secs(1);

enum Command {
    Set(Option<f32>),
}

pub struct Attenuator {
    commands: Option<mpsc::Sender<Command>>,
    thread: Option<JoinHandle<()>>,
}

impl Attenuator {
    pub fn start() -> Option<Self> {
        let (commands, received) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("opencord-attenuation".to_owned())
            .spawn(move || serve(&received))
            .ok()?;
        Some(Self {
            commands: Some(commands),
            thread: Some(thread),
        })
    }

    pub fn set(&self, level: Option<f32>) {
        if let Some(commands) = &self.commands {
            let _ = commands.send(Command::Set(level));
        }
    }
}

impl Drop for Attenuator {
    fn drop(&mut self) {
        // Closing the channel puts everything back and ends the thread.
        self.commands.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// A lowered session and what it goes back to.
struct Lowered {
    volume: ISimpleAudioVolume,
    original: f32,
}

fn serve(commands: &mpsc::Receiver<Command>) {
    // Multithreaded, on our own thread (windows.md §3, decision A).
    // SAFETY: undone below, on this same thread.
    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let mut level: Option<f32> = None;
    let mut lowered: HashMap<String, Lowered> = HashMap::new();
    loop {
        match commands.recv_timeout(RESCAN_EVERY) {
            Ok(Command::Set(new)) => {
                level = new;
                match level {
                    Some(level) => lower(&mut lowered, level),
                    None => restore(&mut lowered),
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if let Some(level) = level {
                    lower(&mut lowered, level);
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    restore(&mut lowered);
    if com.is_ok() {
        // SAFETY: this thread initialised COM above.
        unsafe { CoUninitialize() };
    }
}

/// Every other app's session at `level` of what it was.
fn lower(lowered: &mut HashMap<String, Lowered>, level: f32) {
    for (key, volume) in sessions() {
        let entry = match lowered.get(&key) {
            Some(entry) => entry,
            None => {
                // SAFETY: a live COM interface from `sessions`.
                let Ok(original) = (unsafe { volume.GetMasterVolume() }) else {
                    continue;
                };
                lowered.entry(key).or_insert(Lowered { volume, original })
            }
        };
        // SAFETY: as above; a null event context is allowed.
        let _ = unsafe {
            entry
                .volume
                .SetMasterVolume(entry.original * level, std::ptr::null())
        };
    }
}

fn restore(lowered: &mut HashMap<String, Lowered>) {
    for (_, entry) in lowered.drain() {
        // SAFETY: the interface was kept alive in the map.
        let _ = unsafe {
            entry
                .volume
                .SetMasterVolume(entry.original, std::ptr::null())
        };
    }
}

/// Other apps' sessions on the default speaker, by their instance id.
fn sessions() -> Vec<(String, ISimpleAudioVolume)> {
    // SAFETY: COM objects taken and released by the wrappers; the session
    // list is walked by index within the count it reported; the instance
    // id string is freed with CoTaskMemFree as the API requires.
    unsafe {
        let Ok(devices) =
            CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
        else {
            return Vec::new();
        };
        let Ok(speaker) = devices.GetDefaultAudioEndpoint(eRender, eConsole) else {
            return Vec::new();
        };
        let Ok(manager) = speaker.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else {
            return Vec::new();
        };
        let Ok(list) = manager.GetSessionEnumerator() else {
            return Vec::new();
        };
        let count = list.GetCount().unwrap_or(0);
        let us = GetCurrentProcessId();
        let mut found = Vec::new();
        for index in 0..count {
            let Ok(session) = list.GetSession(index) else {
                continue;
            };
            let Ok(session) = session.cast::<IAudioSessionControl2>() else {
                continue;
            };
            // S_OK means it is the system sounds session; S_FALSE not.
            if session.IsSystemSoundsSession() == S_OK {
                continue;
            }
            if session
                .GetProcessId()
                .is_ok_and(|pid| pid == us || pid == 0)
            {
                continue;
            }
            let Ok(id) = session.GetSessionInstanceIdentifier() else {
                continue;
            };
            let key = id.to_string().unwrap_or_default();
            CoTaskMemFree(Some(id.0.cast_const().cast()));
            let Ok(volume) = session.cast::<ISimpleAudioVolume>() else {
                continue;
            };
            found.push((key, volume));
        }
        found
    }
}
