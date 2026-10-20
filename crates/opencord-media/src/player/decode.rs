//! Turning decoded frames into what the outputs take: RGBA pictures turned
//! upright, and stereo 48 kHz sound.

use std::collections::VecDeque;

use audioadapter_buffers::direct::InterleavedSlice;
use ffmpeg_next::format::Sample;
use ffmpeg_next::frame::Audio;
use rubato::{Fft, FixedSync, Resampler};

/// The size that fits `max_side` on the long side, keeping the shape.
pub fn target_video_size(width: u32, height: u32, max_side: u32) -> (u32, u32) {
    let long_side = width.max(height);
    if long_side <= max_side {
        return (width, height);
    }
    let scale = f64::from(max_side) / f64::from(long_side);
    let target_w = ((f64::from(width) * scale).round() as u32).max(2);
    let target_h = ((f64::from(height) * scale).round() as u32).max(2);
    // Even sizes, as the scaler's chroma subsampling wants.
    (target_w & !1, target_h & !1)
}

/// `pixels` (rows of `width` pixels of `bytes` bytes each) turned
/// clockwise by `rotation` degrees: 0, 90, 180 or 270.
pub fn rotate(
    width: u32,
    height: u32,
    pixels: &[u8],
    bytes: usize,
    rotation: i32,
) -> (u32, u32, Vec<u8>) {
    let (w, h) = (width as usize, height as usize);
    let turn = rotation.rem_euclid(360);
    if !matches!(turn, 90 | 180 | 270) {
        return (width, height, pixels.to_vec());
    }
    let mut out = vec![0u8; pixels.len()];
    for y in 0..h {
        for x in 0..w {
            let (to_x, to_y, to_w) = match turn {
                90 => (h - 1 - y, x, h),
                180 => (w - 1 - x, h - 1 - y, w),
                _ => (y, w - 1 - x, h),
            };
            let from = (y * w + x) * bytes;
            let to = (to_y * to_w + to_x) * bytes;
            out[to..to + bytes].copy_from_slice(&pixels[from..from + bytes]);
        }
    }
    if turn == 180 {
        (width, height, out)
    } else {
        (height, width, out)
    }
}

/// A decoded frame as interleaved stereo f32 at its own rate: mono is
/// doubled, and of more than two channels the first two are kept.
pub fn convert_audio_frame(frame: &Audio) -> Result<Vec<f32>, String> {
    let samples = frame.samples();
    let channels = usize::from(frame.channels());
    if samples == 0 || channels == 0 {
        return Ok(Vec::new());
    }
    let (bytes, read): (usize, fn(&[u8]) -> f32) = match frame.format() {
        Sample::U8(_) => (1, |b| (f32::from(b[0]) - 128.0) / 128.0),
        Sample::I16(_) => (2, |b| {
            f32::from(i16::from_ne_bytes([b[0], b[1]])) / 32_768.0
        }),
        Sample::I32(_) => (4, |b| {
            i32::from_ne_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0
        }),
        Sample::F32(_) => (4, |b| f32::from_ne_bytes([b[0], b[1], b[2], b[3]])),
        Sample::F64(_) => (8, |b| {
            f64::from_ne_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]) as f32
        }),
        other => return Err(format!("unsupported sample format {other:?}")),
    };
    let right = usize::from(channels > 1);
    let planar = frame.is_planar();
    let sample = |channel: usize, index: usize| {
        let at = if planar {
            &frame.data(channel)[index * bytes..]
        } else {
            &frame.data(0)[(index * channels + channel) * bytes..]
        };
        read(at)
    };
    let mut stereo = Vec::with_capacity(samples * 2);
    for index in 0..samples {
        stereo.push(sample(0, index));
        stereo.push(sample(right, index));
    }
    Ok(stereo)
}

/// Resamples interleaved stereo to 48 kHz with rubato; 48 kHz passes through.
pub struct StereoResampler {
    resampler: Option<Fft<f32>>,
    chunk_frames: usize,
    pending: Vec<f32>,
    output: Vec<f32>,
}

impl StereoResampler {
    pub fn new(input_rate: u32) -> Result<Self, String> {
        if input_rate == 48_000 {
            return Ok(Self {
                resampler: None,
                chunk_frames: 0,
                pending: Vec::new(),
                output: Vec::new(),
            });
        }
        let chunk_frames = (input_rate / 100).max(1) as usize;
        let resampler = Fft::<f32>::new(
            input_rate as usize,
            48_000,
            chunk_frames,
            2,
            FixedSync::Input,
        )
        .map_err(|e| e.to_string())?;
        let output = vec![0.0f32; resampler.output_frames_max() * 2];
        Ok(Self {
            resampler: Some(resampler),
            chunk_frames,
            pending: Vec::with_capacity(chunk_frames * 2),
            output,
        })
    }

    pub fn process(&mut self, stereo: &[f32], out: &mut VecDeque<f32>) -> Result<(), String> {
        if self.resampler.is_none() {
            out.extend(stereo);
            return Ok(());
        }
        for &sample in stereo {
            self.pending.push(sample);
            if self.pending.len() == self.chunk_frames * 2 {
                self.run_chunk(out)?;
            }
        }
        Ok(())
    }

    /// Pushes out what is held, padded with silence to a whole chunk.
    pub fn flush(&mut self, out: &mut VecDeque<f32>) -> Result<(), String> {
        if self.resampler.is_none() || self.pending.is_empty() {
            return Ok(());
        }
        self.pending.resize(self.chunk_frames * 2, 0.0);
        self.run_chunk(out)
    }

    /// Forgets everything held, for a seek.
    pub fn reset(&mut self) {
        self.pending.clear();
        if let Some(resampler) = &mut self.resampler {
            resampler.reset();
        }
    }

    fn run_chunk(&mut self, out: &mut VecDeque<f32>) -> Result<(), String> {
        let Some(resampler) = &mut self.resampler else {
            return Ok(());
        };
        let frames_out = self.output.len() / 2;
        let input = InterleavedSlice::new(&self.pending[..], 2, self.chunk_frames)
            .map_err(|e| e.to_string())?;
        let mut output = InterleavedSlice::new_mut(&mut self.output[..], 2, frames_out)
            .map_err(|e| e.to_string())?;
        let (_, produced) = resampler
            .process_into_buffer(&input, &mut output, None)
            .map_err(|e| e.to_string())?;
        out.extend(&self.output[..produced * 2]);
        self.pending.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quarter_turn_moves_the_top_left_to_the_top_right() {
        // 2×1: red, green.
        let pixels = [1, 0, 0, 2, 0, 0];
        assert_eq!(rotate(2, 1, &pixels, 3, 90), (1, 2, vec![1, 0, 0, 2, 0, 0]));
        assert_eq!(
            rotate(2, 1, &pixels, 3, 270),
            (1, 2, vec![2, 0, 0, 1, 0, 0])
        );
        assert_eq!(
            rotate(2, 1, &pixels, 3, 180),
            (2, 1, vec![2, 0, 0, 1, 0, 0])
        );
    }

    #[test]
    fn a_second_at_44_1_khz_comes_out_as_a_second_at_48_khz() {
        let mut resampler = StereoResampler::new(44_100).unwrap();
        let mut out = VecDeque::new();
        resampler
            .process(&vec![0.25; 2 * 44_100], &mut out)
            .unwrap();
        resampler.flush(&mut out).unwrap();
        let frames = out.len() / 2;
        assert!((47_500..=48_500).contains(&frames), "{frames} frames");
    }
}
