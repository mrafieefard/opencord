//! A virtual camera or screen for checks that need one without turning on a
//! real device: prints its id for `camera_start`, then serves until stopped.
//!
//!   cargo run --release -p opencord-media --features testing --example virtual_camera -- camera mjpeg
//!   cargo run --release -p opencord-media --features testing --example virtual_camera -- screen 1920 1080 30

#[cfg(target_os = "linux")]
mod linux;

fn main() {
    #[cfg(target_os = "linux")]
    linux::main();
    #[cfg(not(target_os = "linux"))]
    eprintln!("virtual_camera needs Linux: it uses PipeWire, V4L2 or FFmpeg's encoders there.");
}
