#![cfg(windows)]
//! What an encode costs on this PC, per layer (docs/plans/windows.md W3,
//! step 8). The Windows answer to `examples/codec_costs`, which is Linux's:
//! that example reads CPU time from `/proc`, which has no equivalent here
//! without a new dependency, so this reports the wall time one `encode` call
//! takes — the number the real-time pipeline actually lives with, since the
//! sender encodes a layer and hands it straight to the transport.
//!
//! Ignored, because it is a measurement and not a pass or a fail:
//!
//!     cargo test -p opencord-media --test codec_costs_windows -- --ignored --nocapture

use std::time::{Duration, Instant};

use opencord_media::video::codec::encoder::{Backend, Encoder, EncoderConfig};
use opencord_media::video::codec::scale::Scaler;
use opencord_media::video::pattern::scene;
use opencord_media::video::picture::PixelFormat;

/// Encodes timed, after ten to let the encoder settle.
const ROUNDS: u32 = 480;
const WARMUP: u32 = 10;

/// The layers the sender uses (plan §7.9): 720p and 360p at 30, 180p at 15.
const LAYERS: [(u32, u32, u32, u32); 3] = [
    (1280, 720, 30, 1_500_000),
    (640, 360, 30, 500_000),
    (320, 180, 15, 150_000),
];

#[test]
#[ignore = "a measurement, not a pass or a fail"]
fn what_an_encode_costs_per_layer() {
    let now = Instant::now();
    let source: Vec<_> = (0..8).map(|number| scene(1280, 720, number, now)).collect();
    let mut scaler = Scaler::new();

    println!("layer    encoder            each    bitrate   note");
    for (width, height, fps, bitrate) in LAYERS {
        let layer: Vec<_> = source
            .iter()
            .map(|picture| {
                scaler
                    .picture(picture, width, height, PixelFormat::Nv12)
                    .unwrap()
            })
            .collect();
        let config = EncoderConfig {
            width,
            height,
            fps,
            bitrate,
        };
        // Media Foundation is not in `ALL` (it holds pictures back), but its
        // cost is worth knowing: it is the only encoder here that can run in
        // software, so it is the closest thing to Linux's OpenH264 column
        // until Cisco's library is fetched.
        for backend in Backend::ALL
            .into_iter()
            .chain(std::iter::once(Backend::MediaFoundation))
        {
            let name = format!("{backend:?}");
            let Ok(mut encoder) = Encoder::open(config, &[backend]) else {
                println!("{height:>4}p    {name:<14}        —          —   does not open here");
                continue;
            };
            for round in 0..WARMUP {
                let _ = encoder.encode(&layer[round as usize % 8], round == 0);
            }
            let mut busy = Duration::ZERO;
            let mut bytes = 0;
            for round in 0..ROUNDS {
                let picture = &layer[(round + WARMUP) as usize % 8];
                let began = Instant::now();
                let encoded = encoder.encode(picture, round == 0).unwrap();
                busy += began.elapsed();
                bytes += encoded.map_or(0, |encoded| {
                    encoded.nal_units.iter().map(Vec::len).sum::<usize>()
                });
            }
            let each = busy / ROUNDS;
            let seconds = f64::from(ROUNDS) / f64::from(fps);
            let note = match backend {
                Backend::MediaFoundation => "not in ALL: holds pictures back",
                _ => "",
            };
            println!(
                "{height:>4}p    {name:<14} {:>6.2} ms {:>7.0} kbit/s   {note}",
                each.as_secs_f64() * 1000.0,
                bytes as f64 * 8.0 / seconds / 1000.0,
            );
        }
    }
}
