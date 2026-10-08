//! The microphone's cleanup (plan §7.2–§7.4), one 10 ms tick at a time at
//! 48 kHz: a high-pass filter and echo cancellation, then noise suppression
//! (off, Standard or High), then automatic gain control, so noise is not
//! amplified. The echo canceller hears everything played.
//!
//! Echo cancellation and gain control are the `aec3` crate's Rust port of
//! WebRTC's AudioProcessing (D27); the high-pass filter is our own, as
//! WebRTC's (aec3's is tuned wrong at 48 kHz); Standard noise
//! suppression is RNNoise through `nnnoiseless`, which also says how
//! likely each tick is to be speech; High is DeepFilterNet
//! ([`super::deep_filter`]), which falls back to Standard when the computer
//! cannot keep up.

use std::time::{Duration, Instant};

use aec3::api::config::EchoCanceller3Config;
use aec3::api::control::EchoControl;
use aec3::audio_processing::aec3::echo_canceller3::EchoCanceller3;
use aec3::audio_processing::agc2::input_volume_controller::Config as InputVolumeControllerConfig;
use aec3::audio_processing::audio_buffer::AudioBuffer;
use aec3::audio_processing::gain_controller2::{
    GainController2, GainController2Config, InputVolumeControllerRuntimeConfig,
};
use aec3::audio_processing::stream_config::StreamConfig;

use nnnoiseless::DenoiseState;

use super::capture::Heard;
use super::deep_filter::{DeepFilter, DeepFilterError};
use super::{SAMPLE_RATE, TICK, dbfs};

/// RNNoise works on samples in the 16-bit range.
const RNNOISE_SCALE: f32 = 32_768.0;
/// The far end's recent loudness: the loudest tick played in the last
/// 300 ms, which covers the echo's way back through the room and devices.
const PLAYED_TICKS: usize = 30;
/// A microphone tick this far under what was just played, after echo
/// cancellation, is what is left of the echo, not someone speaking.
const ECHO_MARGIN_DB: f32 = 25.0;
/// Played audio quieter than this leaves no echo worth guarding against.
const FAR_END_DBFS: f32 = -60.0;
/// High gives way to Standard when it takes more than 60 % of each tick
/// over two seconds (plan §7.3).
const FALLBACK_TICKS: usize = 200;
/// On the first run High is the default when it needs less than this
/// share of each tick (plan §7.3).
const HIGH_BY_DEFAULT_UNDER: f32 = 0.2;
const FALLBACK_SHARE: f64 = 0.6;
const TICK_DURATION: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NoiseSuppression {
    Off,
    /// RNNoise: very light.
    #[default]
    Standard,
    /// DeepFilterNet: better on keyboards, dogs and fans, heavier.
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessingSettings {
    pub echo_cancellation: bool,
    pub noise_suppression: NoiseSuppression,
    pub automatic_gain: bool,
}

impl Default for ProcessingSettings {
    fn default() -> Self {
        Self {
            echo_cancellation: true,
            noise_suppression: NoiseSuppression::Standard,
            automatic_gain: true,
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("audio processing could not start: {0}")]
pub struct ProcessingError(String);

impl From<DeepFilterError> for ProcessingError {
    fn from(error: DeepFilterError) -> Self {
        Self(error.to_string())
    }
}

pub struct VoiceProcessing {
    settings: ProcessingSettings,
    /// RNNoise looks for a voice even with suppression off, for automatic
    /// sensitivity.
    voice_analysis: bool,
    high_pass: HighPass,
    /// While echo cancellation is on.
    echo: Option<Box<EchoCancellation>>,
    /// The levels of the last ticks played, oldest overwritten first.
    played: [f32; PLAYED_TICKS],
    played_next: usize,
    /// While noise suppression is High.
    high: Option<Box<DeepFilter>>,
    load: LoadWatch,
    /// High gave way to Standard since the last look.
    fell_back: bool,
    gain: Option<Box<GainControl>>,
    denoise: Box<DenoiseState<'static>>,
    scaled: [f32; TICK],
    cleaned: [f32; TICK],
    /// Added to High's time per tick, to test the fallback.
    #[cfg(test)]
    artificial_load: Duration,
}

impl VoiceProcessing {
    pub fn new(settings: ProcessingSettings) -> Result<Self, ProcessingError> {
        Ok(Self {
            high_pass: HighPass::default(),
            echo: settings
                .echo_cancellation
                .then(|| Box::new(EchoCancellation::new())),
            played: [f32::MIN; PLAYED_TICKS],
            played_next: 0,
            high: high(settings.noise_suppression)?,
            load: LoadWatch::default(),
            fell_back: false,
            gain: settings
                .automatic_gain
                .then(|| Box::new(GainControl::new())),
            settings,
            voice_analysis: false,
            denoise: DenoiseState::new(),
            scaled: [0.0; TICK],
            cleaned: [0.0; TICK],
            #[cfg(test)]
            artificial_load: Duration::ZERO,
        })
    }

    pub fn settings(&self) -> ProcessingSettings {
        self.settings
    }

    /// Takes new settings; the parts that changed start over.
    pub fn set_settings(&mut self, settings: ProcessingSettings) -> Result<(), ProcessingError> {
        if settings.echo_cancellation != self.settings.echo_cancellation {
            self.echo = settings
                .echo_cancellation
                .then(|| Box::new(EchoCancellation::new()));
        }
        if settings.noise_suppression != self.settings.noise_suppression {
            self.high = high(settings.noise_suppression)?;
            self.load = LoadWatch::default();
        }
        if settings.automatic_gain != self.settings.automatic_gain {
            self.gain = settings
                .automatic_gain
                .then(|| Box::new(GainControl::new()));
        }
        self.settings = settings;
        Ok(())
    }

    /// Whether High gave way to Standard since the last call.
    pub fn take_fallback(&mut self) -> bool {
        std::mem::take(&mut self.fell_back)
    }

    /// Whether to find each tick's voice probability while noise
    /// suppression is off.
    pub fn set_voice_analysis(&mut self, on: bool) {
        self.voice_analysis = on;
    }

    /// A tick of everything played, for the echo canceller.
    pub fn render(&mut self, tick: &[f32]) {
        if let Some(echo) = &mut self.echo {
            echo.render(tick);
        }
        self.played[self.played_next] = dbfs(tick);
        self.played_next = (self.played_next + 1) % PLAYED_TICKS;
    }

    /// Cleans a tick of the microphone in place; says how likely it is to
    /// be speech (while noise suppression or voice analysis is on) and
    /// whether it is only the echo's remains, which gain control leaves
    /// alone so it is not turned up.
    pub fn capture(&mut self, tick: &mut [f32]) -> Heard {
        self.high_pass.process(tick);
        let mut echo_only = false;
        if let Some(echo) = &mut self.echo {
            echo.capture(tick);
            let played = self.played.iter().copied().fold(f32::MIN, f32::max);
            echo_only = played > FAR_END_DBFS && played - dbfs(tick) >= ECHO_MARGIN_DB;
        }
        let voice_probability = match self.settings.noise_suppression {
            NoiseSuppression::Off => self.voice_analysis.then(|| self.analyze(tick)),
            NoiseSuppression::Standard => Some(self.rnnoise(tick)),
            NoiseSuppression::High => {
                self.deep_filter(tick);
                // RNNoise looks at what DeepFilterNet left, so the voice
                // probability lines up with what is sent.
                self.voice_analysis.then(|| self.analyze(tick))
            }
        };
        if let Some(gain) = &mut self.gain
            && !echo_only
        {
            gain.process(tick);
        }
        Heard {
            voice_probability,
            echo_only,
        }
    }

    /// High, timed; falls back to Standard when it is too slow or fails.
    fn deep_filter(&mut self, tick: &mut [f32]) {
        let Some(filter) = &mut self.high else {
            return;
        };
        let start = Instant::now();
        let failed = filter.process(tick).is_err();
        #[cfg(test)]
        std::thread::sleep(self.artificial_load);
        if self.load.record(start.elapsed()) || failed {
            self.settings.noise_suppression = NoiseSuppression::Standard;
            self.high = None;
            self.load = LoadWatch::default();
            self.fell_back = true;
        }
    }

    /// RNNoise's voice probability, leaving the audio as it is.
    fn analyze(&mut self, tick: &[f32]) -> f32 {
        for (scaled, sample) in self.scaled.iter_mut().zip(tick.iter()) {
            *scaled = sample * RNNOISE_SCALE;
        }
        self.denoise.process_frame(&mut self.cleaned, &self.scaled)
    }

    fn rnnoise(&mut self, tick: &mut [f32]) -> f32 {
        for (scaled, sample) in self.scaled.iter_mut().zip(tick.iter()) {
            *scaled = sample * RNNOISE_SCALE;
        }
        let probability = self.denoise.process_frame(&mut self.cleaned, &self.scaled);
        for (sample, cleaned) in tick.iter_mut().zip(self.cleaned.iter()) {
            *sample = cleaned / RNNOISE_SCALE;
        }
        probability
    }
}

/// The first run's noise suppression (plan §7.3): High when it needs under
/// 20 % of each tick on this computer, Standard otherwise. Takes a moment.
pub fn recommended_noise_suppression() -> NoiseSuppression {
    choose(super::deep_filter::benchmark().ok())
}

/// `share`: of each tick High needs, when it can run at all.
fn choose(share: Option<f32>) -> NoiseSuppression {
    match share {
        Some(share) if share < HIGH_BY_DEFAULT_UNDER => NoiseSuppression::High,
        _ => NoiseSuppression::Standard,
    }
}

fn high(mode: NoiseSuppression) -> Result<Option<Box<DeepFilter>>, ProcessingError> {
    Ok(match mode {
        NoiseSuppression::High => Some(Box::new(DeepFilter::new()?)),
        NoiseSuppression::Off | NoiseSuppression::Standard => None,
    })
}

/// How long High took over the last two seconds of ticks.
struct LoadWatch {
    took: [Duration; FALLBACK_TICKS],
    next: usize,
    recorded: usize,
    total: Duration,
}

impl Default for LoadWatch {
    fn default() -> Self {
        Self {
            took: [Duration::ZERO; FALLBACK_TICKS],
            next: 0,
            recorded: 0,
            total: Duration::ZERO,
        }
    }
}

impl LoadWatch {
    /// Adds a tick's time; whether the last two seconds took too long.
    fn record(&mut self, took: Duration) -> bool {
        self.total = self.total - self.took[self.next] + took;
        self.took[self.next] = took;
        self.next = (self.next + 1) % FALLBACK_TICKS;
        self.recorded = (self.recorded + 1).min(FALLBACK_TICKS);
        self.recorded == FALLBACK_TICKS
            && self.total.as_secs_f64()
                > FALLBACK_SHARE * (TICK_DURATION * FALLBACK_TICKS as u32).as_secs_f64()
    }
}

/// A 10 ms buffer at 48 kHz, mono, as aec3's processing modules take it.
fn audio_buffer() -> AudioBuffer {
    let rate = SAMPLE_RATE as usize;
    AudioBuffer::from_sample_rates(rate, 1, rate, 1, rate)
}

fn stream() -> StreamConfig {
    StreamConfig::new(SAMPLE_RATE as usize, 1, false)
}

/// AEC3, driven as aec3's own node drives it but without its graph
/// runtime, which cost a third of the time: a buffer per direction, split
/// into frequency bands.
struct EchoCancellation {
    canceller: EchoCanceller3,
    render: AudioBuffer,
    capture: AudioBuffer,
    stream: StreamConfig,
}

impl EchoCancellation {
    fn new() -> Self {
        Self {
            canceller: EchoCanceller3::with_multichannel_config(
                EchoCanceller3Config::default(),
                Some(EchoCanceller3Config::create_default_multichannel_config()),
                SAMPLE_RATE as i32,
                1,
                1,
            ),
            render: audio_buffer(),
            capture: audio_buffer(),
            stream: stream(),
        }
    }

    fn render(&mut self, tick: &[f32]) {
        self.render.copy_from(&[tick], &self.stream);
        self.render.split_into_frequency_bands();
        self.canceller.analyze_render(&mut self.render);
    }

    fn capture(&mut self, tick: &mut [f32]) {
        self.capture.copy_from(&[tick], &self.stream);
        self.canceller.analyze_capture(&mut self.capture);
        self.capture.split_into_frequency_bands();
        self.canceller.process_capture(&mut self.capture, false);
        self.capture.merge_frequency_bands();
        self.capture.copy_to_stream(&self.stream, &mut [tick]);
    }
}

/// AGC2, without the graph runtime either.
struct GainControl {
    controller: GainController2,
    buffer: AudioBuffer,
    stream: StreamConfig,
}

impl GainControl {
    fn new() -> Self {
        // The operating system's microphone volume is not ours to steer.
        let config = GainController2Config {
            input_volume_controller: InputVolumeControllerRuntimeConfig { enabled: false },
            ..GainController2Config::default()
        };
        Self {
            controller: GainController2::new(
                config,
                InputVolumeControllerConfig::default(),
                SAMPLE_RATE as usize,
                1,
                true,
            ),
            buffer: audio_buffer(),
            stream: stream(),
        }
    }

    fn process(&mut self, tick: &mut [f32]) {
        self.buffer.copy_from(&[tick], &self.stream);
        self.controller.process(false, &mut self.buffer);
        self.buffer.copy_to_stream(&self.stream, &mut [tick]);
    }
}

/// A second-order Butterworth high-pass at 100 Hz, which is what WebRTC's
/// filter does: rumble and mains hum out before echo cancellation, the
/// voice left alone. (aec3 0.4's filter applies its 48 kHz coefficients
/// to the 16 kHz low band, which moves the cutoff to about 33 Hz.)
struct HighPass {
    b: [f64; 3],
    a: [f64; 2],
    /// Transposed direct form II state.
    state: [f64; 2],
}

/// Where the high-pass filter starts to cut.
const HIGH_PASS_HZ: f64 = 100.0;

impl Default for HighPass {
    fn default() -> Self {
        let w0 = std::f64::consts::TAU * HIGH_PASS_HZ / f64::from(SAMPLE_RATE);
        let alpha = w0.sin() / (2.0 * std::f64::consts::FRAC_1_SQRT_2);
        let cos = w0.cos();
        let a0 = 1.0 + alpha;
        Self {
            b: [
                (1.0 + cos) / 2.0 / a0,
                -(1.0 + cos) / a0,
                (1.0 + cos) / 2.0 / a0,
            ],
            a: [-2.0 * cos / a0, (1.0 - alpha) / a0],
            state: [0.0; 2],
        }
    }
}

impl HighPass {
    fn process(&mut self, tick: &mut [f32]) {
        let [b0, b1, b2] = self.b;
        let [a1, a2] = self.a;
        for sample in tick {
            let x = f64::from(*sample);
            let y = b0 * x + self.state[0];
            self.state[0] = b1 * x - a1 * y + self.state[1];
            self.state[1] = b2 * x - a2 * y;
            *sample = y as f32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::fixtures::{at_level, noise, speech, voice_with};
    use crate::audio::{SAMPLE_RATE, TICK, dbfs};

    /// Runs `capture` through processing, with `render` played meanwhile;
    /// returns the output and each tick's voice probability.
    fn process(
        settings: ProcessingSettings,
        render: &[f32],
        capture: &[f32],
    ) -> (Vec<f32>, Vec<Option<f32>>) {
        let mut processing = VoiceProcessing::new(settings).unwrap();
        let mut out = Vec::with_capacity(capture.len());
        let mut probabilities = Vec::new();
        let silence = [0.0; TICK];
        for (index, tick) in capture.as_chunks::<TICK>().0.iter().enumerate() {
            let played = render
                .get(index * TICK..(index + 1) * TICK)
                .unwrap_or(&silence);
            processing.render(played);
            let mut cleaned = *tick;
            probabilities.push(processing.capture(&mut cleaned).voice_probability);
            out.extend_from_slice(&cleaned);
        }
        (out, probabilities)
    }

    const ONLY_EQUALIZING: ProcessingSettings = ProcessingSettings {
        echo_cancellation: false,
        noise_suppression: NoiseSuppression::Off,
        automatic_gain: false,
    };

    #[test]
    fn the_echo_of_what_is_played_is_removed() {
        let far = at_level(&speech(), -20.0);
        // The room: 40 ms later, 8 dB down, with a short tail.
        let delay = SAMPLE_RATE as usize / 25;
        let mut echo = vec![0.0; far.len()];
        for (i, sample) in far.iter().enumerate() {
            for (tap, gain) in [(0usize, 0.4f32), (480, 0.15), (1_100, 0.06)] {
                if let Some(slot) = echo.get_mut(i + delay + tap) {
                    *slot += gain * sample;
                }
            }
        }
        let settings = ProcessingSettings {
            echo_cancellation: true,
            ..ONLY_EQUALIZING
        };

        let (out, _) = process(settings, &far, &echo);

        // After a second to find the echo path.
        let converged = SAMPLE_RATE as usize..echo.len();
        let reduction = dbfs(&echo[converged.clone()]) - dbfs(&out[converged]);
        assert!(
            reduction >= 20.0,
            "the echo is only {reduction:.1} dB quieter"
        );
    }

    fn suppressing(mode: NoiseSuppression) -> ProcessingSettings {
        ProcessingSettings {
            noise_suppression: mode,
            ..ONLY_EQUALIZING
        }
    }

    /// How much quieter four seconds of `kind` of noise come out, after a
    /// second to settle.
    fn noise_reduction(kind: &str, mode: NoiseSuppression) -> f32 {
        let alone = at_level(&noise(kind, 5 * SAMPLE_RATE as usize), -35.0);

        let (out, _) = process(suppressing(mode), &[], &alone);

        let settled = SAMPLE_RATE as usize..alone.len();
        dbfs(&alone[settled.clone()]) - dbfs(&out[settled])
    }

    /// How the voice comes out of `kind` of noise, against the clean voice.
    fn voice_change(kind: &str, mode: NoiseSuppression) -> f32 {
        let (mix, voice) = voice_with(kind);

        let (out, _) = process(suppressing(mode), &[], &mix);

        // High's output is 30 ms behind.
        let delay = if mode == NoiseSuppression::High {
            3 * TICK
        } else {
            0
        };
        dbfs(&out[delay..]) - dbfs(&voice[..voice.len() - delay])
    }

    #[test]
    fn standard_suppression_quiets_fans_streets_and_keyboards() {
        // RNNoise leaves barking alone (under 1 dB here); that is High's job
        // (plan §7.3).
        for (kind, at_least) in [("fan", 8.0), ("street", 30.0), ("keyboard", 30.0)] {
            let reduction = noise_reduction(kind, NoiseSuppression::Standard);
            let change = voice_change(kind, NoiseSuppression::Standard);

            assert!(
                reduction >= at_least,
                "{kind}: the noise is only {reduction:.1} dB quieter"
            );
            assert!(
                change.abs() < 1.0,
                "{kind}: the voice changed {change:.1} dB"
            );
        }
    }

    #[test]
    fn high_suppression_silences_every_kind_of_noise() {
        for kind in ["fan", "street", "keyboard", "dog"] {
            let reduction = noise_reduction(kind, NoiseSuppression::High);
            let change = voice_change(kind, NoiseSuppression::High);

            assert!(
                reduction >= 40.0,
                "{kind}: the noise is only {reduction:.1} dB quieter"
            );
            // DeepFilterNet trims a voice with noise under it by a few dB;
            // gain control, after it, makes the level up.
            assert!(change > -6.0, "{kind}: the voice lost {change:.1} dB");
        }
    }

    #[test]
    fn the_voice_probability_tells_speech_from_noise() {
        let mean = |input: &[f32]| {
            let (_, probabilities) = process(suppressing(NoiseSuppression::Standard), &[], input);
            probabilities.iter().map(|p| p.unwrap()).sum::<f32>() / probabilities.len() as f32
        };

        let noise_only = mean(&at_level(&noise("fan", 3 * SAMPLE_RATE as usize), -35.0));
        let speaking = mean(&voice_with("fan").0);

        assert!(noise_only < 0.2, "noise looks like speech: {noise_only:.2}");
        assert!(speaking > 0.5, "speech does not: {speaking:.2}");
    }

    #[test]
    fn without_noise_suppression_there_is_no_voice_probability() {
        let (_, probabilities) = process(ONLY_EQUALIZING, &[], &vec![0.0; 10 * TICK]);

        assert!(probabilities.iter().all(Option::is_none));
    }

    #[test]
    fn gain_control_lifts_a_quiet_voice() {
        let quiet = at_level(&speech(), -45.0);
        let mut capture = quiet.clone();
        capture.extend_from_slice(&quiet);
        let settings = ProcessingSettings {
            automatic_gain: true,
            ..ONLY_EQUALIZING
        };

        let (out, _) = process(settings, &[], &capture);

        // The second time through, once the gain has adapted.
        let second = quiet.len()..capture.len();
        let lift = dbfs(&out[second.clone()]) - dbfs(&capture[second]);
        assert!(lift >= 6.0, "only {lift:.1} dB louder");
    }

    #[test]
    fn with_high_the_voice_probability_is_there_only_for_voice_analysis() {
        let settings = ProcessingSettings {
            noise_suppression: NoiseSuppression::High,
            ..ONLY_EQUALIZING
        };
        let mut processing = VoiceProcessing::new(settings).unwrap();
        let mut tick = [0.0; TICK];

        let without = processing.capture(&mut tick).voice_probability;
        processing.set_voice_analysis(true);
        let with = processing.capture(&mut tick).voice_probability;

        assert_eq!(without, None);
        assert!(with.is_some());
    }

    fn high_under_load(load: Duration, ticks: usize) -> VoiceProcessing {
        let settings = ProcessingSettings {
            noise_suppression: NoiseSuppression::High,
            ..ONLY_EQUALIZING
        };
        let mut processing = VoiceProcessing::new(settings).unwrap();
        processing.artificial_load = load;
        let (capture, _) = voice_with("fan");
        for tick in capture.as_chunks::<TICK>().0.iter().cycle().take(ticks) {
            processing.capture(&mut tick.clone());
        }
        processing
    }

    #[test]
    fn high_falls_back_to_standard_when_it_takes_too_long() {
        // 7 ms of every 10 ms tick, over the 60 % limit.
        let mut processing = high_under_load(Duration::from_millis(7), FALLBACK_TICKS);

        assert_eq!(
            processing.settings().noise_suppression,
            NoiseSuppression::Standard
        );
        assert!(processing.take_fallback());
        assert!(!processing.take_fallback());
    }

    #[test]
    fn high_stays_when_it_keeps_up() {
        let mut processing = high_under_load(Duration::ZERO, FALLBACK_TICKS + 50);

        assert_eq!(
            processing.settings().noise_suppression,
            NoiseSuppression::High
        );
        assert!(!processing.take_fallback());
    }

    #[test]
    fn the_load_watch_needs_two_slow_seconds() {
        let slow = Duration::from_millis(7);
        let mut watch = LoadWatch::default();

        let early = (0..FALLBACK_TICKS - 1).any(|_| watch.record(slow));
        let at_two_seconds = watch.record(slow);

        assert!(!early);
        assert!(at_two_seconds);
    }

    #[test]
    fn the_load_watch_forgives_a_few_slow_ticks() {
        let mut watch = LoadWatch::default();

        let gave_up = (0..3 * FALLBACK_TICKS).any(|index| {
            let took = if index % 4 == 0 { 9 } else { 4 };
            watch.record(Duration::from_millis(took))
        });

        // 5.25 ms on average: busy, but under 6 ms.
        assert!(!gave_up);
    }

    /// The level change of a steady tone at `frequency` through processing.
    fn tone_through(settings: ProcessingSettings, frequency: f32) -> f32 {
        let tone: Vec<f32> = (0..SAMPLE_RATE as usize)
            .map(|i| {
                0.3 * (std::f32::consts::TAU * frequency * i as f32 / SAMPLE_RATE as f32).sin()
            })
            .collect();

        let (out, _) = process(settings, &[], &tone);

        let settled = tone.len() / 2..tone.len();
        dbfs(&out[settled.clone()]) - dbfs(&tone[settled])
    }

    #[test]
    fn the_high_pass_filter_takes_out_rumble_and_keeps_the_voice() {
        for echo_cancellation in [false, true] {
            let settings = ProcessingSettings {
                echo_cancellation,
                ..ONLY_EQUALIZING
            };

            let hum = tone_through(settings, 50.0);
            let low_voice = tone_through(settings, 300.0);
            let voice = tone_through(settings, 1_000.0);

            assert!(
                hum <= -10.0,
                "50 Hz only {hum:.1} dB down (echo cancellation {echo_cancellation})"
            );
            assert!(low_voice > -1.0, "300 Hz lost {low_voice:.1} dB");
            assert!(voice.abs() < 0.5, "1 kHz changed {voice:.1} dB");
        }
    }

    #[test]
    fn the_echo_alone_is_marked() {
        let played = at_level(&speech(), -20.0);
        let delay = SAMPLE_RATE as usize / 25;
        let mut capture = vec![0.0; played.len()];
        for (i, sample) in played.iter().enumerate() {
            if let Some(slot) = capture.get_mut(i + delay) {
                *slot += 0.316 * sample;
            }
        }
        let mut processing = VoiceProcessing::new(ProcessingSettings::default()).unwrap();

        let heard: Vec<Heard> = capture
            .as_chunks::<TICK>()
            .0
            .iter()
            .enumerate()
            .map(|(index, tick)| {
                processing.render(&played[index * TICK..(index + 1) * TICK]);
                processing.capture(&mut tick.clone())
            })
            .collect();

        // After a second for the echo canceller to find the echo.
        let settled = &heard[100..];
        let marked = settled.iter().filter(|heard| heard.echo_only).count();
        assert!(
            marked * 10 >= settled.len() * 9,
            "{marked} of {} marked",
            settled.len()
        );
    }

    #[test]
    fn high_is_the_first_run_choice_only_when_it_is_light_enough() {
        assert_eq!(choose(Some(0.05)), NoiseSuppression::High);
        assert_eq!(choose(Some(0.25)), NoiseSuppression::Standard);
        assert_eq!(choose(None), NoiseSuppression::Standard, "High cannot run");
    }
}
