//! High noise suppression (plan §7.3): DeepFilterNet 3, the network behind
//! DeepFilterNet's real-time PipeWire plugin, run with tract.
//!
//! Each tick is one hop of a 20 ms spectrum. An encoder reads the newest
//! frame and estimates its local SNR; then, for the frame two hops back,
//! an ERB decoder gives a gain per band and a deep-filter decoder rebuilds
//! the bins below 4.8 kHz from the five latest frames. The output lags the
//! input by 30 ms: the two hops of lookahead and the frame's overlap.
//!
//! Ported from DeepFilterNet's libDF (https://github.com/Rikorose/DeepFilterNet,
//! commit d375b2d, © Hendrik Schröter, MIT OR Apache-2.0) to current tract:
//! its streaming path for one channel with the default settings. The model
//! and its licences are in `models/deepfilternet3` (D27).

mod spectrum;

use std::sync::{Arc, OnceLock};
use std::time::Instant;

use realfft::num_complex::Complex32;
use tract_onnx::prelude::*;
use tract_onnx::tract_hir::shapefactoid;
use tract_pulse::model::{PulsedModel, PulsedModelExt};

use self::spectrum::{DF_BINS, ERB_BANDS, Features, SILENT_SPECTRUM, Spectrum, Stft};
use super::{SAMPLE_RATE, TICK};

/// Frames the deep filter combines, oldest first.
const DF_ORDER: usize = 5;
/// Hops between the newest frame and the one being cleaned.
const LOOKAHEAD: usize = 2;
/// The encoder's convolution channels.
const CHANNELS: usize = 64;
const HIDDEN: usize = CHANNELS * ERB_BANDS / 4;

/// Local SNR thresholds in dB (libDF's defaults): below the first only
/// noise is left, so the frame is silenced; above the second it is clean
/// enough to leave alone; above the third the band gains suffice.
const NOISE_ONLY: f32 = -10.0;
const CLEAN: f32 = 30.0;
const LITTLE_NOISE: f32 = 20.0;
/// Digital silence: ticks quieter than this (as a mean square) count
/// towards skipping the network, as do ticks it calls clean; past
/// `QUIET_TICKS` of them in a row it is not run and the output is silent.
const QUIET_MEAN_SQUARE: f32 = 1e-7;
const QUIET_TICKS: usize = 5;
/// The local SNR a skipped tick reports.
const SKIPPED_SNR: f32 = -15.0;

const ENCODER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/models/deepfilternet3/enc.onnx"
));
const ERB_DECODER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/models/deepfilternet3/erb_dec.onnx"
));
const DF_DECODER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/models/deepfilternet3/df_dec.onnx"
));

#[derive(Debug, Clone, thiserror::Error)]
#[error("High noise suppression failed: {0}")]
pub struct DeepFilterError(String);

impl From<TractError> for DeepFilterError {
    fn from(error: TractError) -> Self {
        Self(format!("{error:#}"))
    }
}

/// The three networks, ready to run; loaded once.
struct Networks {
    encoder: Arc<TypedSimplePlan>,
    erb_decoder: Arc<TypedSimplePlan>,
    df_decoder: Arc<TypedSimplePlan>,
}

static NETWORKS: OnceLock<Result<Networks, DeepFilterError>> = OnceLock::new();

fn networks() -> Result<&'static Networks, DeepFilterError> {
    NETWORKS
        .get_or_init(|| {
            Ok(Networks {
                encoder: encoder()?,
                erb_decoder: erb_decoder()?,
                df_decoder: df_decoder()?,
            })
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// Loads the networks now, so the first [`DeepFilter::new`] is quick.
pub fn preload() -> Result<(), DeepFilterError> {
    networks().map(|_| ())
}

fn f32_fact(shape: impl Into<tract_onnx::tract_hir::infer::ShapeFactoid>) -> InferenceFact {
    InferenceFact::dt_shape(f32::datum_type(), shape)
}

/// Fixes a network's inputs and outputs, then turns it into one that takes
/// a frame at a time along its time axis, `S`.
fn streaming(
    bytes: &[u8],
    inputs: impl FnOnce(&Symbol) -> Vec<(&'static str, InferenceFact)>,
    outputs: &[&str],
) -> TractResult<Arc<TypedSimplePlan>> {
    let mut model = tract_onnx::onnx()
        .with_ignore_output_shapes(true)
        .model_for_read(&mut &*bytes)?;
    let time = model.symbols.sym("S");
    let inputs = inputs(&time);
    for (index, (_, fact)) in inputs.iter().enumerate() {
        model.set_input_fact(index, fact.clone())?;
    }
    let mut model = model
        .with_input_names(inputs.iter().map(|(name, _)| *name))?
        .with_outputs_by_name(outputs)?;
    model.analyse(true)?;
    let mut model = model.into_typed()?;
    model.declutter()?;
    let pulsed = PulsedModel::new(&model, time, &1.to_dim())?;
    pulsed.into_typed()?.into_optimized()?.into_runnable()
}

fn encoder() -> TractResult<Arc<TypedSimplePlan>> {
    streaming(
        ENCODER,
        |time| {
            vec![
                ("feat_erb", f32_fact(shapefactoid!(1, 1, time, ERB_BANDS))),
                ("feat_spec", f32_fact(shapefactoid!(1, 2, time, DF_BINS))),
            ]
        },
        &["e0", "e1", "e2", "e3", "emb", "c0", "lsnr"],
    )
}

fn erb_decoder() -> TractResult<Arc<TypedSimplePlan>> {
    streaming(
        ERB_DECODER,
        |time| {
            vec![
                ("emb", f32_fact(shapefactoid!(1, time, HIDDEN))),
                (
                    "e3",
                    f32_fact(shapefactoid!(1, CHANNELS, time, (ERB_BANDS / 4))),
                ),
                (
                    "e2",
                    f32_fact(shapefactoid!(1, CHANNELS, time, (ERB_BANDS / 4))),
                ),
                (
                    "e1",
                    f32_fact(shapefactoid!(1, CHANNELS, time, (ERB_BANDS / 2))),
                ),
                ("e0", f32_fact(shapefactoid!(1, CHANNELS, time, ERB_BANDS))),
            ]
        },
        &["m"],
    )
}

fn df_decoder() -> TractResult<Arc<TypedSimplePlan>> {
    streaming(
        DF_DECODER,
        |time| {
            vec![
                ("emb", f32_fact(shapefactoid!(1, time, HIDDEN))),
                ("c0", f32_fact(shapefactoid!(1, CHANNELS, time, DF_BINS))),
            ]
        },
        &["coefs"],
    )
}

/// What a frame needs, by its local SNR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stages {
    /// Only noise: silenced.
    Silence,
    /// Clean enough to leave alone.
    Untouched,
    /// A little noise: the band gains.
    Gains,
    /// Noisy: the band gains, then the deep filter for the low bins.
    GainsAndDeepFilter,
}

impl Stages {
    fn for_snr(snr: f32) -> Self {
        if snr < NOISE_ONLY {
            Self::Silence
        } else if snr > CLEAN {
            Self::Untouched
        } else if snr > LITTLE_NOISE {
            Self::Gains
        } else {
            Self::GainsAndDeepFilter
        }
    }
}

/// One microphone's High noise suppression.
pub struct DeepFilter {
    encoder: TypedSimpleState,
    erb_decoder: TypedSimpleState,
    df_decoder: TypedSimpleState,
    stft: Stft,
    features: Features,
    /// The latest frames, oldest first.
    frames: [Spectrum; DF_ORDER],
    cleaned: Spectrum,
    erb: [f32; ERB_BANDS],
    low: [Complex32; DF_BINS],
    quiet_ticks: usize,
    /// Runs both decoders whatever the local SNR, for the benchmark.
    every_stage: bool,
}

impl DeepFilter {
    pub fn new() -> Result<Self, DeepFilterError> {
        let networks = networks()?;
        Ok(Self {
            encoder: TypedSimpleState::new(&networks.encoder)?,
            erb_decoder: TypedSimpleState::new(&networks.erb_decoder)?,
            df_decoder: TypedSimpleState::new(&networks.df_decoder)?,
            stft: Stft::new(),
            features: Features::new(),
            frames: [SILENT_SPECTRUM; DF_ORDER],
            cleaned: SILENT_SPECTRUM,
            erb: [0.0; ERB_BANDS],
            low: [Complex32::new(0.0, 0.0); DF_BINS],
            quiet_ticks: 0,
            every_stage: false,
        })
    }

    /// Cleans a tick in place (the output is 30 ms behind); returns the
    /// tick's local SNR estimate in dB.
    pub fn process(&mut self, tick: &mut [f32]) -> Result<f32, DeepFilterError> {
        debug_assert_eq!(tick.len(), TICK);
        let mean_square = tick.iter().map(|s| s * s).sum::<f32>() / TICK as f32;
        if mean_square < QUIET_MEAN_SQUARE {
            self.quiet_ticks += 1;
        } else {
            self.quiet_ticks = 0;
        }
        if self.quiet_ticks > QUIET_TICKS {
            tick.fill(0.0);
            return Ok(SKIPPED_SNR);
        }

        self.frames.rotate_left(1);
        self.stft.analyze(tick, &mut self.frames[DF_ORDER - 1]);
        let newest = &self.frames[DF_ORDER - 1];
        self.features.erb(newest, &mut self.erb);
        self.features.low_spectrum(newest, &mut self.low);

        let encoded = self.encoder.run(tvec!(
            Tensor::from_shape(&[1, 1, 1, ERB_BANDS], &self.erb)?.into_tvalue(),
            low_spectrum_tensor(&self.low)?.into_tvalue(),
        ))?;
        let snr = *encoded[6].try_as_plain_ram()?.to_scalar::<f32>()?;

        self.cleaned = self.frames[DF_ORDER - 1 - LOOKAHEAD];
        let stages = if self.every_stage {
            Stages::GainsAndDeepFilter
        } else {
            Stages::for_snr(snr)
        };
        match stages {
            Stages::Silence => {
                self.cleaned = SILENT_SPECTRUM;
                self.quiet_ticks = 0;
            }
            Stages::Untouched => self.quiet_ticks += 1,
            Stages::Gains | Stages::GainsAndDeepFilter => {
                let gains = self.erb_decoder.run(tvec!(
                    encoded[4].clone(),
                    encoded[3].clone(),
                    encoded[2].clone(),
                    encoded[1].clone(),
                    encoded[0].clone(),
                ))?;
                self.features.apply_gains(
                    &mut self.cleaned,
                    gains[0].try_as_plain_ram()?.as_slice::<f32>()?,
                );
                if stages == Stages::GainsAndDeepFilter {
                    let coefficients = self
                        .df_decoder
                        .run(tvec!(encoded[4].clone(), encoded[5].clone()))?;
                    self.deep_filter(coefficients[0].try_as_plain_ram()?.as_slice::<f32>()?);
                }
                self.quiet_ticks = 0;
            }
        }
        self.stft.synthesize(&mut self.cleaned, tick);
        Ok(snr)
    }

    /// Rebuilds the low bins of the cleaned frame from the latest frames:
    /// for each bin, a complex coefficient per frame, laid out as bin, then
    /// frame, then real and imaginary parts.
    fn deep_filter(&mut self, coefficients: &[f32]) {
        for (bin, cleaned) in self.cleaned[..DF_BINS].iter_mut().enumerate() {
            let mut sum = Complex32::new(0.0, 0.0);
            for (order, frame) in self.frames.iter().enumerate() {
                let at = (bin * DF_ORDER + order) * 2;
                sum += frame[bin] * Complex32::new(coefficients[at], coefficients[at + 1]);
            }
            *cleaned = sum;
        }
    }
}

/// Two seconds of ticks: what the first-run benchmark runs.
const BENCHMARK_TICKS: usize = 200;

/// The first-run benchmark (plan §7.3): the share of each 10 ms tick High
/// needs on this computer at most, running every stage for two seconds.
pub fn benchmark() -> Result<f32, DeepFilterError> {
    let mut filter = DeepFilter::new()?;
    filter.every_stage = true;
    let audio = benchmark_audio();
    let start = Instant::now();
    for tick in audio.as_chunks::<TICK>().0 {
        filter.process(&mut tick.clone())?;
    }
    let budget = BENCHMARK_TICKS as f32 * TICK as f32 / SAMPLE_RATE as f32;
    Ok(start.elapsed().as_secs_f32() / budget)
}

/// A buzzy voice gliding around 140 Hz, with vowel-like resonances, under
/// white noise about 10 dB down. (The network soon calls a steady buzz
/// noise, which is why the benchmark runs every stage regardless.)
fn benchmark_audio() -> Vec<f32> {
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    let mut phases = [0.0f32; 12];
    (0..BENCHMARK_TICKS * TICK)
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            let pitch = 140.0 * (1.0 + 0.15 * (t * 1.3).sin() + 0.05 * (t * 7.0).sin());
            let mut voice = 0.0;
            for (harmonic, phase) in phases.iter_mut().enumerate() {
                let frequency = pitch * (harmonic + 1) as f32;
                let resonance = (-((frequency - 700.0) / 400.0).powi(2)).exp()
                    + 0.6 * (-((frequency - 1600.0) / 600.0).powi(2)).exp()
                    + 0.1;
                *phase = (*phase + std::f32::consts::TAU * frequency / SAMPLE_RATE as f32)
                    % std::f32::consts::TAU;
                voice += resonance * phase.sin();
            }
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let white = (seed >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0;
            0.03 * voice + 0.02 * white
        })
        .collect()
}

/// The low spectrum as the encoder takes it: real parts, then imaginary.
fn low_spectrum_tensor(low: &[Complex32; DF_BINS]) -> TractResult<Tensor> {
    let mut planes = [0.0f32; 2 * DF_BINS];
    let (re, im) = planes.split_at_mut(DF_BINS);
    for ((re, im), bin) in re.iter_mut().zip(im.iter_mut()).zip(low) {
        *re = bin.re;
        *im = bin.im;
    }
    Tensor::from_shape(&[1, 2, 1, DF_BINS], &planes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::dbfs;
    use crate::audio::fixtures::{at_level, noise, speech};

    #[test]
    fn the_local_snr_picks_the_stages_as_libdf_does() {
        assert_eq!(Stages::for_snr(-10.1), Stages::Silence);
        assert_eq!(Stages::for_snr(-10.0), Stages::GainsAndDeepFilter);
        assert_eq!(Stages::for_snr(20.0), Stages::GainsAndDeepFilter);
        assert_eq!(Stages::for_snr(20.1), Stages::Gains);
        assert_eq!(Stages::for_snr(30.0), Stages::Gains);
        assert_eq!(Stages::for_snr(30.1), Stages::Untouched);
    }

    #[test]
    fn the_benchmark_measures_a_share_of_each_tick() {
        let share = benchmark().unwrap();

        assert!(share > 0.0 && share.is_finite(), "{share}");
    }

    fn run(input: &[f32]) -> Vec<f32> {
        let mut filter = DeepFilter::new().unwrap();
        let mut out = Vec::with_capacity(input.len());
        for tick in input.as_chunks::<TICK>().0 {
            let mut cleaned = *tick;
            filter.process(&mut cleaned).unwrap();
            out.extend_from_slice(&cleaned);
        }
        out
    }

    /// The delay, in samples, at which `out` best matches `input`.
    fn delay(input: &[f32], out: &[f32]) -> usize {
        let score = |lag: usize| -> f64 {
            input
                .iter()
                .zip(&out[lag..])
                .map(|(x, y)| f64::from(*x) * f64::from(*y))
                .sum()
        };
        (0..4 * TICK)
            .max_by(|a, b| score(*a).total_cmp(&score(*b)))
            .unwrap()
    }

    #[test]
    fn the_voice_comes_out_30_ms_later() {
        let voice = at_level(&speech(), -22.0);

        let out = run(&voice);

        assert_eq!(delay(&voice, &out), 3 * TICK);
    }

    #[test]
    fn every_kind_of_noise_is_silenced() {
        for kind in ["fan", "street", "keyboard", "dog"] {
            let alone = at_level(&noise(kind, 5 * SAMPLE_RATE as usize), -35.0);

            let out = run(&alone);

            let settled = SAMPLE_RATE as usize..alone.len();
            let reduction = dbfs(&alone[settled.clone()]) - dbfs(&out[settled]);
            // The keyboard's desk thumps partly stay without the high-pass
            // filter in front (about 25 dB off here; 47 dB with it).
            assert!(reduction >= 20.0, "{kind}: only {reduction:.1} dB quieter");
        }
    }

    #[test]
    fn a_clean_voice_passes_untouched() {
        let voice = at_level(&speech(), -22.0);

        let out = run(&voice);

        let change = dbfs(&out[3 * TICK..]) - dbfs(&voice[..voice.len() - 3 * TICK]);
        assert!(change.abs() < 0.5, "{change:.1} dB");
    }

    #[test]
    fn digital_silence_after_a_clean_voice_skips_the_network() {
        let mut filter = DeepFilter::new().unwrap();
        let voice = at_level(&speech(), -22.0);
        for tick in voice[..TICK * 100].as_chunks::<TICK>().0 {
            filter.process(&mut tick.clone()).unwrap();
        }

        let snr: Vec<f32> = (0..20)
            .map(|_| filter.process(&mut [0.0; TICK]).unwrap())
            .collect();

        assert!(
            snr[QUIET_TICKS + 1..].iter().all(|s| *s == SKIPPED_SNR),
            "{snr:?}"
        );
    }

    #[test]
    fn digital_silence_stays_silent() {
        let mut filter = DeepFilter::new().unwrap();

        for _ in 0..10 {
            let mut tick = [0.0; TICK];
            let snr = filter.process(&mut tick).unwrap();

            assert!(tick.iter().all(|s| *s == 0.0));
            assert!(snr < LITTLE_NOISE, "{snr}");
        }
    }

    #[test]
    fn a_second_filter_shares_the_loaded_networks() {
        preload().unwrap();
        let start = std::time::Instant::now();

        DeepFilter::new().unwrap();

        assert!(start.elapsed() < std::time::Duration::from_millis(100));
    }
}
