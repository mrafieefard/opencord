//! What WASAPI offers on Windows, and what a microphone the system has
//! denied us looks like (Windows plan W1, steps 2 and 3).
//!
//! The enumeration test runs anywhere and asserts only what holds when the
//! machine has devices, so a CI runner without any does not fail it. The live
//! test opens the real default microphone, so it is ignored by default:
//!
//!   cargo test -p opencord-media --test devices_windows -- --ignored --nocapture
//!
//! That live test is also the instrument for step 2. Run it with
//! Settings → Privacy & security → Microphone → "Let desktop apps access your
//! microphone" off, and what it prints says which of the two things Windows
//! does: refuse to open the stream, or open one that only ever yields silence.

#![cfg(windows)]

use std::thread::sleep;
use std::time::{Duration, Instant};

use opencord_media::audio::device::{DeviceList, Microphone, devices};

/// Long enough for a WASAPI capture client to have filled its buffer several
/// times over at any sane period.
const LISTEN: Duration = Duration::from_millis(1500);

#[test]
fn windows_names_its_microphones_and_speakers() {
    let list = devices();
    report(&list);

    if list.inputs.is_empty() && list.outputs.is_empty() {
        // A runner with no audio device at all: nothing to check.
        return;
    }

    for (kind, found) in [("microphone", &list.inputs), ("speaker", &list.outputs)] {
        for device in found {
            assert!(!device.id.is_empty(), "a {kind} with no id");
            assert!(
                !device.name.is_empty(),
                "a {kind} with no name: {:?}",
                device.id
            );
        }
        let mut ids: Vec<&str> = found.iter().map(|device| device.id.as_str()).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(before, ids.len(), "two {kind}s share an id");
    }

    // Settings store the id, so a default that names a device not in the list
    // would be a setting the app could never match again.
    for (kind, default, found) in [
        ("microphone", &list.default_input, &list.inputs),
        ("speaker", &list.default_output, &list.outputs),
    ] {
        if let Some(id) = default {
            assert!(
                found.iter().any(|device| &device.id == id),
                "the default {kind} {id:?} is not in the {kind} list"
            );
        }
    }
}

#[test]
#[ignore = "opens the real default microphone; needs a device and the system's consent"]
fn the_default_microphone_opens_and_delivers_samples() {
    // OPENCORD_TEST_MICROPHONE picks another one by a piece of its name, so a
    // check can show which inputs are quiet enough to stand in for Linux's
    // silent null sink:
    //
    //   $env:OPENCORD_TEST_MICROPHONE = 'CABLE Output'
    let wanted = std::env::var("OPENCORD_TEST_MICROPHONE").ok();
    let chosen = wanted.as_deref().map(|name| {
        let list = devices();
        let found = list
            .inputs
            .iter()
            .find(|device| device.name.contains(name))
            .unwrap_or_else(|| {
                panic!(
                    "no microphone's name contains {name:?}; there are {:?}",
                    list.inputs
                        .iter()
                        .map(|device| device.name.as_str())
                        .collect::<Vec<_>>()
                )
            });
        found.id.clone()
    });

    let mut microphone = match Microphone::open(chosen.as_deref()) {
        Ok(microphone) => microphone,
        Err(error) => {
            // Step 2, the first of the two outcomes: Windows refused.
            panic!("the microphone did not open: {error}");
        }
    };
    println!(
        "opened {:?} at {} Hz, {} channels",
        microphone.name, microphone.rate, microphone.channels
    );

    let started = Instant::now();
    let mut frames = 0u64;
    let mut peak = 0f32;
    while started.elapsed() < LISTEN {
        while let Ok(sample) = microphone.samples.pop() {
            frames += 1;
            peak = peak.max(sample.abs());
        }
        sleep(Duration::from_millis(10));
    }

    let dbfs = if peak > 0.0 {
        20.0 * peak.log10()
    } else {
        f32::NEG_INFINITY
    };
    println!(
        "{frames} samples in {:?}, peak {peak:.6} ({dbfs:.1} dBFS), \
         glitches {}, lost {}",
        LISTEN,
        microphone.health.glitches(),
        microphone.health.is_lost()
    );

    assert!(!microphone.health.is_lost(), "the microphone was lost");
    assert!(frames > 0, "the microphone delivered no samples at all");
    if chosen.is_none() {
        // Step 2, the second outcome: a stream that opens and stays at exactly
        // digital zero is a microphone the system is holding back, not a quiet
        // room — a real one always carries some noise. Only the default
        // microphone is held to this: a cable with nothing playing into it is
        // legitimately silent, and probing for that is the point of
        // OPENCORD_TEST_MICROPHONE.
        assert!(
            peak > 0.0,
            "the microphone opened but every one of {frames} samples was \
             silent; this is what a denied microphone looks like"
        );
    }
}

fn report(list: &DeviceList) {
    println!("default microphone: {:?}", list.default_input);
    println!("default speaker   : {:?}", list.default_output);
    for (kind, found) in [("microphone", &list.inputs), ("speaker", &list.outputs)] {
        println!("{} {kind}s:", found.len());
        for device in found {
            println!("  {:?}  {}", device.id, device.name);
        }
    }
}
