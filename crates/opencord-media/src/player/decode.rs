use std::collections::VecDeque;

use audioadapter_buffers::direct::InterleavedSlice;
use ffmpeg_next::format::Sample;
use ffmpeg_next::format::sample::Type as SampleType;
use ffmpeg_next::frame::Audio;
use rubato::{Fft, FixedSync, Resampler};

/// Calculates target dimensions scaled proportionally so that `max(width, height) <= max_side`.
pub fn target_video_size(width: u32, height: u32, max_side: u32) -> (u32, u32) {
    let long_side = width.max(height);
    if long_side <= max_side {
        return (width, height);
    }
    let scale = max_side as f64 / long_side as f64;
    let target_w = ((width as f64 * scale).round() as u32).max(1);
    let target_h = ((height as f64 * scale).round() as u32).max(1);
    // Align to even dimensions for scaling
    (target_w & !1, target_h & !1)
}

/// Rotates an RGBA image buffer by 0, 90, 180, or 270 degrees clockwise.
pub fn rotate_rgba(width: u32, height: u32, rgba: &[u8], rotation: i32) -> (u32, u32, Vec<u8>) {
    let rot = rotation.rem_euclid(360);
    match rot {
        90 => {
            let (w, h) = (width as usize, height as usize);
            let mut out = vec![0u8; w * h * 4];
            for y in 0..h {
                for x in 0..w {
                    let src_idx = (y * w + x) * 4;
                    let dst_x = h - 1 - y;
                    let dst_y = x;
                    let dst_idx = (dst_y * h + dst_x) * 4;
                    out[dst_idx..dst_idx + 4].copy_from_slice(&rgba[src_idx..src_idx + 4]);
                }
            }
            (height, width, out)
        }
        180 => {
            let (w, h) = (width as usize, height as usize);
            let mut out = vec![0u8; w * h * 4];
            for y in 0..h {
                for x in 0..w {
                    let src_idx = (y * w + x) * 4;
                    let dst_x = w - 1 - x;
                    let dst_y = h - 1 - y;
                    let dst_idx = (dst_y * w + dst_x) * 4;
                    out[dst_idx..dst_idx + 4].copy_from_slice(&rgba[src_idx..src_idx + 4]);
                }
            }
            (width, height, out)
        }
        270 => {
            let (w, h) = (width as usize, height as usize);
            let mut out = vec![0u8; w * h * 4];
            for y in 0..h {
                for x in 0..w {
                    let src_idx = (y * w + x) * 4;
                    let dst_x = y;
                    let dst_y = w - 1 - x;
                    let dst_idx = (dst_y * h + dst_x) * 4;
                    out[dst_idx..dst_idx + 4].copy_from_slice(&rgba[src_idx..src_idx + 4]);
                }
            }
            (height, width, out)
        }
        _ => (width, height, rgba.to_vec()),
    }
}

/// Rotates an RGB24 image buffer by 0, 90, 180, or 270 degrees clockwise.
pub fn rotate_rgb(width: u32, height: u32, rgb: &[u8], rotation: i32) -> (u32, u32, Vec<u8>) {
    let rot = rotation.rem_euclid(360);
    match rot {
        90 => {
            let (w, h) = (width as usize, height as usize);
            let mut out = vec![0u8; w * h * 3];
            for y in 0..h {
                for x in 0..w {
                    let src_idx = (y * w + x) * 3;
                    let dst_x = h - 1 - y;
                    let dst_y = x;
                    let dst_idx = (dst_y * h + dst_x) * 3;
                    out[dst_idx..dst_idx + 3].copy_from_slice(&rgb[src_idx..src_idx + 3]);
                }
            }
            (height, width, out)
        }
        180 => {
            let (w, h) = (width as usize, height as usize);
            let mut out = vec![0u8; w * h * 3];
            for y in 0..h {
                for x in 0..w {
                    let src_idx = (y * w + x) * 3;
                    let dst_x = w - 1 - x;
                    let dst_y = h - 1 - y;
                    let dst_idx = (dst_y * w + dst_x) * 3;
                    out[dst_idx..dst_idx + 3].copy_from_slice(&rgb[src_idx..src_idx + 3]);
                }
            }
            (width, height, out)
        }
        270 => {
            let (w, h) = (width as usize, height as usize);
            let mut out = vec![0u8; w * h * 3];
            for y in 0..h {
                for x in 0..w {
                    let src_idx = (y * w + x) * 3;
                    let dst_x = y;
                    let dst_y = w - 1 - x;
                    let dst_idx = (dst_y * h + dst_x) * 3;
                    out[dst_idx..dst_idx + 3].copy_from_slice(&rgb[src_idx..src_idx + 3]);
                }
            }
            (height, width, out)
        }
        _ => (width, height, rgb.to_vec()),
    }
}

/// Converts a decoded FFmpeg audio frame to interleaved stereo f32 samples.
pub fn convert_audio_frame(frame: &Audio) -> Vec<f32> {
    let samples = frame.samples();
    let channels = frame.channels() as usize;
    if samples == 0 || channels == 0 {
        return Vec::new();
    }

    let mut stereo = Vec::with_capacity(samples * 2);
    let format = frame.format();

    match format {
        Sample::U8(SampleType::Planar) => {
            let p0 = frame.data(0);
            let p1 = if channels > 1 { frame.data(1) } else { p0 };
            for s in 0..samples {
                stereo.push((p0[s] as f32 - 128.0) / 128.0);
                stereo.push((p1[s] as f32 - 128.0) / 128.0);
            }
        }
        Sample::U8(SampleType::Packed) => {
            let data = frame.data(0);
            for s in 0..samples {
                let base = s * channels;
                stereo.push((data[base] as f32 - 128.0) / 128.0);
                let r = if channels > 1 {
                    data[base + 1]
                } else {
                    data[base]
                };
                stereo.push((r as f32 - 128.0) / 128.0);
            }
        }
        Sample::I16(SampleType::Planar) => {
            let p0 = frame.data(0);
            let p1 = if channels > 1 { frame.data(1) } else { p0 };
            for s in 0..samples {
                let s0 = i16::from_ne_bytes([p0[s * 2], p0[s * 2 + 1]]);
                let s1 = i16::from_ne_bytes([p1[s * 2], p1[s * 2 + 1]]);
                stereo.push(s0 as f32 / 32768.0);
                stereo.push(s1 as f32 / 32768.0);
            }
        }
        Sample::I16(SampleType::Packed) => {
            let data = frame.data(0);
            for s in 0..samples {
                let base = s * channels * 2;
                let s0 = i16::from_ne_bytes([data[base], data[base + 1]]);
                let s1 = if channels > 1 {
                    i16::from_ne_bytes([data[base + 2], data[base + 3]])
                } else {
                    s0
                };
                stereo.push(s0 as f32 / 32768.0);
                stereo.push(s1 as f32 / 32768.0);
            }
        }
        Sample::I32(SampleType::Planar) => {
            let p0 = frame.data(0);
            let p1 = if channels > 1 { frame.data(1) } else { p0 };
            for s in 0..samples {
                let s0 =
                    i32::from_ne_bytes([p0[s * 4], p0[s * 4 + 1], p0[s * 4 + 2], p0[s * 4 + 3]]);
                let s1 =
                    i32::from_ne_bytes([p1[s * 4], p1[s * 4 + 1], p1[s * 4 + 2], p1[s * 4 + 3]]);
                stereo.push(s0 as f32 / 2147483648.0);
                stereo.push(s1 as f32 / 2147483648.0);
            }
        }
        Sample::I32(SampleType::Packed) => {
            let data = frame.data(0);
            for s in 0..samples {
                let base = s * channels * 4;
                let s0 = i32::from_ne_bytes([
                    data[base],
                    data[base + 1],
                    data[base + 2],
                    data[base + 3],
                ]);
                let s1 = if channels > 1 {
                    i32::from_ne_bytes([
                        data[base + 4],
                        data[base + 5],
                        data[base + 6],
                        data[base + 7],
                    ])
                } else {
                    s0
                };
                stereo.push(s0 as f32 / 2147483648.0);
                stereo.push(s1 as f32 / 2147483648.0);
            }
        }
        Sample::F32(SampleType::Planar) => {
            let p0 = frame.data(0);
            let p1 = if channels > 1 { frame.data(1) } else { p0 };
            for s in 0..samples {
                let s0 =
                    f32::from_ne_bytes([p0[s * 4], p0[s * 4 + 1], p0[s * 4 + 2], p0[s * 4 + 3]]);
                let s1 =
                    f32::from_ne_bytes([p1[s * 4], p1[s * 4 + 1], p1[s * 4 + 2], p1[s * 4 + 3]]);
                stereo.push(s0);
                stereo.push(s1);
            }
        }
        Sample::F32(SampleType::Packed) => {
            let data = frame.data(0);
            for s in 0..samples {
                let base = s * channels * 4;
                let s0 = f32::from_ne_bytes([
                    data[base],
                    data[base + 1],
                    data[base + 2],
                    data[base + 3],
                ]);
                let s1 = if channels > 1 {
                    f32::from_ne_bytes([
                        data[base + 4],
                        data[base + 5],
                        data[base + 6],
                        data[base + 7],
                    ])
                } else {
                    s0
                };
                stereo.push(s0);
                stereo.push(s1);
            }
        }
        Sample::F64(SampleType::Planar) => {
            let p0 = frame.data(0);
            let p1 = if channels > 1 { frame.data(1) } else { p0 };
            for s in 0..samples {
                let s0 = f64::from_ne_bytes([
                    p0[s * 8],
                    p0[s * 8 + 1],
                    p0[s * 8 + 2],
                    p0[s * 8 + 3],
                    p0[s * 8 + 4],
                    p0[s * 8 + 5],
                    p0[s * 8 + 6],
                    p0[s * 8 + 7],
                ]);
                let s1 = f64::from_ne_bytes([
                    p1[s * 8],
                    p1[s * 8 + 1],
                    p1[s * 8 + 2],
                    p1[s * 8 + 3],
                    p1[s * 8 + 4],
                    p1[s * 8 + 5],
                    p1[s * 8 + 6],
                    p1[s * 8 + 7],
                ]);
                stereo.push(s0 as f32);
                stereo.push(s1 as f32);
            }
        }
        Sample::F64(SampleType::Packed) => {
            let data = frame.data(0);
            for s in 0..samples {
                let base = s * channels * 8;
                let s0 = f64::from_ne_bytes([
                    data[base],
                    data[base + 1],
                    data[base + 2],
                    data[base + 3],
                    data[base + 4],
                    data[base + 5],
                    data[base + 6],
                    data[base + 7],
                ]);
                let s1 = if channels > 1 {
                    f64::from_ne_bytes([
                        data[base + 8],
                        data[base + 9],
                        data[base + 10],
                        data[base + 11],
                        data[base + 12],
                        data[base + 13],
                        data[base + 14],
                        data[base + 15],
                    ])
                } else {
                    s0
                };
                stereo.push(s0 as f32);
                stereo.push(s1 as f32);
            }
        }
        _ => {}
    }

    stereo
}

/// Resamples stereo audio to 48 kHz using rubato.
pub struct StereoResampler {
    resampler: Option<Fft<f32>>,
    chunk_frames: usize,
    pending: Vec<f32>,
    output: Vec<f32>,
}

impl StereoResampler {
    pub fn new(input_rate: u32) -> Result<Self, String> {
        if input_rate == 48000 {
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
            48000,
            chunk_frames,
            2,
            FixedSync::Input,
        )
        .map_err(|e| e.to_string())?;

        let output_frames = resampler.output_frames_max();
        let output = vec![0.0f32; output_frames * 2];

        Ok(Self {
            resampler: Some(resampler),
            chunk_frames,
            pending: Vec::with_capacity(chunk_frames * 2),
            output,
        })
    }

    pub fn process(&mut self, stereo_input: &[f32], out: &mut VecDeque<f32>) {
        let Some(resampler) = &mut self.resampler else {
            out.extend(stereo_input);
            return;
        };

        for sample in stereo_input {
            self.pending.push(*sample);
            if self.pending.len() == self.chunk_frames * 2 {
                let frames_out = self.output.len() / 2;
                let input = InterleavedSlice::new(&self.pending[..], 2, self.chunk_frames);
                let output = InterleavedSlice::new_mut(&mut self.output[..], 2, frames_out);
                if let (Ok(input), Ok(mut output)) = (input, output)
                    && let Ok((_, produced)) =
                        resampler.process_into_buffer(&input, &mut output, None)
                {
                    out.extend(&self.output[..produced * 2]);
                }
                self.pending.clear();
            }
        }
    }

    pub fn flush(&mut self, out: &mut VecDeque<f32>) {
        if let Some(resampler) = &mut self.resampler
            && !self.pending.is_empty()
        {
            // Pad remaining frames with silence up to chunk_frames
            self.pending.resize(self.chunk_frames * 2, 0.0);
            let frames_out = self.output.len() / 2;
            let input = InterleavedSlice::new(&self.pending[..], 2, self.chunk_frames);
            let output = InterleavedSlice::new_mut(&mut self.output[..], 2, frames_out);
            if let (Ok(input), Ok(mut output)) = (input, output)
                && let Ok((_, produced)) = resampler.process_into_buffer(&input, &mut output, None)
            {
                out.extend(&self.output[..produced * 2]);
            }
            self.pending.clear();
        }
    }
}
