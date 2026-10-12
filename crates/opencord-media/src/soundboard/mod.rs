//! Soundboard audio (Phase 2 plan §11.3, §11.5): a sound prepared for
//! upload (normalized to EBU R128 −18 LUFS, encoded to Ogg Opus at
//! 96 kbps) and a stored sound decoded for playing.

pub mod defaults;
pub mod input;

use opencord_common::sound::ogg::{self, OggError, OpusFile};
use opencord_common::sound::{NORMALIZE_LUFS, UPLOAD_BITRATE};

use crate::audio::convert::ConvertError;
use crate::audio::{FRAME, SAMPLE_RATE};

/// Opus allows no longer packet than 120 ms.
const MAX_PACKET_SAMPLES: usize = 5_760;
/// Normalizing never lets the true peak go above this.
const PEAK_CEILING_DB: f64 = -1.0;

#[derive(Debug, thiserror::Error)]
pub enum SoundError {
    #[error(transparent)]
    File(#[from] OggError),
    #[error("Opus: {0}")]
    Opus(#[from] opus::Error),
    #[error("loudness measurement: {0}")]
    Loudness(String),
    #[error("a sound has one or two channels, not {0}")]
    Channels(u8),
    #[error("the file is larger than 10 MB")]
    TooLarge,
    #[error("this kind of file can't be used: {0}")]
    Unsupported(String),
    #[error("the file has no sound in it")]
    Empty,
    #[error(transparent)]
    Convert(#[from] ConvertError),
}

/// Interleaved samples at 48 kHz.
#[derive(Debug, Clone, PartialEq)]
pub struct Pcm {
    pub channels: u8,
    pub samples: Vec<f32>,
}

impl Pcm {
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels.max(1))
    }

    pub fn duration_ms(&self) -> u32 {
        let frames = u64::try_from(self.frames()).unwrap_or(u64::MAX);
        u32::try_from(frames * 1_000 / u64::from(SAMPLE_RATE)).unwrap_or(u32::MAX)
    }

    /// The frames from `start_ms` for `length_ms`.
    pub fn trimmed(&self, start_ms: u32, length_ms: u32) -> Self {
        let channels = usize::from(self.channels);
        let frame = |ms: u32| (u64::from(ms) * u64::from(SAMPLE_RATE) / 1_000) as usize;
        let start = frame(start_ms).min(self.frames());
        let end = (start + frame(length_ms)).min(self.frames());
        Self {
            channels: self.channels,
            samples: self.samples[start * channels..end * channels].to_vec(),
        }
    }

    /// Scales by `gain` (a factor).
    pub fn scaled(&self, gain: f32) -> Self {
        Self {
            channels: self.channels,
            samples: self.samples.iter().map(|sample| sample * gain).collect(),
        }
    }
}

/// The gain that brings `pcm` to −18 LUFS without its true peak passing
/// −1 dBTP. Sounds too short or quiet to measure keep their level.
pub fn normalizing_gain(pcm: &Pcm) -> Result<f32, SoundError> {
    use ebur128::{EbuR128, Mode};
    let channels = u32::from(pcm.channels);
    let mut meter = EbuR128::new(channels, SAMPLE_RATE, Mode::I | Mode::TRUE_PEAK)
        .map_err(|error| SoundError::Loudness(error.to_string()))?;
    meter
        .add_frames_f32(&pcm.samples)
        .map_err(|error| SoundError::Loudness(error.to_string()))?;
    let loudness = meter
        .loudness_global()
        .map_err(|error| SoundError::Loudness(error.to_string()))?;
    if !loudness.is_finite() {
        return Ok(1.0);
    }
    let mut peak = 0.0f64;
    for channel in 0..channels {
        let channel_peak = meter
            .true_peak(channel)
            .map_err(|error| SoundError::Loudness(error.to_string()))?;
        peak = peak.max(channel_peak);
    }
    let mut gain_db = NORMALIZE_LUFS - loudness;
    if peak > 0.0 {
        let peak_db = 20.0 * peak.log10();
        gain_db = gain_db.min(PEAK_CEILING_DB - peak_db);
    }
    Ok(10f64.powf(gain_db / 20.0) as f32)
}

/// An Ogg Opus file of `pcm`, at the upload bitrate in 20 ms packets.
pub fn encode(pcm: &Pcm) -> Result<Vec<u8>, SoundError> {
    let channels = match pcm.channels {
        1 => opus::Channels::Mono,
        2 => opus::Channels::Stereo,
        other => return Err(SoundError::Channels(other)),
    };
    let mut encoder = opus::Encoder::new(SAMPLE_RATE, channels, opus::Application::Audio)?;
    encoder.set_bitrate(opus::Bitrate::Bits(UPLOAD_BITRATE))?;
    encoder.set_complexity(10)?;
    let pre_skip = u16::try_from(encoder.get_lookahead()?).unwrap_or(0);
    let width = usize::from(pcm.channels);
    // The encoder runs pre-skip samples behind: pad so the end comes out.
    let total = pcm.frames() + usize::from(pre_skip);
    let mut input = pcm.samples.clone();
    input.resize(total.div_ceil(FRAME) * FRAME * width, 0.0);
    let mut packet = vec![0u8; 4_000];
    let mut packets = Vec::new();
    for frame in input.chunks_exact(FRAME * width) {
        let size = encoder.encode_float(frame, &mut packet)?;
        packets.push(packet[..size].to_vec());
    }
    let samples = u64::try_from(pcm.frames()).unwrap_or(u64::MAX);
    Ok(ogg::write(pcm.channels, pre_skip, &packets, samples))
}

/// What is uploaded for the window `start_ms`..`start_ms + length_ms` of
/// `pcm`: normalized to −18 LUFS when asked (plan §11.3), encoded.
pub fn prepare_upload(
    pcm: &Pcm,
    start_ms: u32,
    length_ms: u32,
    normalize: bool,
) -> Result<Vec<u8>, SoundError> {
    let window = pcm.trimmed(start_ms, length_ms);
    if window.frames() == 0 {
        return Err(SoundError::Empty);
    }
    let window = if normalize {
        window.scaled(normalizing_gain(&window)?)
    } else {
        window
    };
    encode(&window)
}

/// A stored sound's samples, pre-skip dropped and the end trimmed to the
/// final granule position.
pub fn decode(file: &[u8]) -> Result<Pcm, SoundError> {
    decode_packets(&OpusFile::read(file)?)
}

/// A stored sound as the mixer plays it: mono.
pub fn decode_mono(file: &[u8]) -> Result<Vec<f32>, SoundError> {
    let pcm = decode(file)?;
    let width = usize::from(pcm.channels.max(1));
    Ok(pcm
        .samples
        .chunks_exact(width)
        .map(|frame| frame.iter().sum::<f32>() / width as f32)
        .collect())
}

fn decode_packets(file: &OpusFile) -> Result<Pcm, SoundError> {
    let channels = match file.info.channels {
        1 => opus::Channels::Mono,
        _ => opus::Channels::Stereo,
    };
    let width = usize::from(file.info.channels);
    let mut decoder = opus::Decoder::new(SAMPLE_RATE, channels)?;
    let mut out = vec![0f32; MAX_PACKET_SAMPLES * width];
    let mut samples = Vec::new();
    for packet in &file.packets {
        let frames = decoder.decode_float(packet, &mut out, false)?;
        samples.extend_from_slice(&out[..frames * width]);
    }
    let skip = usize::from(file.info.pre_skip) * width;
    let end = usize::try_from(file.final_granule).unwrap_or(usize::MAX) * width;
    let samples = samples
        .get(skip..end.min(samples.len()))
        .unwrap_or_default()
        .to_vec();
    Ok(Pcm {
        channels: file.info.channels,
        samples,
    })
}

/// The loudest sample in each of `buckets` equal slices, for drawing a
/// waveform.
pub fn waveform(pcm: &Pcm, buckets: usize) -> Vec<f32> {
    let width = usize::from(pcm.channels.max(1));
    let frames = pcm.frames();
    if buckets == 0 || frames == 0 {
        return vec![0.0; buckets];
    }
    (0..buckets)
        .map(|bucket| {
            let start = bucket * frames / buckets;
            let end = ((bucket + 1) * frames / buckets).max(start + 1).min(frames);
            pcm.samples[start * width..end * width]
                .iter()
                .fold(0f32, |peak, sample| peak.max(sample.abs()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(seconds: f32, amplitude: f32, channels: u8) -> Pcm {
        let frames = (seconds * SAMPLE_RATE as f32) as usize;
        let samples = (0..frames)
            .flat_map(|n| {
                let t = n as f32 / SAMPLE_RATE as f32;
                let value = amplitude * (2.0 * std::f32::consts::PI * 440.0 * t).sin();
                std::iter::repeat_n(value, usize::from(channels))
            })
            .collect();
        Pcm { channels, samples }
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn a_sound_survives_encoding_and_keeps_its_length() {
        for channels in [1, 2] {
            let original = tone(1.37, 0.3, channels);
            let file = encode(&original).unwrap();
            let read = OpusFile::read(&file).unwrap();
            let decoded = decode(&file).unwrap();

            assert_eq!(read.info.duration_ms, 1_370);
            assert_eq!(decoded.channels, channels);
            assert_eq!(decoded.frames(), original.frames());
            let middle = decoded.samples.len() / 4..decoded.samples.len() * 3 / 4;
            let level = rms(&decoded.samples[middle.clone()]) / rms(&original.samples[middle]);
            assert!((0.9..1.1).contains(&level), "level ratio {level}");
        }
    }

    #[test]
    fn five_seconds_at_96_kbps_fits_the_file_limit() {
        let file = encode(&tone(5.0, 0.5, 2)).unwrap();

        assert!(
            file.len() <= opencord_common::sound::MAX_FILE_BYTES,
            "{}",
            file.len()
        );
    }

    #[test]
    fn normalizing_reaches_minus_18_lufs_or_the_peak_ceiling() {
        let quiet = tone(2.0, 0.01, 1);
        let gain = normalizing_gain(&quiet).unwrap();
        let louder = quiet.scaled(gain);
        let after = normalizing_gain(&louder).unwrap();
        // Brought up; measuring again asks for no further change.
        assert!(gain > 10.0, "gain {gain}");
        assert!((after - 1.0).abs() < 0.05, "second gain {after}");

        // A full-scale square wave is louder than −18 LUFS and is brought
        // down, its peak under −1 dBFS.
        let square = Pcm {
            channels: 1,
            samples: (0..96_000)
                .map(|n| if n / 50 % 2 == 0 { 1.0 } else { -1.0 })
                .collect(),
        };
        let down = normalizing_gain(&square).unwrap();
        assert!(down < 0.89, "gain {down}");
    }

    #[test]
    fn trimming_takes_the_chosen_window() {
        let pcm = tone(3.0, 0.3, 2);
        let window = pcm.trimmed(500, 2_000);

        assert_eq!(window.channels, 2);
        assert_eq!(window.duration_ms(), 2_000);
        assert_eq!(window.samples[..2], pcm.samples[24_000 * 2..24_000 * 2 + 2]);
        // A window past the end stops at the end.
        assert_eq!(pcm.trimmed(2_500, 2_000).duration_ms(), 500);
    }

    #[test]
    fn the_waveform_shows_where_the_sound_is() {
        let mut pcm = tone(1.0, 0.0, 1);
        pcm.samples[30_000] = 0.8;
        let peaks = waveform(&pcm, 10);

        assert_eq!(peaks.len(), 10);
        assert_eq!(peaks[6], 0.8);
        assert_eq!(peaks.iter().filter(|peak| **peak > 0.0).count(), 1);
    }
}
