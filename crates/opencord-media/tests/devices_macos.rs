//! Headsets coming and going during a call on the Mac (docs/plans/macos.md
//! M8, step 1, AirPods without AirPods): a "headset" is a Core Audio
//! aggregate device over BlackHole 2ch, listed to every app like a plugged
//! one; destroying it is pulling it out. The audio engine must fall back to
//! the default when a chosen headset goes, reopen one whose sample rate
//! changes (AirPods do when their microphone opens), and follow the default
//! when the default goes, all without stopping the microphone, which hears
//! an `afplay` tone looped back by BlackHole (silence would send almost no
//! frames: Opus leaves them out). Run by
//! `app/tool/macos/check_devices.sh`, which makes BlackHole the default
//! output and input, so nothing reaches a speaker or comes from a
//! microphone; `--ignored`.

#![cfg(target_os = "macos")]

use std::ffi::c_void;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_core_audio::{
    AudioHardwareCreateAggregateDevice, AudioHardwareDestroyAggregateDevice,
    AudioObjectGetPropertyData, AudioObjectID, AudioObjectPropertyAddress,
    AudioObjectSetPropertyData, kAudioAggregateDeviceIsPrivateKey,
    kAudioAggregateDeviceMainSubDeviceKey, kAudioAggregateDeviceNameKey,
    kAudioAggregateDeviceSubDeviceListKey, kAudioAggregateDeviceUIDKey,
    kAudioDevicePropertyNominalSampleRate, kAudioHardwarePropertyDefaultInputDevice,
    kAudioHardwarePropertyDefaultOutputDevice, kAudioHardwarePropertyTranslateUIDToDevice,
    kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject,
    kAudioSubDeviceUIDKey,
};
use objc2_core_foundation::CFString;
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString};
use opencord_media::audio::capture::InputMode;
use opencord_media::audio::device::devices;
use opencord_media::audio::engine::{AudioEngine, DeviceChoice, EngineEvent, EngineSettings};
use opencord_media::audio::processing::ProcessingSettings;
use opencord_media::audio::processor::ProcessorSettings;

const BLACKHOLE: &str = "BlackHole2ch_UID";
/// Frames go out every 20 ms: 50 a second while push-to-talk is held.
const FRAMES_A_SECOND: usize = 50;
/// The longest the microphone may be gone while it is reopened.
const REOPENED_WITHIN: Duration = Duration::from_millis(1500);

fn address(selector: u32) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    }
}

fn get<T: Copy + Default>(object: AudioObjectID, selector: u32) -> T {
    let mut value = T::default();
    let mut size = std::mem::size_of::<T>() as u32;
    // SAFETY: `value` has room for `size` bytes.
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&mut address(selector)),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::from(&mut value).cast(),
        )
    };
    assert_eq!(status, 0, "reading property {selector:#x} of {object}");
    value
}

fn set<T: Copy>(object: AudioObjectID, selector: u32, value: T) {
    // SAFETY: `value` is `size_of::<T>()` bytes.
    let status = unsafe {
        AudioObjectSetPropertyData(
            object,
            NonNull::from(&mut address(selector)),
            0,
            std::ptr::null(),
            std::mem::size_of::<T>() as u32,
            NonNull::from(&value).cast::<c_void>(),
        )
    };
    assert_eq!(status, 0, "setting property {selector:#x} of {object}");
}

/// A headset everyone sees: an aggregate device over BlackHole 2ch.
/// Dropping it pulls it out.
struct Headset {
    id: AudioObjectID,
    uid: String,
}

impl Headset {
    fn plug(name: &str) -> Self {
        let uid = format!("dev.opencord.test-headset.{}.{name}", std::process::id());
        let key = |key: &std::ffi::CStr| NSString::from_str(key.to_str().unwrap());
        let any = |object: Retained<NSString>| -> Retained<AnyObject> { object.into() };
        let sub: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_retained_objects(
            &[&*key(kAudioSubDeviceUIDKey)],
            &[any(NSString::from_str(BLACKHOLE))],
        );
        let keys = [
            key(kAudioAggregateDeviceUIDKey),
            key(kAudioAggregateDeviceNameKey),
            key(kAudioAggregateDeviceIsPrivateKey),
            key(kAudioAggregateDeviceMainSubDeviceKey),
            key(kAudioAggregateDeviceSubDeviceListKey),
        ];
        let values: [Retained<AnyObject>; 5] = [
            any(NSString::from_str(&uid)),
            any(NSString::from_str(name)),
            NSNumber::new_bool(false).into(),
            any(NSString::from_str(BLACKHOLE)),
            NSArray::from_retained_slice(&[sub]).into(),
        ];
        let key_refs: Vec<&NSString> = keys.iter().map(|key| &**key).collect();
        let settings =
            NSDictionary::<NSString, AnyObject>::from_retained_objects(&key_refs, &values);
        let mut id: AudioObjectID = 0;
        // SAFETY: NSDictionary and CFDictionary are toll-free bridged.
        let status = unsafe {
            let settings: *const NSDictionary<NSString, AnyObject> = &*settings;
            AudioHardwareCreateAggregateDevice(&*settings.cast(), NonNull::from(&mut id))
        };
        assert_eq!(
            status, 0,
            "making the headset (is BlackHole 2ch installed?)"
        );
        let headset = Self { id, uid };
        // Listed to apps a moment after it is made.
        let deadline = Instant::now() + Duration::from_secs(5);
        while !devices().outputs.iter().any(|d| d.id == headset.cpal_id()) {
            assert!(Instant::now() < deadline, "the headset is never listed");
            std::thread::sleep(Duration::from_millis(50));
        }
        headset
    }

    fn cpal_id(&self) -> String {
        format!("coreaudio:{}", self.uid)
    }

    fn make_default(&self) {
        let system = kAudioObjectSystemObject as AudioObjectID;
        set(system, kAudioHardwarePropertyDefaultOutputDevice, self.id);
        set(system, kAudioHardwarePropertyDefaultInputDevice, self.id);
    }
}

impl Drop for Headset {
    fn drop(&mut self) {
        // SAFETY: the device this made.
        unsafe { AudioHardwareDestroyAggregateDevice(self.id) };
    }
}

/// A tone `afplay` plays to the default output (BlackHole, which hands it
/// to its input) until dropped: 1 kHz at a quarter of full scale, 30 s.
struct Tone(Child);

impl Tone {
    fn play() -> Self {
        let rate = 48_000u32;
        let mut data = Vec::new();
        for n in 0..rate * 30 {
            let t = n as f32 / rate as f32;
            let value = (0.25 * (std::f32::consts::TAU * 1000.0 * t).sin() * 32_767.0) as i16;
            data.extend_from_slice(&value.to_le_bytes());
        }
        let mut file = b"RIFF".to_vec();
        file.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        file.extend_from_slice(b"WAVEfmt ");
        file.extend_from_slice(&16u32.to_le_bytes());
        file.extend_from_slice(&1u16.to_le_bytes());
        file.extend_from_slice(&1u16.to_le_bytes());
        file.extend_from_slice(&rate.to_le_bytes());
        file.extend_from_slice(&(rate * 2).to_le_bytes());
        file.extend_from_slice(&2u16.to_le_bytes());
        file.extend_from_slice(&16u16.to_le_bytes());
        file.extend_from_slice(b"data");
        file.extend_from_slice(&(data.len() as u32).to_le_bytes());
        file.extend_from_slice(&data);
        let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("devices_macos_tone.wav");
        std::fs::write(&path, file).unwrap();
        Self(Command::new("afplay").arg(&path).spawn().unwrap())
    }
}

impl Drop for Tone {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// An engine with push-to-talk held, so it sends every frame, even of
/// BlackHole's silence; it counts the frames and keeps the events.
struct Call {
    engine: AudioEngine,
    frames: Arc<AtomicUsize>,
    events: Arc<Mutex<Vec<(Instant, EngineEvent)>>>,
}

impl Call {
    fn start(input_device: DeviceChoice, output_device: DeviceChoice) -> Self {
        let frames = Arc::new(AtomicUsize::new(0));
        let events = Arc::new(Mutex::new(Vec::new()));
        let counted = Arc::clone(&frames);
        let heard = Arc::clone(&events);
        let engine = AudioEngine::start(
            EngineSettings {
                input_device,
                output_device,
                processor: ProcessorSettings {
                    mode: InputMode::PushToTalk {
                        release_delay: Duration::ZERO,
                    },
                    processing: ProcessingSettings::default(),
                    bitrate: 64_000,
                    input_volume: 1.0,
                    output_volume: 1.0,
                },
            },
            move |_| {
                counted.fetch_add(1, Ordering::Relaxed);
            },
            move |event| heard.lock().unwrap().push((Instant::now(), event)),
        )
        .unwrap();
        engine.set_push_to_talk(true);
        Self {
            engine,
            frames,
            events,
        }
    }

    /// Frames sent over the next `length`.
    fn frames_over(&self, length: Duration) -> usize {
        let before = self.frames.load(Ordering::Relaxed);
        std::thread::sleep(length);
        self.frames.load(Ordering::Relaxed) - before
    }

    /// Frames sent in each 100 ms over the next `length`.
    fn timeline(&self, length: Duration) -> Vec<usize> {
        let mut counts = Vec::new();
        let mut before = self.frames.load(Ordering::Relaxed);
        let start = Instant::now();
        let mut next = start;
        while next < start + length {
            next += Duration::from_millis(100);
            std::thread::sleep(next.saturating_duration_since(Instant::now()));
            let now = self.frames.load(Ordering::Relaxed);
            counts.push(now - before);
            before = now;
        }
        counts
    }

    /// Device events (fell back, failed) so far, with when they came.
    fn device_events(&self) -> Vec<(Instant, EngineEvent)> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, event)| {
                matches!(
                    event,
                    EngineEvent::DeviceFellBack { .. } | EngineEvent::DeviceFailed { .. }
                )
            })
            .cloned()
            .collect()
    }

    /// Waits up to `limit` for `wanted` device events.
    fn wait_for_device_events(
        &self,
        wanted: usize,
        limit: Duration,
    ) -> Vec<(Instant, EngineEvent)> {
        let deadline = Instant::now() + limit;
        loop {
            let events = self.device_events();
            if events.len() >= wanted || Instant::now() >= deadline {
                return events;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// Most of a second's frames: the microphone is running.
fn assert_sending(frames: usize, when: &str) {
    assert!(
        frames >= FRAMES_A_SECOND * 4 / 5,
        "{frames} frames in the second {when}"
    );
}

/// The longest run of 100 ms with at most one frame: how long the
/// microphone was gone.
fn gap(timeline: &[usize]) -> Duration {
    let mut longest = 0;
    let mut run = 0;
    for &frames in timeline {
        run = if frames <= 1 { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    Duration::from_millis(100 * longest)
}

/// A device's AudioObjectID from its UID.
fn device_id(uid: &str) -> AudioObjectID {
    let uid = CFString::from_str(uid);
    let qualifier: *const CFString = &*uid;
    let mut id: AudioObjectID = 0;
    let mut size = std::mem::size_of::<AudioObjectID>() as u32;
    // SAFETY: the qualifier is a CFStringRef; `id` has room for `size`.
    let status = unsafe {
        AudioObjectGetPropertyData(
            kAudioObjectSystemObject as AudioObjectID,
            NonNull::from(&mut address(kAudioHardwarePropertyTranslateUIDToDevice)),
            std::mem::size_of::<*const CFString>() as u32,
            NonNull::from(&qualifier).cast::<c_void>().as_ptr(),
            NonNull::from(&mut size),
            NonNull::from(&mut id).cast(),
        )
    };
    assert!(status == 0 && id != 0, "no device {uid}");
    id
}

fn default_device_name() -> String {
    let list = devices();
    let id = list.default_output.unwrap();
    list.outputs.into_iter().find(|d| d.id == id).unwrap().name
}

#[test]
#[ignore = "needs BlackHole 2ch as the default devices: app/tool/macos/check_devices.sh"]
fn a_chosen_headset_pulled_out_falls_back_to_the_default_and_keeps_sending() {
    let _tone = Tone::play();
    let headset = Headset::plug("Opencord test headset");
    let chosen = DeviceChoice::Id(headset.cpal_id());
    let call = Call::start(chosen.clone(), chosen);
    std::thread::sleep(Duration::from_millis(500));
    assert_sending(call.frames_over(Duration::from_secs(1)), "on the headset");
    assert!(
        call.device_events().is_empty(),
        "{:?}",
        call.device_events()
    );

    let pulled = Instant::now();
    drop(headset);
    let events = call.wait_for_device_events(2, Duration::from_secs(3));
    let fallback = default_device_name();
    for output in [false, true] {
        let (at, _) = events
            .iter()
            .find(|(_, event)| {
                *event
                    == EngineEvent::DeviceFellBack {
                        output,
                        device: fallback.clone(),
                    }
            })
            .unwrap_or_else(|| panic!("no fallback (output {output}) to {fallback}: {events:?}"));
        println!(
            "{} fell back to {fallback} {} ms after the headset was pulled out",
            if output { "speaker" } else { "microphone" },
            at.duration_since(pulled).as_millis()
        );
    }
    assert_eq!(events.len(), 2, "{events:?}");
    let frames = call.frames_over(Duration::from_secs(1));
    println!("{frames} frames in the second after the fallback");
    assert_sending(frames, "after the fallback");
    drop(call.engine);
}

#[test]
#[ignore = "needs BlackHole 2ch as the default devices: app/tool/macos/check_devices.sh"]
fn a_headset_whose_rate_changes_is_reopened_not_replaced() {
    let _tone = Tone::play();
    let blackhole = device_id(BLACKHOLE);
    set(
        blackhole,
        kAudioDevicePropertyNominalSampleRate,
        48_000.0f64,
    );
    let headset = Headset::plug("Opencord test headset");
    let chosen = DeviceChoice::Id(headset.cpal_id());
    let call = Call::start(chosen.clone(), chosen);
    std::thread::sleep(Duration::from_millis(500));
    assert_sending(call.frames_over(Duration::from_secs(1)), "at 48 kHz");

    // AirPods' headset profile runs at 16 kHz (BlackHole has no 24); another
    // app can ask for 44.1.
    for rate in [16_000.0f64, 44_100.0] {
        set(blackhole, kAudioDevicePropertyNominalSampleRate, rate);
        let timeline = call.timeline(Duration::from_secs(3));
        let now: f64 = get(blackhole, kAudioDevicePropertyNominalSampleRate);
        println!(
            "{rate} Hz: silent for {} ms; the device is at {now} Hz after; frames per 100 ms: {timeline:?}",
            gap(&timeline).as_millis()
        );
        assert!(gap(&timeline) <= REOPENED_WITHIN, "{timeline:?}");
        assert_sending(timeline[20..].iter().sum(), &format!("after {rate} Hz"));
    }
    assert!(
        call.device_events().is_empty(),
        "{:?}",
        call.device_events()
    );
    drop(call.engine);
}

#[test]
#[ignore = "needs BlackHole 2ch as the default devices: app/tool/macos/check_devices.sh"]
fn the_default_headset_pulled_out_moves_to_the_new_default_silently() {
    let _tone = Tone::play();
    let headset = Headset::plug("Opencord test headset");
    headset.make_default();
    let call = Call::start(DeviceChoice::Default, DeviceChoice::Default);
    std::thread::sleep(Duration::from_millis(500));
    assert_sending(call.frames_over(Duration::from_secs(1)), "on the headset");

    drop(headset);
    let timeline = call.timeline(Duration::from_secs(3));
    println!(
        "silent for {} ms after the default headset was pulled out (now {}); frames per 100 ms: {timeline:?}",
        gap(&timeline).as_millis(),
        default_device_name()
    );
    assert!(gap(&timeline) <= REOPENED_WITHIN, "{timeline:?}");
    assert_sending(timeline[20..].iter().sum(), "after the headset went");
    // Following the default is no fallback: nothing to tell the user.
    assert!(
        call.device_events().is_empty(),
        "{:?}",
        call.device_events()
    );
    drop(call.engine);
}

/// A real headset (`OPENCORD_TEST_HEADSET`, its UID without `:output` or
/// `:input`, e.g. AirPods' `74-65-0C-EE-4E-66`): its speaker is opened,
/// then its microphone, which moves Bluetooth headsets to their headset
/// profile; the speaker must still be playing after.
#[test]
#[ignore = "needs a Bluetooth headset: OPENCORD_TEST_HEADSET=<uid>"]
fn a_real_headset_keeps_playing_when_its_microphone_opens() {
    use opencord_media::audio::device::{Microphone, Speaker};
    let Ok(uid) = std::env::var("OPENCORD_TEST_HEADSET") else {
        println!("skipped: no OPENCORD_TEST_HEADSET");
        return;
    };
    let output_uid = format!("{uid}:output");
    let output = device_id(&output_uid);
    let running = || {
        get::<u32>(
            output, 0x676f6e65, /* kAudioDevicePropertyDeviceIsRunningSomewhere */
        )
    };
    let rate = || get::<f64>(output, kAudioDevicePropertyNominalSampleRate);

    let speaker = Speaker::open(Some(&format!("coreaudio:{output_uid}"))).unwrap();
    std::thread::sleep(Duration::from_millis(500));
    println!(
        "speaker open at {} Hz × {}: device at {} Hz, running {}",
        speaker.rate,
        speaker.channels,
        rate(),
        running()
    );
    let microphone = Microphone::open(Some(&format!("coreaudio:{uid}:input"))).unwrap();
    println!(
        "microphone open at {} Hz × {}",
        microphone.rate, microphone.channels
    );
    for tenth in 0..30 {
        std::thread::sleep(Duration::from_millis(100));
        if tenth % 5 == 4 {
            println!(
                "{} ms: speaker lost {}, device at {} Hz, running {}",
                (tenth + 1) * 100,
                speaker.health.is_lost(),
                rate(),
                running()
            );
        }
    }
    // Lost when the profile changes; reopened as the engine does.
    if speaker.health.is_lost() {
        drop(speaker);
        let again = Speaker::open(Some(&format!("coreaudio:{output_uid}"))).unwrap();
        std::thread::sleep(Duration::from_millis(1000));
        println!(
            "reopened at {} Hz × {}: device at {} Hz, running {}, lost {}",
            again.rate,
            again.channels,
            rate(),
            running(),
            again.health.is_lost()
        );
        assert!(running() == 1 && !again.health.is_lost());
    } else {
        assert_eq!(running(), 1);
    }
    drop(microphone);
}
