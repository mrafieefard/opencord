//! Synthesizes the default soundboard sounds (Phase 2 plan §11.8) into
//! `crates/opencord-media/sounds/`. They are made here from oscillators and
//! seeded noise, so they are ours to release under CC0, and running this
//! again writes the same files.
//!
//! cargo run -p opencord-media --example default_sounds

use std::error::Error;
use std::f32::consts::TAU;
use std::fs;
use std::path::Path;

use opencord_common::sound::defaults::DEFAULT_SOUNDS;
use opencord_common::sound::ogg::OpusFile;
use opencord_common::sound::within_duration;
use opencord_media::soundboard::{Pcm, encode, normalizing_gain};

const RATE: f32 = 48_000.0;

fn main() -> Result<(), Box<dyn Error>> {
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("sounds");
    fs::create_dir_all(&out)?;
    for sound in DEFAULT_SOUNDS {
        let pcm = Pcm {
            channels: 1,
            samples: synthesize(sound.id),
        };
        let file = encode(&pcm.scaled(normalizing_gain(&pcm)?))?;
        let info = OpusFile::read(&file)?.info;
        assert!(within_duration(&info), "{} is too long", sound.id);
        fs::write(out.join(format!("{}.opus", sound.id)), &file)?;
        println!(
            "{:<13} {:>5} ms {:>6} bytes",
            sound.id,
            info.duration_ms,
            file.len()
        );
    }
    Ok(())
}

fn synthesize(id: &str) -> Vec<f32> {
    match id {
        "chime" => chime(),
        "tada" => tada(),
        "air-horn" => air_horn(),
        "rimshot" => rimshot(),
        "applause" => applause(),
        "sad-trombone" => sad_trombone(),
        "boing" => boing(),
        "crickets" => crickets(),
        other => panic!("no recipe for {other}"),
    }
}

fn silence(seconds: f32) -> Vec<f32> {
    vec![0.0; (seconds * RATE) as usize]
}

/// Adds `voice(t)` (t in seconds from its start) from `start` for
/// `length` seconds.
fn add(out: &mut [f32], start: f32, length: f32, mut voice: impl FnMut(f32) -> f32) {
    let first = (start * RATE) as usize;
    let count = (length * RATE) as usize;
    for (n, sample) in out.iter_mut().skip(first).take(count).enumerate() {
        *sample += voice(n as f32 / RATE);
    }
}

/// A rise over `attack` seconds, then exponential decay with time constant
/// `decay`.
fn strike(t: f32, attack: f32, decay: f32) -> f32 {
    (t / attack).min(1.0) * (-(t - attack).max(0.0) / decay).exp()
}

/// Rises over `attack`, holds, and falls over `release` before `length`.
fn hold(t: f32, length: f32, attack: f32, release: f32) -> f32 {
    (t / attack).min(1.0) * ((length - t) / release).clamp(0.0, 1.0)
}

/// A sawtooth from its harmonics below 12 kHz, each `1 / h^tilt`.
fn saw(phase: f32, frequency: f32, tilt: f32) -> f32 {
    let harmonics = (12_000.0 / frequency) as u32;
    (1..=harmonics.max(1))
        .map(|h| (phase * h as f32).sin() / (h as f32).powf(tilt))
        .sum::<f32>()
        * 0.5
}

/// Seeded white noise in [-1, 1].
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f32 / u32::MAX as f32 * 2.0 - 1.0
    }
}

/// A one-pole filter's coefficient for `cutoff` Hz.
fn pole(cutoff: f32) -> f32 {
    (-TAU * cutoff / RATE).exp()
}

fn bell(frequency: f32) -> impl FnMut(f32) -> f32 {
    const PARTIALS: [(f32, f32); 5] = [
        (1.0, 1.0),
        (2.0, 0.4),
        (2.76, 0.5),
        (5.4, 0.25),
        (8.93, 0.12),
    ];
    move |t| {
        PARTIALS
            .iter()
            .enumerate()
            .map(|(k, (ratio, amplitude))| {
                let decay = 0.9 / (1.0 + k as f32 * 0.6);
                amplitude * (TAU * frequency * ratio * t).sin() * strike(t, 0.003, decay)
            })
            .sum()
    }
}

/// A two-note doorbell.
fn chime() -> Vec<f32> {
    let mut out = silence(1.4);
    add(&mut out, 0.0, 1.4, bell(659.25));
    add(&mut out, 0.5, 0.9, bell(523.25));
    out
}

fn brass(frequency: f32, length: f32, vibrato: f32) -> impl FnMut(f32) -> f32 {
    let mut phase = 0.0f32;
    move |t| {
        let wobble = 1.0 + vibrato * (TAU * 5.5 * t).sin() * (t / 0.3).min(1.0);
        phase += TAU * frequency * wobble / RATE;
        saw(phase, frequency, 1.4) * hold(t, length, 0.02, 0.4)
    }
}

/// Three rising notes and a held major chord.
fn tada() -> Vec<f32> {
    let mut out = silence(1.6);
    for (index, frequency) in [523.25, 659.25, 783.99].into_iter().enumerate() {
        add(
            &mut out,
            index as f32 * 0.13,
            0.14,
            brass(frequency, 0.14, 0.0),
        );
    }
    for frequency in [523.25, 659.25, 783.99, 1_046.5] {
        add(&mut out, 0.42, 1.18, brass(frequency, 1.18, 0.006));
    }
    out
}

/// Two dissonant sawtooths in three blasts, the last one long.
fn air_horn() -> Vec<f32> {
    let mut out = silence(1.75);
    for (start, length) in [(0.0, 0.18), (0.24, 0.18), (0.48, 1.27)] {
        let mut phases = [0.0f32; 2];
        add(&mut out, start, length, move |t| {
            let bend = 0.92 + 0.08 * (t / 0.04).min(1.0);
            let mut sum = 0.0;
            for (phase, frequency) in phases.iter_mut().zip([415.3, 466.2]) {
                *phase += TAU * frequency * bend / RATE;
                sum += saw(*phase, frequency, 1.0);
            }
            (sum * 0.8).tanh() * hold(t, length, 0.01, 0.03)
        });
    }
    out
}

/// A drum whose pitch drops, with a click on the hit.
fn tom(high: f32, low: f32, seed: u32) -> impl FnMut(f32) -> f32 {
    let mut phase = 0.0f32;
    let mut noise = Noise(seed);
    move |t| {
        let frequency = low + (high - low) * (-t / 0.05).exp();
        phase += TAU * frequency / RATE;
        phase.sin() * strike(t, 0.002, 0.15) + noise.next() * 0.3 * strike(t, 0.001, 0.008)
    }
}

/// Two drum hits and a cymbal.
fn rimshot() -> Vec<f32> {
    let mut out = silence(1.6);
    add(&mut out, 0.0, 0.5, tom(200.0, 100.0, 7));
    add(&mut out, 0.22, 0.5, tom(160.0, 80.0, 11));
    let mut noise = Noise(23);
    let (mut low, mut previous) = (0.0f32, 0.0f32);
    add(&mut out, 0.48, 1.12, move |t| {
        // Twice-filtered noise keeps what is above about 6 kHz.
        let white = noise.next();
        low = low * pole(6_000.0) + white * (1.0 - pole(6_000.0));
        let high = white - low;
        let sizzle = high - previous * 0.3;
        previous = high;
        let metal: f32 = [3_200.0, 4_700.0, 6_100.0, 7_300.0]
            .iter()
            .map(|frequency| (TAU * frequency * t).sin())
            .sum();
        (sizzle * 0.9 + metal * 0.08) * strike(t, 0.002, 0.35)
    });
    out
}

/// Many short noise bursts: a crowd clapping, swelling and dying away.
fn applause() -> Vec<f32> {
    let mut out = silence(3.0);
    let mut chance = Noise(99);
    let step = 0.001f32;
    let mut t = 0.0f32;
    let mut seed = 1u32;
    while t < 2.95 {
        let crowd = (t / 0.3).min(1.0) * ((3.0 - t) / 1.0).clamp(0.0, 1.0);
        if (chance.next() + 1.0) / 2.0 < 45.0 * crowd * step {
            let loudness = 0.3 + 0.7 * (chance.next() + 1.0) / 2.0;
            let mut noise = Noise(seed.wrapping_mul(2_654_435_761) | 1);
            let (mut low, mut band) = (0.0f32, 0.0f32);
            add(&mut out, t, 0.04, move |time| {
                let white = noise.next();
                low = low * pole(800.0) + white * (1.0 - pole(800.0));
                band = band * pole(4_000.0) + (white - low) * (1.0 - pole(4_000.0));
                band * loudness * strike(time, 0.0005, 0.004)
            });
            seed += 1;
        }
        t += step;
    }
    out
}

/// Four falling notes, the last one wobbling.
fn sad_trombone() -> Vec<f32> {
    let mut out = silence(2.8);
    let notes = [
        (293.66, 0.0, 0.45),
        (277.18, 0.5, 0.45),
        (261.63, 1.0, 0.45),
        (246.94, 1.5, 1.3),
    ];
    for (index, (frequency, start, length)) in notes.into_iter().enumerate() {
        let last = index == notes.len() - 1;
        let mut phase = 0.0f32;
        add(&mut out, start, length, move |t| {
            let wah = if last {
                (TAU * 6.0 * t).sin() * (t / 0.6).min(1.0)
            } else {
                0.0
            };
            phase += TAU * frequency * (1.0 + 0.03 * wah) / RATE;
            let brightness = 1.8 - 0.4 * wah;
            saw(phase, frequency, brightness) * hold(t, length, 0.04, 0.06)
        });
    }
    out
}

/// A spring: the pitch jumps up and wobbles as it fades.
fn boing() -> Vec<f32> {
    let mut out = silence(0.9);
    let mut phase = 0.0f32;
    add(&mut out, 0.0, 0.9, move |t| {
        let rise = 1.0 - (-t / 0.05).exp();
        let frequency =
            180.0 + 260.0 * rise * (1.0 + 0.25 * (TAU * 7.0 * t).sin() * (-3.0 * t).exp());
        phase += TAU * frequency / RATE;
        (phase.sin() + 0.2 * (2.0 * phase).sin()) * strike(t, 0.005, 0.35)
    });
    out
}

/// Two crickets chirping in threes.
fn crickets() -> Vec<f32> {
    let mut out = silence(2.5);
    let mut jitter = Noise(5);
    for (frequency, offset) in [(4_600.0, 0.0), (5_100.0, 0.21)] {
        let mut start = offset;
        while start < 2.3 {
            for pulse in 0..3 {
                add(&mut out, start + pulse as f32 * 0.05, 0.03, move |t| {
                    (TAU * frequency * t).sin() * (std::f32::consts::PI * t / 0.03).sin()
                });
            }
            start += 0.45 + 0.05 * jitter.next();
        }
    }
    out
}
