//! Screen audio sender (Phase 2 plan §10.5, §7.7): encodes captured audio as
//! Opus stereo at 128 kbps in music mode, then passes it on.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::Duration;

use super::pipewire::Captured;
use crate::audio::codec::{CodecError, MAX_PACKET};
use crate::audio::{FRAME, SAMPLE_RATE};

const WAIT: Duration = Duration::from_millis(100);

/// One Opus packet of screen audio.
#[derive(Debug, Clone)]
pub struct Encoded {
    pub payload: Vec<u8>,
    pub position: u64,
}

pub struct Sender {
    commands: mpsc::Sender<Command>,
    thread: Option<JoinHandle<()>>,
}

enum Command {
    Stop,
}

impl Sender {
    pub fn start(
        captured: Receiver<Captured>,
        send: impl FnMut(Encoded) + Send + 'static,
    ) -> Self {
        let (commands, received) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("opencord-screen-audio".to_owned())
            .spawn(move || run(captured, received, send))
            .ok();
        Self { commands, thread }
    }

    pub fn stop(&self) {
        let _ = self.commands.send(Command::Stop);
    }
}

impl Drop for Sender {
    fn drop(&mut self) {
        let (closed, _) = mpsc::channel();
        drop(std::mem::replace(&mut self.commands, closed));
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
        debug_assert_eq!(pcm.len(), FRAME * 2);
        Ok(self.inner.encode_float(pcm, packet)?)
    }
}

fn run(
    captured: Receiver<Captured>,
    commands: Receiver<Command>,
    mut send: impl FnMut(Encoded) + Send + 'static,
) {
    let encoder = match StereoEncoder::new() {
        Ok(encoder) => encoder,
        Err(error) => {
            tracing::warn!(%error, "screen audio encoder could not start");
            return;
        }
    };
    let mut pending = Vec::new();
    let mut encoder = encoder;
    let mut packet = [0u8; MAX_PACKET];

    loop {
        if pending.is_empty() {
            match captured.recv_timeout(WAIT) {
                Ok(c) => {
                    let frames = c.samples.len() / 2;
                    let chunks = frames / FRAME;
                    for i in 0..chunks {
                        let start = i * FRAME;
                        let end = start + FRAME;
                        let left = &c.samples[start..end];
                        let right = if end + FRAME <= c.samples.len() {
                            &c.samples[(frames + start)..(frames + end)]
                        } else {
                            left
                        };
                        let mut stereo = Vec::with_capacity(FRAME * 2);
                        for (l, r) in left.iter().zip(right.iter()) {
                            stereo.push(*l);
                            stereo.push(*r);
                        }
                        let position = c.position + (i * FRAME) as u64;
                        pending.push((stereo, position));
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }

        loop {
            match commands.try_recv() {
                Ok(Command::Stop) => return,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        }

        if let Some((pcm, position)) = pending.first() {
            match encoder.encode(pcm, &mut packet) {
                Ok(size) => {
                    if size > 2 {
                        send(Encoded {
                            payload: packet[..size].to_vec(),
                            position: *position,
                        });
                    }
                    pending.remove(0);
                }
                Err(error) => {
                    tracing::debug!(%error, "screen audio encode failed");
                    pending.remove(0);
                }
            }
        }
    }
}
