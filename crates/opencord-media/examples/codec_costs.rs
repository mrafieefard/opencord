//! How long each step of sending a camera picture takes on this machine:
//! reading the camera's frame, scaling to each layer, encoding each layer.
//! With `paced`, steps come 33 ms apart as a camera's pictures do, so the
//! GPU idles between them.
//!
//!   cargo run --release -p opencord-media --features testing --example codec_costs -- paced

use std::time::{Duration, Instant};

use opencord_media::video::camera::{RawFormat, RawFrame};
use opencord_media::video::codec::convert;
use opencord_media::video::codec::encoder::{Backend, Encoder, EncoderConfig};
use opencord_media::video::codec::raw::{RawDecoder, encode_jpeg};
use opencord_media::video::codec::scale::Scaler;
use opencord_media::video::pattern::scene;
use opencord_media::video::picture::PixelFormat;

const ROUNDS: u32 = 120;
const FRAME: Duration = Duration::from_nanos(33_333_333);

/// This thread's CPU time so far, from /proc (10 ms steps).
fn cpu() -> Duration {
    let stat = std::fs::read_to_string("/proc/thread-self/stat").unwrap_or_default();
    let fields: Vec<&str> = stat
        .rsplit_once(')')
        .map(|(_, rest)| rest.split_whitespace().collect())
        .unwrap_or_default();
    let ticks: f64 = fields
        .get(11..13)
        .map(|both| {
            both.iter()
                .filter_map(|field| field.parse::<f64>().ok())
                .sum()
        })
        .unwrap_or(0.0);
    Duration::from_secs_f64(ticks / 100.0)
}

fn time(label: &str, mut step: impl FnMut(u32)) {
    let paced = std::env::args().nth(1).as_deref() == Some("paced");
    for round in 0..10 {
        step(round);
    }
    let rounds = if paced { ROUNDS } else { ROUNDS * 4 };
    let started = Instant::now();
    let cpu_before = cpu();
    let mut busy = Duration::ZERO;
    for round in 0..rounds {
        let began = Instant::now();
        step(round + 10);
        busy += began.elapsed();
        if paced {
            let next = started + FRAME * (round + 1);
            std::thread::sleep(next.saturating_duration_since(Instant::now()));
        }
    }
    let each = busy / rounds;
    let cpu_each = (cpu() - cpu_before) / rounds;
    println!(
        "{label}: {:.2} ms, {:.2} ms of CPU",
        each.as_secs_f64() * 1000.0,
        cpu_each.as_secs_f64() * 1000.0
    );
}

fn main() {
    let now = Instant::now();
    let pictures: Vec<_> = (0..8).map(|number| scene(1280, 720, number, now)).collect();
    let yuyv: Vec<u8> = pictures[0]
        .y()
        .as_chunks::<2>()
        .0
        .iter()
        .flat_map(|pair| [pair[0], 128, pair[1], 128])
        .collect();
    let frame = RawFrame {
        format: RawFormat::Yuyv,
        width: 1280,
        height: 720,
        data: yuyv,
        captured: now,
    };
    let mut decoder = RawDecoder::new();
    time("YUYV 720p to NV12", |_| {
        decoder.picture(&frame).unwrap();
    });
    let jpeg = RawFrame {
        format: RawFormat::Mjpeg,
        data: encode_jpeg(&pictures[0]),
        ..frame
    };
    time("MJPEG 720p to NV12", |_| {
        decoder.picture(&jpeg).unwrap();
    });
    let mut scaler = Scaler::new();
    let i420 = scaler
        .picture(&pictures[0], 1280, 720, PixelFormat::I420)
        .unwrap();
    let planes = i420.planes();
    time("full-range 4:2:0 to NV12", |_| {
        convert::full_420_to_nv12(
            [planes[0].0, planes[1].0, planes[2].0],
            [planes[0].1, planes[1].1, planes[2].1],
            1280,
            720,
        );
    });
    time("halve 720p", |round| {
        convert::halve_nv12(&pictures[round as usize % 8]).unwrap();
    });
    time("720p to 360p", |round| {
        scaler
            .picture(&pictures[round as usize % 8], 640, 360, PixelFormat::Nv12)
            .unwrap();
    });
    time("720p to 180p", |round| {
        scaler
            .picture(&pictures[round as usize % 8], 320, 180, PixelFormat::Nv12)
            .unwrap();
    });
    for (width, height, bitrate) in [
        (1280, 720, 1_500_000),
        (640, 360, 500_000),
        (320, 180, 150_000),
    ] {
        let layer: Vec<_> = pictures
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
            fps: 30,
            bitrate,
        };
        for backend in Backend::ALL {
            let Ok(mut encoder) = Encoder::open(config, &[backend]) else {
                continue;
            };
            time(&format!("encode {height}p with {backend:?}"), |round| {
                encoder
                    .encode(&layer[round as usize % 8], round == 0)
                    .unwrap();
            });
        }
    }
}
