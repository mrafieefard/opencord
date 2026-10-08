//! Audio (Phase 2 plan §7.2–§7.7): everything between the devices and the
//! transport. Processing runs at 48 kHz, mono, in 10 ms ticks; Opus frames
//! are 20 ms.

pub mod capture;
pub mod codec;
pub mod convert;
pub mod device;
pub mod engine;
#[cfg(test)]
mod fixtures;
pub mod jitter;
pub mod mixer;
pub mod playback;
pub mod processing;
pub mod processor;

/// The processing sample rate.
pub const SAMPLE_RATE: u32 = 48_000;
/// One processing tick: 10 ms.
pub const TICK: usize = 480;
/// One Opus frame: 20 ms.
pub const FRAME: usize = 960;

/// RMS level in dBFS; -120 for silence.
pub fn dbfs(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return SILENT_DBFS;
    }
    let mean_square = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
    if mean_square <= 0.0 {
        SILENT_DBFS
    } else {
        (10.0 * mean_square.log10()).max(SILENT_DBFS)
    }
}

/// What [`dbfs`] calls silence.
pub const SILENT_DBFS: f32 = -120.0;
