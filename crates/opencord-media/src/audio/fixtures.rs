//! Test audio: a recorded voice, and a way to set levels.

use super::convert::FromDevice;
use super::{TICK, dbfs};

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
