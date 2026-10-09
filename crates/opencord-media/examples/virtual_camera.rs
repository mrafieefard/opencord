//! A virtual PipeWire camera (1280×720 at 30 fps, MJPEG as webcams send
//! 720p, or YUYV) for checks that need a camera without turning on a real
//! one: prints its id for `camera_start`, then serves until stopped.
//!
//!   cargo run --release -p opencord-media --features testing --example virtual_camera -- mjpeg

use std::io::Write;

use opencord_media::video::camera::virtual_camera::{VirtualCamera, VirtualFormat};

fn main() {
    let format = match std::env::args().nth(1).as_deref() {
        Some("yuyv") => VirtualFormat::Yuyv,
        _ => VirtualFormat::Mjpeg,
    };
    let Some(camera) = VirtualCamera::start_with(format) else {
        eprintln!("virtual_camera: no PipeWire server");
        std::process::exit(1);
    };
    println!("camera={}", camera.id());
    let _ = std::io::stdout().flush();
    loop {
        std::thread::park();
    }
}
