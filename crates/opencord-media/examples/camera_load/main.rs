//! What a camera costs to send on this machine (plan §16.1: a camera on,
//! 720p30 with its layers and hardware encoding, at most 6 % CPU): a
//! virtual PipeWire camera in YUYV or MJPEG (as webcams send 720p),
//! captured and encoded into its three layers for 20 seconds. Prints the
//! CPU time of the capture and encoding threads, and of the whole process
//! but the virtual camera.
//!
//!   cargo run --release -p opencord-media --features testing --example camera_load -- mjpeg

#[cfg(target_os = "linux")]
mod linux;

fn main() {
    #[cfg(target_os = "linux")]
    linux::main();
    #[cfg(not(target_os = "linux"))]
    eprintln!("camera_load needs Linux: it uses PipeWire, V4L2 or FFmpeg's encoders there.");
}
