//! Files someone picks to make a sound from (plan §11.2, §11.3): MP3, WAV,
//! FLAC, Ogg Vorbis or Opus and M4A/AAC up to 10 MB, decoded to 48 kHz mono
//! for the trim editor. Symphonia has no Opus decoder, so Ogg Opus goes
//! through our own reader and libopus.

use std::io::Cursor;

use opencord_common::sound::MAX_INPUT_BYTES;
use opencord_common::sound::ogg::OpusFile;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use super::{Pcm, SoundError, decode_packets};
use crate::audio::convert::FromDevice;
use crate::audio::{SAMPLE_RATE, TICK};

/// Any accepted file, as 48 kHz mono.
pub fn decode_input(bytes: Vec<u8>) -> Result<Pcm, SoundError> {
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(SoundError::TooLarge);
    }
    let pcm = match OpusFile::read_any_rate(&bytes) {
        Ok(file) => decode_packets(&file)?,
        Err(_) => {
            let (samples, rate, channels) = decode_other(bytes)?;
            return Ok(Pcm {
                channels: 1,
                samples: to_48k_mono(&samples, rate, channels)?,
            });
        }
    };
    Ok(downmixed(pcm))
}

/// Interleaved samples, their rate and channels.
fn decode_other(bytes: Vec<u8>) -> Result<(Vec<f32>, u32, u16), SoundError> {
    let unsupported = |error: SymphoniaError| SoundError::Unsupported(error.to_string());
    let source = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
    let mut format = symphonia::default::get_probe()
        .probe(
            &Hint::new(),
            source,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(unsupported)?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| SoundError::Unsupported("the file has no sound".to_owned()))?;
    let track_id = track.id;
    let parameters = track
        .codec_params
        .as_ref()
        .and_then(|parameters| parameters.audio())
        .ok_or_else(|| SoundError::Unsupported("the file has no sound".to_owned()))?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(parameters, &AudioDecoderOptions::default())
        .map_err(unsupported)?;
    let mut samples = Vec::new();
    let mut spec = None;
    let mut buffer = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            // A damaged end is still a sound up to there.
            Err(SymphoniaError::IoError(_)) if !samples.is_empty() => break,
            Err(error) => return Err(unsupported(error)),
        };
        if packet.track_id != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(error) => return Err(unsupported(error)),
        };
        let rate = decoded.spec().rate();
        let channels = u16::try_from(decoded.spec().channels().count()).unwrap_or(0);
        if *spec.get_or_insert((rate, channels)) != (rate, channels) {
            return Err(SoundError::Unsupported(
                "the file changes format partway".to_owned(),
            ));
        }
        buffer.resize(decoded.samples_interleaved(), 0.0);
        decoded.copy_to_slice_interleaved(&mut buffer);
        samples.extend_from_slice(&buffer);
    }
    let (rate, channels) = spec.ok_or(SoundError::Empty)?;
    Ok((samples, rate, channels))
}

/// Resampled to 48 kHz and averaged to one channel.
fn to_48k_mono(samples: &[f32], rate: u32, channels: u16) -> Result<Vec<f32>, SoundError> {
    let mut convert = FromDevice::new(rate, channels)?;
    let width = usize::from(channels);
    let frames = samples.len() / width;
    let wanted = (frames as u64 * u64::from(SAMPLE_RATE) / u64::from(rate.max(1))) as usize;
    convert.push(&samples[..frames * width]);
    // Silence after the end lets the resampler give out its last samples.
    let flush = (rate as usize / 10 + convert.delay()) * width;
    convert.push(&vec![0.0; flush]);
    let mut out = Vec::with_capacity(wanted + 2 * TICK);
    let mut tick = [0.0; TICK];
    while convert.pop_tick(&mut tick) {
        out.extend_from_slice(&tick);
    }
    let start = convert.delay().min(out.len());
    let end = (start + wanted).min(out.len());
    Ok(out[start..end].to_vec())
}

fn downmixed(pcm: Pcm) -> Pcm {
    if pcm.channels <= 1 {
        return pcm;
    }
    let width = usize::from(pcm.channels);
    Pcm {
        channels: 1,
        samples: pcm
            .samples
            .chunks_exact(width)
            .map(|frame| frame.iter().sum::<f32>() / width as f32)
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soundboard::encode;

    /// A 16-bit PCM WAV file of a 440 Hz tone.
    fn wav(rate: u32, channels: u16, seconds: f32) -> Vec<u8> {
        let frames = (rate as f32 * seconds) as usize;
        let mut data = Vec::new();
        for n in 0..frames {
            let t = n as f32 / rate as f32;
            let value = (0.4 * (std::f32::consts::TAU * 440.0 * t).sin() * 32_767.0) as i16;
            for _ in 0..channels {
                data.extend_from_slice(&value.to_le_bytes());
            }
        }
        let block = channels * 2;
        let mut file = b"RIFF".to_vec();
        file.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        file.extend_from_slice(b"WAVEfmt ");
        file.extend_from_slice(&16u32.to_le_bytes());
        file.extend_from_slice(&1u16.to_le_bytes());
        file.extend_from_slice(&channels.to_le_bytes());
        file.extend_from_slice(&rate.to_le_bytes());
        file.extend_from_slice(&(rate * u32::from(block)).to_le_bytes());
        file.extend_from_slice(&block.to_le_bytes());
        file.extend_from_slice(&16u16.to_le_bytes());
        file.extend_from_slice(b"data");
        file.extend_from_slice(&(data.len() as u32).to_le_bytes());
        file.extend_from_slice(&data);
        file
    }

    fn peak(samples: &[f32]) -> f32 {
        samples
            .iter()
            .fold(0f32, |peak, sample| peak.max(sample.abs()))
    }

    #[test]
    fn a_44_khz_stereo_wav_becomes_48_khz_mono_of_the_same_length() {
        let pcm = decode_input(wav(44_100, 2, 2.5)).unwrap();

        assert_eq!(pcm.channels, 1);
        assert_eq!(pcm.duration_ms(), 2_500);
        // The tone starts at once and runs to the end.
        assert!(peak(&pcm.samples[..480]) > 0.1);
        assert!(peak(&pcm.samples[pcm.samples.len() - 480..]) > 0.3);
    }

    #[test]
    fn a_stereo_ogg_opus_file_becomes_mono() {
        let tone = Pcm {
            channels: 2,
            samples: (0..48_000)
                .flat_map(|n| {
                    let value = 0.3 * (std::f32::consts::TAU * 440.0 * n as f32 / 48_000.0).sin();
                    [value, -value]
                })
                .collect(),
        };
        let pcm = decode_input(encode(&tone).unwrap()).unwrap();

        assert_eq!((pcm.channels, pcm.duration_ms()), (1, 1_000));
        // Opposite channels cancel in the downmix.
        assert!(peak(&pcm.samples) < 0.05, "{}", peak(&pcm.samples));
    }

    #[test]
    fn what_is_not_sound_is_refused() {
        assert!(matches!(
            decode_input(b"hello, this is not a sound".to_vec()),
            Err(SoundError::Unsupported(_))
        ));
        assert!(matches!(
            decode_input(vec![0; MAX_INPUT_BYTES + 1]),
            Err(SoundError::TooLarge)
        ));
    }
}
