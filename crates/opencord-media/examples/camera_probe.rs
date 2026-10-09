//! Which cameras this computer has and what each way of opening them
//! delivers, for diagnosing a camera that does not work: lists them
//! through the PipeWire session and V4L2, then captures three seconds
//! through each (the camera named, or the first) and says how many
//! pictures came, in what mode, and whether the first one decodes as the
//! sender decodes it.
//! Opens the camera; the portal is not asked.
//!
//!   cargo run -p opencord-media --example camera_probe [-- <camera id>]

use std::sync::mpsc;
use std::time::{Duration, Instant};

use opencord_media::video::camera::pipewire::{self, Remote};
use opencord_media::video::camera::{RawFormat, RawFrame, v4l2};
use opencord_media::video::codec::raw::RawDecoder;

const RUN: Duration = Duration::from_secs(3);

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "opencord_media=debug".into()),
        )
        .init();
    let camera = std::env::args().nth(1);
    println!("PipeWire session cameras:");
    match pipewire::cameras(Remote::Session) {
        Ok(cameras) => cameras
            .iter()
            .for_each(|c| println!("  {} — {}", c.id, c.name)),
        Err(error) => println!("  failed: {error}"),
    }
    println!("V4L2 cameras:");
    v4l2::cameras()
        .iter()
        .for_each(|c| println!("  {} — {}", c.id, c.name));

    let pipewire_camera = camera.clone().filter(|id| !id.starts_with("v4l2:"));
    println!(
        "\nPipeWire capture ({}):",
        pipewire_camera.as_deref().unwrap_or("first")
    );
    let (frames, received) = mpsc::sync_channel(4);
    match pipewire::Capture::start(Remote::Session, pipewire_camera.as_deref(), frames) {
        Ok(capture) => {
            println!("  mode {:?}", capture.mode);
            report(&received);
        }
        Err(error) => println!("  failed: {error}"),
    }

    let v4l2_camera = camera.filter(|id| id.starts_with("v4l2:"));
    println!(
        "\nV4L2 capture ({}):",
        v4l2_camera.as_deref().unwrap_or("first")
    );
    let (frames, received) = mpsc::sync_channel(4);
    match v4l2::Capture::start(v4l2_camera.as_deref(), frames) {
        Ok(capture) => {
            println!("  mode {:?}", capture.mode);
            report(&received);
        }
        Err(error) => println!("  failed: {error}"),
    }
}

/// Counts what arrives for [`RUN`] and looks at the first picture.
fn report(received: &mpsc::Receiver<RawFrame>) {
    let start = Instant::now();
    let mut count = 0;
    let mut first: Option<RawFrame> = None;
    while let Some(left) = RUN.checked_sub(start.elapsed()) {
        match received.recv_timeout(left) {
            Ok(frame) => {
                count += 1;
                first.get_or_insert(frame);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => break,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                println!("  the camera went away");
                break;
            }
        }
    }
    println!(
        "  {count} pictures in {:.1} s ({:.1} fps)",
        start.elapsed().as_secs_f32(),
        count as f32 / start.elapsed().as_secs_f32()
    );
    if let Some(frame) = first {
        println!(
            "  first: {:?} {}×{}, {} bytes{}",
            frame.format,
            frame.width,
            frame.height,
            frame.data.len(),
            match frame.format {
                RawFormat::Mjpeg =>
                    format!(", starts {:02x?}", &frame.data[..frame.data.len().min(4)]),
                _ => String::new(),
            }
        );
        match RawDecoder::new().picture(&frame) {
            Ok(picture) => println!(
                "  decodes to {:?} {}×{}",
                picture.format, picture.width, picture.height
            ),
            Err(error) => println!("  does not decode: {error}"),
        }
    }
}
