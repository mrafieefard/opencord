//! Uploaded songs as the channel hears them: decoded with symphonia,
//! made 48 kHz stereo, and encoded as 20 ms Opus frames like Lavalink's.
//! Everything here blocks; callers run it on blocking threads.

use std::collections::VecDeque;
use std::fs::File;
use std::path::Path;

use audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, StandardTag};
use symphonia::core::units::Time;

/// What the channel hears: 48 kHz stereo.
pub const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: usize = 2;
/// 20 ms at 48 kHz.
pub const FRAME: usize = 960;
const BITRATE: i32 = 128_000;
/// Longer tags are cut.
const TAG_MAX: usize = 200;

/// What a file is, found by decoding all of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probed {
    pub duration_ms: u64,
    pub title: Option<String>,
    pub artist: Option<String>,
}

/// Why a file can't be played.
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("not a sound file this server can play: {0}")]
    Unsupported(String),
    #[error("the file has no sound")]
    NoSound,
    #[error("the file could not be read: {0}")]
    Io(#[from] std::io::Error),
    #[error("the sound could not be encoded: {0}")]
    Encode(#[from] opus::Error),
}

fn unsupported(error: SymphoniaError) -> DecodeError {
    DecodeError::Unsupported(error.to_string())
}

/// A file opened for decoding: its reader, decoder and audio track.
struct Opened {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
}

/// The format reader and audio decoder for the file at `path`.
fn open(path: &Path) -> Result<Opened, DecodeError> {
    let file = File::open(path)?;
    let source = MediaSourceStream::new(Box::new(file), Default::default());
    let format = symphonia::default::get_probe()
        .probe(
            &Hint::new(),
            source,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(unsupported)?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or(DecodeError::NoSound)?;
    let track_id = track.id;
    let parameters = track
        .codec_params
        .as_ref()
        .and_then(|parameters| parameters.audio())
        .ok_or(DecodeError::NoSound)?;
    let decoder = symphonia::default::get_codecs()
        .make_audio_decoder(parameters, &AudioDecoderOptions::default())
        .map_err(unsupported)?;
    Ok(Opened {
        format,
        decoder,
        track_id,
    })
}

fn tag(text: &str) -> Option<String> {
    let text: String = text.trim().chars().take(TAG_MAX).collect();
    (!text.is_empty()).then_some(text)
}

/// Decodes the whole file: its length and tags, and that it plays at all.
pub fn probe(path: &Path) -> Result<Probed, DecodeError> {
    let Opened {
        mut format,
        mut decoder,
        track_id,
    } = open(path)?;
    let (mut title, mut artist) = (None, None);
    if let Some(revision) = format.metadata().skip_to_latest() {
        for entry in &revision.media.tags {
            match &entry.std {
                Some(StandardTag::TrackTitle(text)) if title.is_none() => title = tag(text),
                Some(StandardTag::Artist(text)) if artist.is_none() => artist = tag(text),
                _ => {}
            }
        }
    }
    let mut frames: u64 = 0;
    let mut rate = 0;
    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            // A damaged end is still a song up to there.
            Err(SymphoniaError::IoError(_)) if frames > 0 => break,
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
        let channels = decoded.spec().channels().count().max(1);
        rate = decoded.spec().rate();
        frames += (decoded.samples_interleaved() / channels) as u64;
    }
    if frames == 0 || rate == 0 {
        return Err(DecodeError::NoSound);
    }
    Ok(Probed {
        duration_ms: frames * 1_000 / u64::from(rate),
        title,
        artist,
    })
}

/// Stereo audio from the file's rate to 48 kHz, any amount at a time.
struct Resample {
    resampler: Fft<f32>,
    /// Frames per chunk the resampler takes.
    chunk: usize,
    pending: Vec<f32>,
    output: Vec<f32>,
}

impl Resample {
    fn new(from: u32) -> Result<Self, DecodeError> {
        let chunk = (from / 100).max(1) as usize;
        let resampler = Fft::<f32>::new(
            from as usize,
            SAMPLE_RATE as usize,
            chunk,
            CHANNELS,
            FixedSync::Input,
        )
        .map_err(|error| DecodeError::Unsupported(error.to_string()))?;
        let output = vec![0.0; resampler.output_frames_max() * CHANNELS];
        Ok(Self {
            resampler,
            chunk,
            pending: Vec::with_capacity(chunk * CHANNELS),
            output,
        })
    }

    fn process(&mut self, stereo: &[f32], out: &mut VecDeque<f32>) {
        for sample in stereo {
            self.pending.push(*sample);
            if self.pending.len() < self.chunk * CHANNELS {
                continue;
            }
            let frames_out = self.output.len() / CHANNELS;
            let input = InterleavedSlice::new(&self.pending[..], CHANNELS, self.chunk);
            let output = InterleavedSlice::new_mut(&mut self.output[..], CHANNELS, frames_out);
            if let (Ok(input), Ok(mut output)) = (input, output)
                && let Ok((_, produced)) =
                    self.resampler
                        .process_into_buffer(&input, &mut output, None)
            {
                out.extend(&self.output[..produced * CHANNELS]);
            }
            self.pending.clear();
        }
    }
}

/// A song being played: Opus frames from a position on.
pub struct SongDecoder {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    /// Made for the file's rate once it is known.
    resample: Option<Option<Resample>>,
    /// 48 kHz stereo samples waiting to make up a frame.
    ready: VecDeque<f32>,
    encoder: opus::Encoder,
    decoded: Vec<f32>,
    stereo: Vec<f32>,
    ended: bool,
}

impl SongDecoder {
    /// Opens the file at `path` and seeks to `position_ms`.
    pub fn open(path: &Path, position_ms: u64) -> Result<Self, DecodeError> {
        let Opened {
            mut format,
            decoder,
            track_id,
        } = open(path)?;
        if position_ms > 0 {
            let time = Time::try_from_secs_f64(position_ms as f64 / 1_000.0).unwrap_or(Time::ZERO);
            // A file that can't seek plays from the start.
            let _ = format.seek(
                SeekMode::Coarse,
                SeekTo::Time {
                    time,
                    track_id: Some(track_id),
                },
            );
        }
        let mut encoder = opus::Encoder::new(
            SAMPLE_RATE,
            opus::Channels::Stereo,
            opus::Application::Audio,
        )?;
        encoder.set_bitrate(opus::Bitrate::Bits(BITRATE))?;
        encoder.set_complexity(8)?;
        Ok(Self {
            format,
            decoder,
            track_id,
            resample: None,
            ready: VecDeque::with_capacity(8 * FRAME * CHANNELS),
            encoder,
            decoded: Vec::new(),
            stereo: Vec::new(),
            ended: false,
        })
    }

    /// The next 20 ms as Opus; `None` once the song ended.
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>, DecodeError> {
        while self.ready.len() < FRAME * CHANNELS && !self.ended {
            self.decode_more()?;
        }
        if self.ready.is_empty() {
            return Ok(None);
        }
        let mut pcm: Vec<f32> = self
            .ready
            .drain(..self.ready.len().min(FRAME * CHANNELS))
            .collect();
        // The last frame is filled out with silence.
        pcm.resize(FRAME * CHANNELS, 0.0);
        let mut packet = vec![0u8; 4_000];
        let size = self.encoder.encode_float(&pcm, &mut packet)?;
        packet.truncate(size);
        Ok(Some(packet))
    }

    /// Decodes one more packet into `ready`, or marks the end.
    fn decode_more(&mut self) -> Result<(), DecodeError> {
        let packet = match self.format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) | Err(SymphoniaError::IoError(_)) => {
                self.ended = true;
                return Ok(());
            }
            Err(error) => return Err(unsupported(error)),
        };
        if packet.track_id != self.track_id {
            return Ok(());
        }
        let decoded = match self.decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(SymphoniaError::DecodeError(_)) => return Ok(()),
            Err(error) => return Err(unsupported(error)),
        };
        let rate = decoded.spec().rate();
        let channels = decoded.spec().channels().count().max(1);
        self.decoded.resize(decoded.samples_interleaved(), 0.0);
        decoded.copy_to_slice_interleaved(&mut self.decoded);
        self.stereo.clear();
        for frame in self.decoded.chunks_exact(channels) {
            let left = frame[0];
            let right = if channels > 1 { frame[1] } else { left };
            self.stereo.push(left);
            self.stereo.push(right);
        }
        if self.resample.is_none() {
            self.resample = Some(
                (rate != SAMPLE_RATE)
                    .then(|| Resample::new(rate))
                    .transpose()?,
            );
        }
        match self.resample.as_mut().and_then(Option::as_mut) {
            Some(resample) => resample.process(&self.stereo, &mut self.ready),
            None => self.ready.extend(&self.stereo),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 16-bit PCM WAV file of a 440 Hz tone.
    pub(crate) fn wav(rate: u32, channels: u16, seconds: f32) -> Vec<u8> {
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

    fn written(bytes: &[u8]) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), bytes).unwrap();
        file
    }

    #[test]
    fn a_probe_finds_the_length_by_decoding() {
        let file = written(&wav(44_100, 1, 2.5));
        let probed = probe(file.path()).unwrap();
        assert_eq!(probed.duration_ms, 2_500);
        assert_eq!(probed.title, None);
    }

    #[test]
    fn what_is_not_sound_is_refused() {
        let file = written(b"just some text, not a song at all");
        assert!(matches!(
            probe(file.path()),
            Err(DecodeError::Unsupported(_))
        ));
    }

    #[test]
    fn a_song_at_another_rate_comes_out_as_20_ms_frames_of_its_length() {
        let file = written(&wav(44_100, 1, 1.0));
        let mut decoder = SongDecoder::open(file.path(), 0).unwrap();
        let mut frames = 0;
        while let Some(packet) = decoder.next_frame().unwrap() {
            assert!(!packet.is_empty());
            frames += 1;
        }
        // One second is 50 frames; the resampler may hold back a little.
        assert!((48..=51).contains(&frames), "{frames} frames");
    }

    #[test]
    fn opening_at_a_position_skips_what_came_before() {
        let file = written(&wav(48_000, 2, 2.0));
        let mut decoder = SongDecoder::open(file.path(), 1_000).unwrap();
        let mut frames = 0;
        while decoder.next_frame().unwrap().is_some() {
            frames += 1;
        }
        assert!((49..=51).contains(&frames), "{frames} frames");
    }
}
