//! Voice message validation and waveform extraction.

use opencord_common::sound::ogg::OpusFile;

use crate::error::ApiError;

#[derive(Debug, Clone)]
pub struct VoiceInfo {
    pub duration_ms: u32,
    pub waveform: Vec<u8>,
}

pub fn check(bytes: &[u8], max_seconds: u32) -> Result<VoiceInfo, ApiError> {
    let file = OpusFile::read(bytes).map_err(|e| ApiError::voice_message_invalid(e.to_string()))?;

    if file.info.duration_ms > max_seconds * 1000 {
        return Err(ApiError::voice_message_too_long(max_seconds));
    }

    let channels = match file.info.channels {
        1 => opus::Channels::Mono,
        2 => opus::Channels::Stereo,
        other => {
            return Err(ApiError::voice_message_invalid(format!(
                "unsupported channel count: {other}"
            )));
        }
    };

    let mut decoder = opus::Decoder::new(48_000, channels)
        .map_err(|e| ApiError::voice_message_invalid(e.to_string()))?;

    let channel_count = file.info.channels as usize;
    let mut out_buffer = vec![0.0f32; 5_760 * channel_count];
    let mut mono_samples = Vec::new();

    for packet in &file.packets {
        let frames = decoder
            .decode_float(packet, &mut out_buffer, false)
            .map_err(|e| ApiError::voice_message_invalid(e.to_string()))?;

        if channel_count == 1 {
            mono_samples.extend_from_slice(&out_buffer[..frames]);
        } else {
            for frame in out_buffer[..frames * 2].as_chunks::<2>().0 {
                mono_samples.push((frame[0] + frame[1]) * 0.5);
            }
        }
    }

    let pre_skip = file.info.pre_skip as usize;
    let keep = (file.info.duration_ms as usize) * 48;
    let pcm: Vec<f32> = mono_samples.into_iter().skip(pre_skip).take(keep).collect();

    let waveform = opencord_common::waveform::waveform(&pcm, 128);

    Ok(VoiceInfo {
        duration_ms: file.info.duration_ms,
        waveform,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_test_ogg(seconds_tone: f32, seconds_silence: f32) -> Vec<u8> {
        use opencord_common::sound::ogg;
        let mut encoder =
            opus::Encoder::new(48_000, opus::Channels::Mono, opus::Application::Voip).unwrap();
        let tone_samples = (seconds_tone * 48_000.0).round() as usize;
        let silence_samples = (seconds_silence * 48_000.0).round() as usize;
        let total_samples = tone_samples + silence_samples;
        let mut pcm = Vec::with_capacity(total_samples);
        for t in 0..tone_samples {
            let sample = 0.5 * (2.0 * std::f32::consts::PI * 440.0 * (t as f32) / 48_000.0).sin();
            pcm.push(sample);
        }
        pcm.resize(total_samples, 0.0);

        let frame_size = 960;
        let mut packets = Vec::new();
        let mut out = vec![0u8; 1000];
        for chunk in pcm.chunks(frame_size) {
            let mut frame = chunk.to_vec();
            frame.resize(frame_size, 0.0);
            let len = encoder.encode_float(&frame, &mut out).unwrap();
            packets.push(out[..len].to_vec());
        }
        ogg::write(1, 0, &packets, total_samples as u64)
    }

    #[test]
    fn voice_check_measures_and_draws() {
        let bytes = build_test_ogg(1.0, 1.0);
        let info = check(&bytes, 300).unwrap();
        assert!(
            (1960..=2040).contains(&info.duration_ms),
            "duration was {}",
            info.duration_ms
        );
        assert_eq!(info.waveform.len(), 128);
        for (i, &val) in info.waveform[..64].iter().enumerate() {
            assert!(val > 150, "at {i}: expected > 150, got {val}");
        }
        for (i, &val) in info.waveform[118..].iter().enumerate() {
            assert!(val < 20, "at {}: expected < 20, got {val}", 118 + i);
        }

        // Garbage -> VOICE_MESSAGE_INVALID
        let err = check(b"not an ogg file", 300).unwrap_err();
        assert_eq!(err.code, opencord_proto::v1::ErrorCode::VoiceMessageInvalid);

        // Max 1 s -> VOICE_MESSAGE_TOO_LONG
        let err = check(&bytes, 1).unwrap_err();
        assert_eq!(err.code, opencord_proto::v1::ErrorCode::VoiceMessageTooLong);
    }
}
