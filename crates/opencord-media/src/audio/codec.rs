//! Opus for voice (plan §7.7): 48 kHz mono, 20 ms frames, VoIP mode,
//! complexity 10, VBR, in-band FEC and DTX.

use opus::{Application, Bitrate, Channels};

use super::{FRAME, SAMPLE_RATE};

/// Room for any Opus packet this encoder makes.
pub const MAX_PACKET: usize = 1_500;
/// Room for the longest Opus packet, 120 ms.
pub const MAX_DECODED: usize = 6 * FRAME;
/// Expected loss until receiver reports say otherwise; enough for the
/// encoder to include FEC.
const DEFAULT_EXPECTED_LOSS: i32 = 10;

#[derive(Debug, thiserror::Error)]
#[error("Opus: {0}")]
pub struct CodecError(#[from] opus::Error);

/// A packet this small is silence the encoder chose not to describe (DTX),
/// and is not worth sending.
pub fn is_dtx(size: usize) -> bool {
    size <= 2
}

pub struct VoiceEncoder {
    inner: opus::Encoder,
}

impl VoiceEncoder {
    /// `bitrate` in bits per second.
    pub fn new(bitrate: u32) -> Result<Self, CodecError> {
        let mut inner = opus::Encoder::new(SAMPLE_RATE, Channels::Mono, Application::Voip)?;
        inner.set_complexity(10)?;
        inner.set_vbr(true)?;
        inner.set_inband_fec(true)?;
        inner.set_packet_loss_perc(DEFAULT_EXPECTED_LOSS)?;
        inner.set_dtx(true)?;
        let mut encoder = Self { inner };
        encoder.set_bitrate(bitrate)?;
        Ok(encoder)
    }

    pub fn set_bitrate(&mut self, bitrate: u32) -> Result<(), CodecError> {
        let bits = i32::try_from(bitrate).unwrap_or(i32::MAX);
        Ok(self.inner.set_bitrate(Bitrate::Bits(bits))?)
    }

    /// The loss the receivers report, in percent: how much FEC to include.
    pub fn set_expected_loss(&mut self, percent: u8) -> Result<(), CodecError> {
        Ok(self
            .inner
            .set_packet_loss_perc(i32::from(percent.min(100)))?)
    }

    /// Forgets the previous talk spurt.
    pub fn reset(&mut self) -> Result<(), CodecError> {
        Ok(self.inner.reset_state()?)
    }

    /// Encodes one 20 ms frame into `packet`; returns its size.
    pub fn encode(&mut self, pcm: &[f32], packet: &mut [u8]) -> Result<usize, CodecError> {
        debug_assert_eq!(pcm.len(), FRAME);
        Ok(self.inner.encode_float(pcm, packet)?)
    }
}

pub struct VoiceDecoder {
    inner: opus::Decoder,
}

impl VoiceDecoder {
    pub fn new() -> Result<Self, CodecError> {
        Ok(Self {
            inner: opus::Decoder::new(SAMPLE_RATE, Channels::Mono)?,
        })
    }

    /// Decodes a packet into `pcm`, which has room for [`MAX_DECODED`]
    /// samples; returns how many it wrote.
    pub fn decode(&mut self, packet: &[u8], pcm: &mut [f32]) -> Result<usize, CodecError> {
        Ok(self.inner.decode_float(packet, pcm, false)?)
    }

    /// The frame before `next_packet`, which was lost, from the forward
    /// error correction `next_packet` carries (or concealed when it has
    /// none).
    pub fn recover(&mut self, next_packet: &[u8], pcm: &mut [f32]) -> Result<usize, CodecError> {
        Ok(self
            .inner
            .decode_float(next_packet, &mut pcm[..FRAME], true)?)
    }

    /// A frame to stand in for one that is missing.
    pub fn conceal(&mut self, pcm: &mut [f32]) -> Result<usize, CodecError> {
        Ok(self.inner.decode_float(&[], &mut pcm[..FRAME], false)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{FRAME, SAMPLE_RATE};

    fn tone(frame: usize, amplitude: f32) -> Vec<f32> {
        (0..FRAME)
            .map(|i| {
                let t = (frame * FRAME + i) as f32 / SAMPLE_RATE as f32;
                amplitude * (std::f32::consts::TAU * 440.0 * t).sin()
            })
            .collect()
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn a_tone_survives_encoding_and_decoding() {
        let mut encoder = VoiceEncoder::new(64_000).unwrap();
        let mut decoder = VoiceDecoder::new().unwrap();
        let mut packet = [0u8; MAX_PACKET];
        let mut decoded = vec![0f32; MAX_DECODED];

        let mut last = 0.0;
        for frame in 0..10 {
            let input = tone(frame, 0.1);
            let size = encoder.encode(&input, &mut packet).unwrap();
            let samples = decoder.decode(&packet[..size], &mut decoded).unwrap();
            assert_eq!(samples, FRAME);
            last = rms(&decoded[..samples]);
        }

        let expected = rms(&tone(9, 0.1));
        assert!(
            (last / expected - 1.0).abs() < 0.3,
            "{last} against {expected}"
        );
    }

    #[test]
    fn silence_turns_into_packets_too_small_to_send() {
        let mut encoder = VoiceEncoder::new(64_000).unwrap();
        let mut packet = [0u8; MAX_PACKET];
        let silence = vec![0f32; FRAME];

        let sizes: Vec<usize> = (0..50)
            .map(|_| encoder.encode(&silence, &mut packet).unwrap())
            .collect();

        // After 200 ms of silence, only a comfort noise update every 400 ms
        // is worth sending.
        let worth_sending = sizes[10..].iter().filter(|&&size| !is_dtx(size)).count();
        assert!(worth_sending <= 2, "{sizes:?}");
    }

    #[test]
    fn a_lost_frame_comes_back_from_the_next_packet_or_is_concealed() {
        let mut encoder = VoiceEncoder::new(32_000).unwrap();
        let mut decoder = VoiceDecoder::new().unwrap();
        let mut packets = Vec::new();
        for frame in 0..6 {
            let mut packet = [0u8; MAX_PACKET];
            let size = encoder.encode(&tone(frame, 0.2), &mut packet).unwrap();
            packets.push(packet[..size].to_vec());
        }
        let mut decoded = vec![0f32; MAX_DECODED];
        for packet in &packets[..3] {
            decoder.decode(packet, &mut decoded).unwrap();
        }

        let recovered = decoder.recover(&packets[4], &mut decoded).unwrap();
        let after = decoder.decode(&packets[4], &mut decoded).unwrap();
        let concealed = decoder.conceal(&mut decoded).unwrap();

        assert_eq!(recovered, FRAME);
        assert_eq!(after, FRAME);
        assert_eq!(concealed, FRAME);
    }

    #[test]
    fn the_bitrate_follows_the_channel() {
        let mut encoder = VoiceEncoder::new(64_000).unwrap();
        let mut packet = [0u8; MAX_PACKET];
        let loud = |frame| tone(frame, 0.3);

        let at_64: usize = (0..25)
            .map(|frame| encoder.encode(&loud(frame), &mut packet).unwrap())
            .sum();
        encoder.set_bitrate(16_000).unwrap();
        let at_16: usize = (25..50)
            .map(|frame| encoder.encode(&loud(frame), &mut packet).unwrap())
            .sum();

        assert!(
            at_16 * 2 < at_64,
            "{at_16} bytes at 16 kbps, {at_64} at 64 kbps"
        );
    }
}
