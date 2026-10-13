//! Screen audio sender (Phase 2 plan §10.5, §7.7): encodes captured audio as
//! Opus stereo at 128 kbps in music mode, then passes it on.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::Captured;
use crate::audio::codec::{CodecError, MAX_PACKET};
use crate::audio::{FRAME, SAMPLE_RATE};

const WAIT: Duration = Duration::from_millis(100);
/// Interleaved stereo.
const CHANNELS: usize = 2;

/// One Opus packet of screen audio.
#[derive(Debug, Clone)]
pub struct Encoded {
    pub payload: Vec<u8>,
    pub position: u64,
    /// When its first sample was captured.
    pub captured: Instant,
}

/// One Opus frame of time.
const FRAME_TIME: Duration = Duration::from_millis(20);

pub struct Sender {
    stop: mpsc::Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl Sender {
    /// Encodes what `captured` brings and hands each packet to `send`;
    /// `heard` sees each buffer first, as captured.
    pub fn start(
        captured: Receiver<Captured>,
        heard: impl FnMut(&[f32]) + Send + 'static,
        send: impl FnMut(Encoded) + Send + 'static,
    ) -> Self {
        let (stop, stopped) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("opencord-screen-audio".to_owned())
            .spawn(move || run(captured, stopped, heard, send))
            .ok();
        Self { stop, thread }
    }

    pub fn stop(&self) {
        let _ = self.stop.send(());
    }
}

impl Drop for Sender {
    fn drop(&mut self) {
        self.stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct StereoEncoder {
    inner: opus::Encoder,
}

impl StereoEncoder {
    fn new() -> Result<Self, CodecError> {
        let mut inner = opus::Encoder::new(
            SAMPLE_RATE,
            opus::Channels::Stereo,
            opus::Application::Audio,
        )?;
        inner.set_complexity(10)?;
        inner.set_bitrate(opus::Bitrate::Bits(128_000))?;
        inner.set_inband_fec(true)?;
        Ok(Self { inner })
    }

    fn encode(&mut self, pcm: &[f32], packet: &mut [u8]) -> Result<usize, CodecError> {
        debug_assert_eq!(pcm.len(), FRAME * CHANNELS);
        Ok(self.inner.encode_float(pcm, packet)?)
    }
}

/// PipeWire hands over buffers of its quantum (256, 1024, ... frames), not
/// Opus frames: samples left over from one buffer wait for the next.
fn run(
    captured: Receiver<Captured>,
    stop: Receiver<()>,
    mut heard: impl FnMut(&[f32]),
    mut send: impl FnMut(Encoded),
) {
    let mut encoder = match StereoEncoder::new() {
        Ok(encoder) => encoder,
        Err(error) => {
            tracing::warn!(%error, "screen audio encoder could not start");
            return;
        }
    };
    let mut packet = [0u8; MAX_PACKET];
    let mut pending: Vec<f32> = Vec::with_capacity(FRAME * CHANNELS * 2);
    // The capture position and time of pending's first frame.
    let mut position = 0;
    let mut captured_at = Instant::now();
    loop {
        if !matches!(stop.try_recv(), Err(TryRecvError::Empty)) {
            return;
        }
        let captured = match captured.recv_timeout(WAIT) {
            Ok(captured) => captured,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        heard(&captured.samples);
        if pending.is_empty() {
            position = captured.position;
            captured_at = captured.captured;
        }
        pending.extend_from_slice(&captured.samples);
        while pending.len() >= FRAME * CHANNELS {
            match encoder.encode(&pending[..FRAME * CHANNELS], &mut packet) {
                // Two bytes or less is Opus DTX: nothing worth sending.
                Ok(size) if size > 2 => send(Encoded {
                    payload: packet[..size].to_vec(),
                    position,
                    captured: captured_at,
                }),
                Ok(_) => {}
                Err(error) => tracing::debug!(%error, "screen audio encode failed"),
            }
            pending.drain(..FRAME * CHANNELS);
            position += FRAME as u64;
            captured_at += FRAME_TIME;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Left a 440 Hz tone, right silent, in buffers the size of a common
    /// PipeWire quantum.
    fn captured(quantum: usize, buffers: usize) -> Vec<Captured> {
        (0..buffers)
            .map(|index| {
                let first = index * quantum;
                let samples = (first..first + quantum)
                    .flat_map(|frame| {
                        let t = frame as f32 / SAMPLE_RATE as f32;
                        [0.5 * (2.0 * std::f32::consts::PI * 440.0 * t).sin(), 0.0]
                    })
                    .collect();
                Captured {
                    samples,
                    position: first as u64,
                    captured: Instant::now(),
                }
            })
            .collect()
    }

    fn encoded(quantum: usize, buffers: usize) -> Vec<Encoded> {
        let (frames, received) = mpsc::sync_channel(buffers);
        for buffer in captured(quantum, buffers) {
            frames.send(buffer).unwrap();
        }
        drop(frames);
        let (_stop, stopped) = mpsc::channel();
        let mut packets = Vec::new();
        run(received, stopped, |_| {}, |packet| packets.push(packet));
        packets
    }

    #[test]
    fn buffers_of_any_quantum_become_whole_opus_frames_in_order() {
        // 40 × 256 = 10 240 frames: ten 960-frame packets, 640 left over.
        let packets = encoded(256, 40);

        let positions: Vec<u64> = packets.iter().map(|packet| packet.position).collect();
        assert_eq!(
            positions,
            (0..10).map(|n| n * FRAME as u64).collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_channels_stay_apart() {
        let packets = encoded(1024, 20);
        let mut decoder = opus::Decoder::new(SAMPLE_RATE, opus::Channels::Stereo).unwrap();
        let mut pcm = vec![0f32; FRAME * CHANNELS];
        let (mut left, mut right) = (0f32, 0f32);
        // The encoder's first packets fade in.
        for packet in &packets[5..] {
            let frames = decoder
                .decode_float(&packet.payload, &mut pcm, false)
                .unwrap();
            for frame in pcm[..frames * CHANNELS].as_chunks::<CHANNELS>().0 {
                left = left.max(frame[0].abs());
                right = right.max(frame[1].abs());
            }
        }

        assert!(left > 0.3, "left peak {left}");
        assert!(right < 0.05, "right peak {right}");
    }
}
