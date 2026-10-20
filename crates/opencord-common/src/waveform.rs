//! Voice message waveform extraction.

pub const WAVEFORM_LEN: usize = 128;

pub fn level_unit(rms: f32) -> f32 {
    ((20.0 * rms.max(1e-9).log10() + 60.0) / 60.0).clamp(0.0, 1.0)
}

pub fn level_byte(rms: f32) -> u8 {
    (level_unit(rms) * 255.0).round() as u8
}

pub fn waveform(samples: &[f32], len: usize) -> Vec<u8> {
    let n = samples.len();
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        let start = i * n / len;
        let end = (i + 1) * n / len;
        let slice = &samples[start..end];
        if slice.is_empty() {
            out.push(0);
        } else {
            let sum_sq: f32 = slice.iter().map(|&x| x * x).sum();
            let mean_sq = sum_sq / slice.len() as f32;
            let rms = mean_sq.sqrt();
            out.push(level_byte(rms));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_byte_calculations() {
        assert_eq!(level_byte(1.0), 255);
        assert_eq!(level_byte(0.001), 0);
        let b = level_byte(0.0316);
        assert!((127..=129).contains(&b), "got {b}, expected 128 ± 1");
    }

    #[test]
    fn empty_samples_returns_all_zeros() {
        assert_eq!(waveform(&[], 128), vec![0u8; 128]);
    }

    #[test]
    fn half_silence_then_half_sine() {
        let n = 2048;
        let mut samples = vec![0.0f32; n];
        for (idx, sample) in samples[n / 2..].iter_mut().enumerate() {
            let phase = idx as f32 * std::f32::consts::TAU / 32.0;
            *sample = phase.sin();
        }
        let w = waveform(&samples, 128);
        assert_eq!(w.len(), 128);
        assert_eq!(w[0], 0);
        assert!(w[127] > 240, "expected w[127] > 240, got {}", w[127]);
    }
}
