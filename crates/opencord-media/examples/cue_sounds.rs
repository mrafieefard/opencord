//! Synthesizes the voice event sounds (Phase 2 §12) into
//! `crates/opencord-media/sounds/cues/`. Soft sine notes with a short
//! attack and an exponential tail: rising for joining, unmuting and going
//! live, falling for the reverse. Ours to release under CC0, and running
//! this again writes the same files.
//!
//! cargo run -p opencord-media --example cue_sounds

use std::error::Error;
use std::f32::consts::TAU;
use std::fs;
use std::path::Path;

use opencord_common::sound::ogg::OpusFile;
use opencord_media::soundboard::cues::Cue;
use opencord_media::soundboard::{Pcm, encode};

const RATE: f32 = 48_000.0;
/// Every cue's loudest sample, −6 dBFS: quieter than speech.
const PEAK: f32 = 0.5;

/// Notes in Hz.
const E5: f32 = 659.26;
const A5: f32 = 880.0;
const C6: f32 = 1046.5;
const E6: f32 = 1318.5;
const A4: f32 = 440.0;
const C5: f32 = 523.25;
const G5: f32 = 783.99;

fn main() -> Result<(), Box<dyn Error>> {
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("sounds/cues");
    fs::create_dir_all(&out)?;
    for cue in Cue::ALL {
        let pcm = Pcm {
            channels: 1,
            samples: synthesize(cue),
        };
        // Short sounds measure little loudness, so loudness normalization
        // would clip them: they are scaled to one peak instead.
        let peak = pcm.samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()));
        let file = encode(&pcm.scaled(PEAK / peak))?;
        let info = OpusFile::read(&file)?.info;
        fs::write(out.join(format!("{}.opus", cue.id())), &file)?;
        println!(
            "{:<13} {:>4} ms {:>5} bytes",
            cue.id(),
            info.duration_ms,
            file.len()
        );
    }
    Ok(())
}

fn synthesize(cue: Cue) -> Vec<f32> {
    match cue {
        Cue::Join => notes(&[E5, A5], 0.09, 0.30),
        Cue::Leave => notes(&[A5, E5], 0.09, 0.30),
        Cue::UserJoin => notes(&[C5, G5], 0.08, 0.25),
        Cue::UserLeave => notes(&[G5, C5], 0.08, 0.25),
        Cue::Mute => glide(A5, E5, 0.12),
        Cue::Unmute => glide(E5, A5, 0.12),
        Cue::Deafen => notes(&[A5, E5, A4], 0.07, 0.18),
        Cue::Undeafen => notes(&[A4, E5, A5], 0.07, 0.18),
        Cue::StreamStart => notes(&[A5, C6, E6], 0.08, 0.35),
        Cue::StreamStop => notes(&[E6, C6, A5], 0.08, 0.35),
    }
}

/// Each note `step` seconds after the last, ringing for `ring`.
fn notes(frequencies: &[f32], step: f32, ring: f32) -> Vec<f32> {
    let length = step * (frequencies.len() - 1) as f32 + ring;
    let mut out = vec![0.0; (length * RATE) as usize];
    for (n, &frequency) in frequencies.iter().enumerate() {
        let start = ((step * n as f32) * RATE) as usize;
        for (i, sample) in out.iter_mut().skip(start).enumerate() {
            let t = i as f32 / RATE;
            *sample += tone(frequency, t, ring);
        }
    }
    out
}

/// One note sliding from `from` to `to` over `length` seconds.
fn glide(from: f32, to: f32, length: f32) -> Vec<f32> {
    let count = (length * RATE) as usize;
    let mut phase = 0.0f32;
    (0..count)
        .map(|i| {
            let t = i as f32 / RATE;
            let frequency = from + (to - from) * (t / length).min(1.0);
            phase += TAU * frequency / RATE;
            phase.sin() * envelope(t, length)
        })
        .collect()
}

/// A sine with a little of its octave, faded in over 5 ms and out by `ring`.
fn tone(frequency: f32, t: f32, ring: f32) -> f32 {
    let wave = (TAU * frequency * t).sin() + 0.25 * (TAU * 2.0 * frequency * t).sin();
    wave * envelope(t, ring)
}

fn envelope(t: f32, length: f32) -> f32 {
    let attack = (t / 0.005).min(1.0);
    let decay = (-t / (length / 4.0)).exp();
    let tail = ((length - t) / 0.02).clamp(0.0, 1.0);
    attack * decay * tail
}
