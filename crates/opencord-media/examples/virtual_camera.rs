//! A virtual camera or screen for checks that need one without turning on a
//! real device: prints its id for `camera_start`, then serves until stopped.
//!
//!   cargo run --release -p opencord-media --features testing --example virtual_camera -- camera mjpeg
//!   cargo run --release -p opencord-media --features testing --example virtual_camera -- screen 1920 1080 30

use std::io::Write;

use opencord_media::video::camera::virtual_camera::{VirtualCamera, VirtualFormat};

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let format = match args.first().map(|s| s.as_str()) {
        Some("camera") => match args.get(1).map(|s| s.as_str()) {
            Some("yuyv") => VirtualFormat::Yuyv,
            _ => VirtualFormat::Mjpeg,
        },
        Some("screen") => {
            let width = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1920);
            let height = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1080);
            let fps = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(30);
            VirtualFormat::Screen { width, height, fps }
        }
        _ => {
            eprintln!("Usage:");
            eprintln!("  virtual_camera camera [mjpeg|yuyv]  -- virtual camera");
            eprintln!("  virtual_camera screen [WIDTH [HEIGHT [FPS]]]  -- virtual screen");
            std::process::exit(1);
        }
    };
    let Some(device) = VirtualCamera::start_with(format) else {
        eprintln!("virtual_camera: no PipeWire server");
        std::process::exit(1);
    };
    let id = if matches!(format, VirtualFormat::Screen { .. }) {
        format!("screen={}", device.name)
    } else {
        format!("camera={}", device.id())
    };
    println!("{id}");
    let _ = std::io::stdout().flush();
    loop {
        std::thread::park();
    }
}
