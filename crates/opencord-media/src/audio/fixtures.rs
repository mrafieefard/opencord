//! Test audio: a recorded voice, noises, and a way to set levels.

use super::convert::FromDevice;
use super::{SAMPLE_RATE, TICK, dbfs};

/// Three and a half seconds of a (synthesized) voice, at 48 kHz.
pub fn speech() -> Vec<f32> {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/speech.wav"
    ))
    .unwrap();
    let data = bytes.windows(4).position(|w| w == b"data").unwrap() + 8;
    let samples: Vec<f32> = bytes[data..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| f32::from(i16::from_le_bytes(*pair)) / 32_768.0)
        .collect();
    let mut convert = FromDevice::new(22_050, 1).unwrap();
    convert.push(&samples);
    let mut out = Vec::new();
    let mut tick = [0.0; TICK];
    while convert.pop_tick(&mut tick) {
        out.extend_from_slice(&tick);
    }
    out
}

/// `samples` turned up or down to `level` dBFS.
pub fn at_level(samples: &[f32], level: f32) -> Vec<f32> {
    let gain = 10f32.powf((level - dbfs(samples)) / 20.0);
    samples.iter().map(|s| s * gain).collect()
}

struct Noise(u64);

impl Noise {
    fn white(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        ((self.0 >> 11) as f64 / (1u64 << 53) as f64) as f32 * 2.0 - 1.0
    }
}

/// The noises the plan names, made up from a fixed seed: a fan's hum and
/// hiss, keyboard clicks, street rumble, and barking.
pub fn noise(kind: &str, samples: usize) -> Vec<f32> {
    let mut rng = Noise(0x0dd5);
    let mut low = 0.0f32;
    (0..samples)
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            let white = rng.white();
            low = 0.97 * low + 0.03 * white;
            match kind {
                "fan" => {
                    0.6 * low + 0.05 * white + 0.05 * (std::f32::consts::TAU * 120.0 * t).sin()
                }
                "keyboard" => {
                    let in_click = (t * 7.0).fract() < 0.004;
                    if in_click { 0.4 * white } else { 0.002 * white }
                }
                "street" => 0.9 * low + 0.02 * white,
                "dog" => {
                    let bark = (t * 1.5).fract();
                    let envelope = if bark < 0.15 {
                        (std::f32::consts::PI * bark / 0.15).sin()
                    } else {
                        0.0
                    };
                    envelope * (0.3 * (std::f32::consts::TAU * 550.0 * t).sin() + 0.1 * white)
                }
                _ => unreachable!(),
            }
        })
        .collect()
}

/// The voice at -22 dBFS over `kind` of noise at -35 dBFS, after a second of
/// the noise alone; returns the mix and where the voice starts.
pub fn voice_over(kind: &str) -> (Vec<f32>, usize) {
    let voice = at_level(&speech(), -22.0);
    let lead = SAMPLE_RATE as usize;
    let mut mix = at_level(&noise(kind, lead + voice.len()), -35.0);
    for (i, sample) in voice.iter().enumerate() {
        mix[lead + i] += sample;
    }
    (mix, lead)
}
