//! Test audio: a recorded voice and recorded noises (sources and licences
//! in `tests/fixtures/README.md`), and a way to set levels.

use super::convert::FromDevice;
use super::{TICK, dbfs};

/// A 16-bit WAV file from `tests/fixtures`, at 48 kHz mono.
fn wav(name: &str) -> Vec<f32> {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{path}: {error}"));
    let chunk = |id: &[u8]| bytes.windows(4).position(|w| w == id).unwrap() + 8;
    let format = chunk(b"fmt ");
    let channels = u16::from_le_bytes([bytes[format + 2], bytes[format + 3]]);
    let rate = u32::from_le_bytes(bytes[format + 4..format + 8].try_into().unwrap());
    let samples: Vec<f32> = bytes[chunk(b"data")..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| f32::from(i16::from_le_bytes(*pair)) / 32_768.0)
        .collect();
    let mut convert = FromDevice::new(rate, channels).unwrap();
    convert.push(&samples);
    let mut out = Vec::new();
    let mut tick = [0.0; TICK];
    while convert.pop_tick(&mut tick) {
        out.extend_from_slice(&tick);
    }
    out
}

/// Three and a half seconds of someone reading aloud.
pub fn speech() -> Vec<f32> {
    wav("speech.wav")
}

/// `samples` of a recorded noise the plan names: "keyboard", "fan",
/// "street" or "dog".
pub fn noise(kind: &str, samples: usize) -> Vec<f32> {
    wav(&format!("noise/{kind}.wav"))
        .into_iter()
        .cycle()
        .take(samples)
        .collect()
}

/// `samples` turned up or down to `level` dBFS.
pub fn at_level(samples: &[f32], level: f32) -> Vec<f32> {
    let gain = 10f32.powf((level - dbfs(samples)) / 20.0);
    samples.iter().map(|s| s * gain).collect()
}

/// The voice at -22 dBFS with `kind` of noise at -35 dBFS under it;
/// returns the mix and the voice alone.
pub fn voice_with(kind: &str) -> (Vec<f32>, Vec<f32>) {
    let voice = at_level(&speech(), -22.0);
    let noise = at_level(&noise(kind, voice.len()), -35.0);
    let mix = voice.iter().zip(&noise).map(|(v, n)| v + n).collect();
    (mix, voice)
}
