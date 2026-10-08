//! DeepFilterNet's signal side: a short-time Fourier transform with a
//! Vorbis window (20 ms frames every 10 ms), and the two features the
//! network reads, normalized ERB band energies and the normalized low
//! spectrum.
//!
//! Ported from DeepFilterNet's libDF (commit d375b2d, © Hendrik Schröter,
//! MIT OR Apache-2.0; licences in `models/deepfilternet3`). The arithmetic
//! follows libDF step for step, so the network sees what it was trained on.

use std::sync::Arc;

use realfft::num_complex::Complex32;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

use crate::audio::{SAMPLE_RATE, TICK};

/// A frame: two ticks.
pub const FFT_SIZE: usize = 2 * TICK;
/// Bins in a frame's spectrum, 0 to 24 kHz in 50 Hz steps.
pub const FREQS: usize = FFT_SIZE / 2 + 1;
pub const ERB_BANDS: usize = 32;
/// The low bins (up to 4.8 kHz) the deep filter rebuilds.
pub const DF_BINS: usize = 96;
const MIN_FREQS_PER_BAND: usize = 2;
/// How much of the running normalization each frame keeps: a one-second
/// time constant at 100 frames a second, rounded as libDF rounds it.
const NORM_ALPHA: f32 = 0.99;
/// Scales the forward transform so a windowed overlap-add gives the input
/// back.
const WINDOW_NORM: f32 = 2.0 * TICK as f32 / (FFT_SIZE * FFT_SIZE) as f32;
/// Where the running normalizations start (libDF's MEAN_NORM_INIT and
/// UNIT_NORM_INIT).
const ERB_NORM_START: (f32, f32) = (-60.0, -90.0);
const UNIT_NORM_START: (f32, f32) = (0.001, 0.0001);

pub type Spectrum = [Complex32; FREQS];

pub const SILENT_SPECTRUM: Spectrum = [Complex32::new(0.0, 0.0); FREQS];

/// Ticks to spectra and back, one hop at a time.
pub struct Stft {
    forward: Arc<dyn RealToComplex<f32>>,
    inverse: Arc<dyn ComplexToReal<f32>>,
    window: Vec<f32>,
    /// The previous tick, the first half of the next frame.
    previous: [f32; TICK],
    /// The second half of the last frame synthesized, to add to the next.
    overlap: [f32; TICK],
    frame: Vec<f32>,
    forward_scratch: Vec<Complex32>,
    inverse_scratch: Vec<Complex32>,
}

impl Stft {
    pub fn new() -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let forward = planner.plan_fft_forward(FFT_SIZE);
        let inverse = planner.plan_fft_inverse(FFT_SIZE);
        let half = (FFT_SIZE / 2) as f64;
        let window = (0..FFT_SIZE)
            .map(|i| {
                let sin = (0.5 * std::f64::consts::PI * (i as f64 + 0.5) / half).sin();
                (0.5 * std::f64::consts::PI * sin * sin).sin() as f32
            })
            .collect();
        Self {
            forward_scratch: forward.make_scratch_vec(),
            inverse_scratch: inverse.make_scratch_vec(),
            forward,
            inverse,
            window,
            previous: [0.0; TICK],
            overlap: [0.0; TICK],
            frame: vec![0.0; FFT_SIZE],
        }
    }

    /// The spectrum of the frame ending with `tick`.
    pub fn analyze(&mut self, tick: &[f32], spectrum: &mut Spectrum) {
        let (first, second) = self.frame.split_at_mut(TICK);
        let (window_first, window_second) = self.window.split_at(TICK);
        for ((x, &sample), &w) in first.iter_mut().zip(&self.previous).zip(window_first) {
            *x = sample * w;
        }
        for ((x, &sample), &w) in second.iter_mut().zip(tick).zip(window_second) {
            *x = sample * w;
        }
        self.previous.copy_from_slice(tick);
        self.forward
            .process_with_scratch(&mut self.frame, spectrum, &mut self.forward_scratch)
            .expect("buffers are the planned sizes");
        for bin in spectrum.iter_mut() {
            *bin *= WINDOW_NORM;
        }
    }

    /// The next tick of output from `spectrum` (which is used up).
    pub fn synthesize(&mut self, spectrum: &mut Spectrum, tick: &mut [f32]) {
        // A real signal's spectrum has no imaginary part at 0 Hz and at
        // the top; the inverse transform ignores them, and complains.
        spectrum[0].im = 0.0;
        spectrum[FREQS - 1].im = 0.0;
        self.inverse
            .process_with_scratch(spectrum, &mut self.frame, &mut self.inverse_scratch)
            .expect("buffers are the planned sizes");
        for (x, &w) in self.frame.iter_mut().zip(&self.window) {
            *x *= w;
        }
        let (first, second) = self.frame.split_at(TICK);
        for ((out, &x), &overlap) in tick.iter_mut().zip(first).zip(&self.overlap) {
            *out = x + overlap;
        }
        self.overlap.copy_from_slice(second);
    }
}

/// What the network reads from each frame.
pub struct Features {
    bands: [usize; ERB_BANDS],
    erb_mean: [f32; ERB_BANDS],
    unit_norm: [f32; DF_BINS],
}

impl Features {
    pub fn new() -> Self {
        Self {
            bands: erb_bands(),
            erb_mean: spread(ERB_NORM_START),
            unit_norm: spread(UNIT_NORM_START),
        }
    }

    /// Each ERB band's energy in dB, less its running mean, over 40.
    pub fn erb(&mut self, spectrum: &Spectrum, out: &mut [f32; ERB_BANDS]) {
        let mut start = 0;
        for ((&size, out), mean) in self
            .bands
            .iter()
            .zip(out.iter_mut())
            .zip(&mut self.erb_mean)
        {
            let k = 1.0 / size as f32;
            let mut energy = 0.0;
            for bin in &spectrum[start..start + size] {
                energy += (bin.re * bin.re + bin.im * bin.im) * k;
            }
            start += size;
            let db = (energy + 1e-10).log10() * 10.0;
            *mean = db * (1.0 - NORM_ALPHA) + *mean * NORM_ALPHA;
            *out = (db - *mean) / 40.0;
        }
    }

    /// The low bins, each divided by the square root of its running
    /// magnitude.
    pub fn low_spectrum(&mut self, spectrum: &Spectrum, out: &mut [Complex32; DF_BINS]) {
        for ((out, &bin), norm) in out.iter_mut().zip(spectrum.iter()).zip(&mut self.unit_norm) {
            *norm = bin.norm() * (1.0 - NORM_ALPHA) + *norm * NORM_ALPHA;
            *out = bin / norm.sqrt();
        }
    }

    /// Multiplies each band's bins by its gain.
    pub fn apply_gains(&self, spectrum: &mut Spectrum, gains: &[f32]) {
        let mut start = 0;
        for (&size, &gain) in self.bands.iter().zip(gains) {
            for bin in &mut spectrum[start..start + size] {
                *bin *= gain;
            }
            start += size;
        }
    }
}

/// `N` values evenly from `from` to `to`.
fn spread<const N: usize>((from, to): (f32, f32)) -> [f32; N] {
    let step = (to - from) / (N - 1) as f32;
    std::array::from_fn(|i| from + i as f32 * step)
}

fn freq_to_erb(hz: f32) -> f32 {
    9.265 * (hz / (24.7 * 9.265)).ln_1p()
}

fn erb_to_freq(erb: f32) -> f32 {
    24.7 * 9.265 * ((erb / 9.265).exp() - 1.0)
}

/// How many bins each ERB band covers, from 0 Hz up, at least two each.
fn erb_bands() -> [usize; ERB_BANDS] {
    let bin_width = SAMPLE_RATE as f32 / FFT_SIZE as f32;
    let low = freq_to_erb(0.0);
    let high = freq_to_erb((SAMPLE_RATE / 2) as f32);
    let step = (high - low) / ERB_BANDS as f32;
    let mut bands = [0; ERB_BANDS];
    let mut previous = 0i32;
    let mut borrowed = 0i32;
    for (i, band) in bands.iter_mut().enumerate() {
        let top = (erb_to_freq(low + (i + 1) as f32 * step) / bin_width).round() as i32;
        let mut count = top - previous - borrowed;
        if count < MIN_FREQS_PER_BAND as i32 {
            borrowed = MIN_FREQS_PER_BAND as i32 - count;
            count = MIN_FREQS_PER_BAND as i32;
        } else {
            borrowed = 0;
        }
        *band = count as usize;
        previous = top;
    }
    // The top bin, 24 kHz itself.
    bands[ERB_BANDS - 1] += 1;
    let total: usize = bands.iter().sum();
    bands[ERB_BANDS - 1] -= total - FREQS;
    bands
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bands_cover_the_spectrum_with_at_least_two_bins_each() {
        let bands = erb_bands();

        assert_eq!(bands.iter().sum::<usize>(), FREQS);
        assert!(bands.iter().all(|&size| size >= MIN_FREQS_PER_BAND));
        // Narrow at the bottom, wide at the top.
        assert!(bands[0] < bands[ERB_BANDS - 1]);
    }

    #[test]
    fn analysis_then_synthesis_gives_the_input_a_tick_later() {
        let mut stft = Stft::new();
        let input: Vec<f32> = (0..20 * TICK)
            .map(|i| (i as f32 * 0.013).sin() * 0.5 + (i as f32 * 0.0021).cos() * 0.2)
            .collect();
        let mut output = Vec::new();
        let mut spectrum = SILENT_SPECTRUM;
        let mut tick = [0.0; TICK];

        for chunk in input.as_chunks::<TICK>().0 {
            stft.analyze(chunk, &mut spectrum);
            stft.synthesize(&mut spectrum, &mut tick);
            output.extend_from_slice(&tick);
        }

        let error = input[..input.len() - TICK]
            .iter()
            .zip(&output[TICK..])
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(error < 1e-4, "off by {error}");
    }
}
