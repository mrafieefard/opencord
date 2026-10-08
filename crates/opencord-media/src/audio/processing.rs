//! The microphone's cleanup (plan §7.2–§7.4), one 10 ms tick at a time at
//! 48 kHz: a high-pass filter and echo cancellation, then noise suppression
//! (off, Standard or High), then automatic gain control, so noise is not
//! amplified. The echo canceller hears everything played.
//!
//! Echo cancellation, the high-pass filter and gain control are the `aec3`
//! crate's Rust port of WebRTC's AudioProcessing (D27); Standard noise
//! suppression is RNNoise through `nnnoiseless`, which also says how
//! likely each tick is to be speech.

use aec3::audio_processing::gain_controller2::{
    GainController2Config, InputVolumeControllerRuntimeConfig,
};
use aec3::graph::{
    GraphBuilder, GraphError, InPort, OutPort, Packet, PacketMeta, QueueConfig, Runtime, Sink,
    Source,
};
use aec3::nodes::audio::{AudioChunk, AudioFormat};
use aec3::nodes::{agc2, hpf};
use aec3::pipelines::linear::{self, LinearPipeline};
use nnnoiseless::DenoiseState;

use super::{SAMPLE_RATE, TICK};

/// RNNoise works on samples in the 16-bit range.
const RNNOISE_SCALE: f32 = 32_768.0;

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

impl From<GraphError> for ProcessingError {
    fn from(error: GraphError) -> Self {
        Self(error.to_string())
    }
}

/// Before noise suppression: the high-pass filter, with or without echo
/// cancellation.
enum Front {
    EchoCancelling(Box<LinearPipeline>),
    Filtering(Stage),
}

pub struct VoiceProcessing {
    settings: ProcessingSettings,
    /// RNNoise looks for a voice even with suppression off, for automatic
    /// sensitivity.
    voice_analysis: bool,
    front: Front,
    gain: Option<Stage>,
    denoise: Box<DenoiseState<'static>>,
    scaled: [f32; TICK],
    cleaned: [f32; TICK],
}

impl VoiceProcessing {
    pub fn new(settings: ProcessingSettings) -> Result<Self, ProcessingError> {
        Ok(Self {
            front: front(settings.echo_cancellation)?,
            gain: settings.automatic_gain.then(Stage::gain).transpose()?,
            settings,
            voice_analysis: false,
            denoise: DenoiseState::new(),
            scaled: [0.0; TICK],
            cleaned: [0.0; TICK],
        })
    }

    pub fn settings(&self) -> ProcessingSettings {
        self.settings
    }

    /// Takes new settings; the parts that changed start over.
    pub fn set_settings(&mut self, settings: ProcessingSettings) -> Result<(), ProcessingError> {
        if settings.echo_cancellation != self.settings.echo_cancellation {
            self.front = front(settings.echo_cancellation)?;
        }
        if settings.automatic_gain != self.settings.automatic_gain {
            self.gain = settings.automatic_gain.then(Stage::gain).transpose()?;
        }
        self.settings = settings;
        Ok(())
    }

    /// Whether to find each tick's voice probability while noise
    /// suppression is off.
    pub fn set_voice_analysis(&mut self, on: bool) {
        self.voice_analysis = on;
    }

    /// A tick of everything played, for the echo canceller.
    pub fn render(&mut self, tick: &[f32]) {
        if let Front::EchoCancelling(pipeline) = &mut self.front {
            let _ = pipeline.handle_render_frame(tick);
        }
    }

    /// Cleans a tick of the microphone in place; returns how likely it is
    /// to be speech, from 0 to 1, while noise suppression or voice analysis
    /// is on.
    pub fn capture(&mut self, tick: &mut [f32]) -> Option<f32> {
        match &mut self.front {
            Front::EchoCancelling(pipeline) => {
                if let Ok(true) = pipeline.process_capture_frame(tick, &mut self.cleaned) {
                    tick.copy_from_slice(&self.cleaned);
                }
            }
            Front::Filtering(stage) => {
                let _ = stage.process(tick);
            }
        }
        let probability = match self.settings.noise_suppression {
            NoiseSuppression::Off if self.voice_analysis => Some(self.analyze(tick)),
            NoiseSuppression::Off => None,
            NoiseSuppression::Standard | NoiseSuppression::High => Some(self.rnnoise(tick)),
        };
        if let Some(gain) = &mut self.gain {
            let _ = gain.process(tick);
        }
        probability
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

fn format() -> AudioFormat {
    AudioFormat::ten_ms(SAMPLE_RATE, 1)
}

fn front(echo_cancellation: bool) -> Result<Front, ProcessingError> {
    Ok(if echo_cancellation {
        let pipeline = linear::builder(format(), format())
            .enable_high_pass_filter(true)
            .enable_noise_suppression(false)
            .enable_gain_controller2(false)
            .enable_post_filter(false)
            .build()?;
        Front::EchoCancelling(Box::new(pipeline))
    } else {
        Front::Filtering(Stage::high_pass()?)
    })
}

/// A graph of one capture-side node: a tick in, a tick out.
struct Stage {
    runtime: Runtime,
    input: Source<AudioChunk>,
    output: Sink<AudioChunk>,
    sequence: u64,
}

impl Stage {
    fn high_pass() -> Result<Self, GraphError> {
        Self::build(|graph| {
            let node = hpf::builder(format()).add_to(graph)?;
            Ok((node.audio_in, node.audio_out))
        })
    }

    fn gain() -> Result<Self, GraphError> {
        // The operating system's microphone volume is not ours to steer.
        let config = GainController2Config {
            input_volume_controller: InputVolumeControllerRuntimeConfig { enabled: false },
            ..GainController2Config::default()
        };
        Self::build(|graph| {
            let node = agc2::builder(format())
                .config(config)
                .with_applied_input_volume(false)
                .with_capture_output_used(false)
                .with_recommended_input_volume(false)
                .add_to(graph)?;
            Ok((node.audio_in, node.audio_out))
        })
    }

    fn build(
        node: impl FnOnce(
            &mut GraphBuilder,
        ) -> Result<(InPort<AudioChunk>, OutPort<AudioChunk>), GraphError>,
    ) -> Result<Self, GraphError> {
        let mut graph = GraphBuilder::new();
        let input = graph.source::<AudioChunk>("in");
        let output = graph.sink::<AudioChunk>("out", QueueConfig::audio_default());
        let (node_in, node_out) = node(&mut graph)?;
        graph.connect(input, node_in)?;
        graph.connect(node_out, output)?;
        Ok(Self {
            runtime: Runtime::new(graph.build()?)?,
            input,
            output,
            sequence: 0,
        })
    }

    fn process(&mut self, tick: &mut [f32]) -> Result<(), GraphError> {
        self.sequence += 1;
        self.runtime.push(
            self.input,
            Packet {
                meta: PacketMeta {
                    sequence: Some(self.sequence),
                    ..PacketMeta::default()
                },
                payload: AudioChunk::from_interleaved(format(), tick),
            },
        )?;
        self.runtime.run_until_stalled()?;
        if let Some(packet) = self.runtime.try_pull(self.output)? {
            tick.copy_from_slice(packet.payload().samples());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::fixtures::{at_level, speech};
    use crate::audio::{SAMPLE_RATE, TICK, dbfs};

    struct Noise(u64);

    impl Noise {
        fn white(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            ((self.0 >> 11) as f64 / (1u64 << 53) as f64) as f32 * 2.0 - 1.0
        }
    }

    /// The noises the plan names, made up: a fan's hum and hiss, keyboard
    /// clicks, street rumble, and barking.
    fn noise(kind: &str, samples: usize) -> Vec<f32> {
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
            probabilities.push(processing.capture(&mut cleaned));
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

    /// The voice over `kind` of noise, after a second of the noise alone;
    /// returns how much quieter the noise got and how the voice changed.
    fn suppress(kind: &str, mode: NoiseSuppression) -> (f32, f32) {
        let voice = at_level(&speech(), -22.0);
        let lead = SAMPLE_RATE as usize;
        let background = at_level(&noise(kind, lead + voice.len()), -35.0);
        let mut capture = background.clone();
        for (i, sample) in voice.iter().enumerate() {
            capture[lead + i] += sample;
        }
        let settings = ProcessingSettings {
            noise_suppression: mode,
            ..ONLY_EQUALIZING
        };

        let (out, _) = process(settings, &[], &capture);

        // The second half of the noise-only lead, once suppression settled.
        let noise_only = lead / 2..lead;
        let reduction = dbfs(&capture[noise_only.clone()]) - dbfs(&out[noise_only]);
        let speaking = lead..capture.len();
        let voice_change = dbfs(&out[speaking.clone()]) - dbfs(&capture[speaking]);
        (reduction, voice_change)
    }

    #[test]
    fn standard_suppression_quiets_steady_noise_and_keeps_the_voice() {
        // RNNoise barely touches barking (about 2 dB here); that is High's
        // job (plan §7.3).
        for (kind, at_least) in [("fan", 20.0), ("street", 20.0), ("keyboard", 6.0)] {
            let (reduction, voice_change) = suppress(kind, NoiseSuppression::Standard);

            assert!(
                reduction >= at_least,
                "{kind}: the noise is only {reduction:.1} dB quieter"
            );
            assert!(
                voice_change > -3.0,
                "{kind}: the voice lost {voice_change:.1} dB"
            );
        }
    }

    #[test]
    fn the_voice_probability_tells_speech_from_noise() {
        let voice = at_level(&speech(), -22.0);
        let lead = SAMPLE_RATE as usize;
        let mut capture = at_level(&noise("fan", lead + voice.len()), -40.0);
        for (i, sample) in voice.iter().enumerate() {
            capture[lead + i] += sample;
        }
        let settings = ProcessingSettings {
            noise_suppression: NoiseSuppression::Standard,
            ..ONLY_EQUALIZING
        };

        let (_, probabilities) = process(settings, &[], &capture);

        let mean = |range: std::ops::Range<usize>| {
            let values: Vec<f32> = probabilities[range].iter().map(|p| p.unwrap()).collect();
            values.iter().sum::<f32>() / values.len() as f32
        };
        let lead_ticks = lead / TICK;
        let noise_only = mean(lead_ticks / 2..lead_ticks);
        let speaking = mean(lead_ticks..probabilities.len());
        assert!(noise_only < 0.3, "noise looks like speech: {noise_only:.2}");
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
}
