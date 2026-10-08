//! Between the devices and processing: any sample rate and channel count on
//! the device side, 48 kHz mono in 10 ms ticks on ours (plan §7.2, §7.5).
//! Rates other than 48 kHz go through rubato's FFT resampler.

use std::collections::VecDeque;

use audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};

use super::{SAMPLE_RATE, TICK};

#[derive(Debug, thiserror::Error)]
pub enum ConvertError {
    #[error("a device with no channels")]
    NoChannels,
    #[error("cannot resample between {from} Hz and {to} Hz: {reason}")]
    Rate { from: u32, to: u32, reason: String },
}

/// Mono audio from one rate to another, fed any amount at a time.
struct Resample {
    resampler: Fft<f32>,
    chunk: usize,
    /// Input waiting for a whole chunk.
    pending: Vec<f32>,
    output: Vec<f32>,
}

impl Resample {
    fn new(from: u32, to: u32) -> Result<Self, ConvertError> {
        let error = |reason: String| ConvertError::Rate { from, to, reason };
        // 10 ms at a time.
        let chunk = (from / 100).max(1) as usize;
        let resampler = Fft::<f32>::new(from as usize, to as usize, chunk, 1, FixedSync::Input)
            .map_err(|problem| error(problem.to_string()))?;
        let output = vec![0.0; resampler.output_frames_max()];
        Ok(Self {
            resampler,
            chunk,
            pending: Vec::with_capacity(chunk),
            output,
        })
    }

    /// Resamples `samples`, appending what comes out to `out`.
    fn process(&mut self, samples: &[f32], out: &mut VecDeque<f32>) {
        for sample in samples {
            self.pending.push(*sample);
            if self.pending.len() < self.chunk {
                continue;
            }
            let frames_out = self.output.len();
            let input = InterleavedSlice::new(&self.pending[..], 1, self.chunk);
            let output = InterleavedSlice::new_mut(&mut self.output[..], 1, frames_out);
            if let (Ok(input), Ok(mut output)) = (input, output)
                && let Ok((_, produced)) =
                    self.resampler
                        .process_into_buffer(&input, &mut output, None)
            {
                out.extend(&self.output[..produced]);
            }
            self.pending.clear();
        }
    }
}

/// Microphone audio, at the device's rate and channels, into ticks.
pub struct FromDevice {
    channels: usize,
    resample: Option<Resample>,
    mono: Vec<f32>,
    ready: VecDeque<f32>,
}

impl FromDevice {
    pub fn new(rate: u32, channels: u16) -> Result<Self, ConvertError> {
        if channels == 0 {
            return Err(ConvertError::NoChannels);
        }
        let resample = (rate != SAMPLE_RATE)
            .then(|| Resample::new(rate, SAMPLE_RATE))
            .transpose()?;
        Ok(Self {
            channels: usize::from(channels),
            resample,
            mono: Vec::with_capacity(4 * TICK),
            ready: VecDeque::with_capacity(8 * TICK),
        })
    }

    /// Takes interleaved device samples.
    pub fn push(&mut self, interleaved: &[f32]) {
        let channels = self.channels as f32;
        self.mono.clear();
        self.mono.extend(
            interleaved
                .chunks_exact(self.channels)
                .map(|frame| frame.iter().sum::<f32>() / channels),
        );
        match &mut self.resample {
            Some(resample) => resample.process(&self.mono, &mut self.ready),
            None => self.ready.extend(&self.mono),
        }
    }

    /// The next tick, if a whole one is there.
    pub fn pop_tick(&mut self, out: &mut [f32]) -> bool {
        if self.ready.len() < TICK {
            return false;
        }
        for (slot, sample) in out.iter_mut().zip(self.ready.drain(..TICK)) {
            *slot = sample;
        }
        true
    }

    /// Samples at 48 kHz waiting to make up ticks.
    pub fn buffered(&self) -> usize {
        self.ready.len()
    }

    /// Forgets what is waiting, after the microphone fell behind or ran ahead.
    pub fn clear(&mut self) {
        self.ready.clear();
    }
}

/// Ticks, to the device's rate and channels.
pub struct ToDevice {
    channels: usize,
    resample: Option<Resample>,
    /// Mono at the device's rate.
    ready: VecDeque<f32>,
}

impl ToDevice {
    pub fn new(rate: u32, channels: u16) -> Result<Self, ConvertError> {
        if channels == 0 {
            return Err(ConvertError::NoChannels);
        }
        let resample = (rate != SAMPLE_RATE)
            .then(|| Resample::new(SAMPLE_RATE, rate))
            .transpose()?;
        Ok(Self {
            channels: usize::from(channels),
            resample,
            ready: VecDeque::with_capacity(8 * TICK),
        })
    }

    pub fn push_tick(&mut self, tick: &[f32]) {
        match &mut self.resample {
            Some(resample) => resample.process(tick, &mut self.ready),
            None => self.ready.extend(tick),
        }
    }

    /// Fills interleaved device samples with what is ready; returns the
    /// frames written.
    pub fn pop(&mut self, interleaved: &mut [f32]) -> usize {
        let frames = (interleaved.len() / self.channels).min(self.ready.len());
        for (frame, sample) in interleaved
            .chunks_exact_mut(self.channels)
            .zip(self.ready.drain(..frames))
        {
            frame.fill(sample);
        }
        frames
    }

    /// Frames waiting for the device.
    pub fn buffered(&self) -> usize {
        self.ready.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{SAMPLE_RATE, TICK};

    fn sine(rate: u32, frequency: f32, frames: usize, channels: usize) -> Vec<f32> {
        (0..frames)
            .flat_map(|i| {
                let value =
                    0.5 * (std::f32::consts::TAU * frequency * i as f32 / rate as f32).sin();
                std::iter::repeat_n(value, channels)
            })
            .collect()
    }

    /// Upward zero crossings per second.
    fn frequency(samples: &[f32], rate: u32) -> f32 {
        let crossings = samples
            .windows(2)
            .filter(|pair| pair[0] < 0.0 && pair[1] >= 0.0)
            .count();
        crossings as f32 * rate as f32 / samples.len() as f32
    }

    fn ticks_from(converter: &mut FromDevice) -> Vec<f32> {
        let mut out = Vec::new();
        let mut tick = [0.0; TICK];
        while converter.pop_tick(&mut tick) {
            out.extend_from_slice(&tick);
        }
        out
    }

    #[test]
    fn a_48_khz_stereo_microphone_is_mixed_to_mono() {
        let mut converter = FromDevice::new(SAMPLE_RATE, 2).unwrap();
        let mut stereo = Vec::new();
        for _ in 0..TICK {
            stereo.extend([0.4, 0.2]);
        }

        converter.push(&stereo);
        let mono = ticks_from(&mut converter);

        assert_eq!(mono.len(), TICK);
        assert!(mono.iter().all(|s| (s - 0.3).abs() < 1e-6));
    }

    #[test]
    fn a_44_1_khz_microphone_comes_out_at_48_khz() {
        let mut converter = FromDevice::new(44_100, 1).unwrap();
        let second = sine(44_100, 1_000.0, 44_100, 1);

        for chunk in second.chunks(512) {
            converter.push(chunk);
        }
        let out = ticks_from(&mut converter);

        let expected = SAMPLE_RATE as usize;
        assert!(
            out.len().abs_diff(expected) < 2 * TICK,
            "{} samples",
            out.len()
        );
        let steady = &out[out.len() / 4..];
        let heard = frequency(steady, SAMPLE_RATE);
        assert!((heard - 1_000.0).abs() < 15.0, "{heard} Hz");
    }

    #[test]
    fn ticks_go_to_every_speaker_channel() {
        let mut converter = ToDevice::new(SAMPLE_RATE, 2).unwrap();
        let tick = [0.25; TICK];

        converter.push_tick(&tick);
        let mut interleaved = vec![0.0; 2 * TICK];
        let frames = converter.pop(&mut interleaved);

        assert_eq!(frames, TICK);
        assert!(interleaved.iter().all(|s| *s == 0.25));
    }

    #[test]
    fn ticks_play_on_a_44_1_khz_speaker() {
        let mut converter = ToDevice::new(44_100, 2).unwrap();
        let one_second = sine(SAMPLE_RATE, 1_000.0, SAMPLE_RATE as usize, 1);

        let mut played = Vec::new();
        for tick in one_second.chunks(TICK) {
            converter.push_tick(tick);
            let mut interleaved = vec![0.0; 2 * 1024];
            let frames = converter.pop(&mut interleaved);
            played.extend(interleaved[..2 * frames].iter().step_by(2));
        }

        assert!(
            played.len().abs_diff(44_100) < 2 * 441,
            "{} frames",
            played.len()
        );
        let heard = frequency(&played[played.len() / 4..], 44_100);
        assert!((heard - 1_000.0).abs() < 15.0, "{heard} Hz");
    }

    #[test]
    fn what_the_speaker_cannot_take_yet_waits() {
        let mut converter = ToDevice::new(SAMPLE_RATE, 1).unwrap();
        converter.push_tick(&[0.1; TICK]);

        let mut small = [0.0; 100];
        let first = converter.pop(&mut small);
        let mut rest = [0.0; TICK];
        let second = converter.pop(&mut rest);

        assert_eq!(first, 100);
        assert_eq!(second, TICK - 100);
        assert_eq!(converter.buffered(), 0);
    }
}
