//! How long each step of sending a camera picture takes on this machine:
//! reading the camera's frame, scaling to each layer, encoding each layer.
//! With `paced`, steps come 33 ms apart as a camera's pictures do, so the
//! GPU idles between them.
//!
//!   cargo run --release -p opencord-media --features testing --example codec_costs -- paced

#[cfg(target_os = "linux")]
mod linux;

fn main() {
    #[cfg(target_os = "linux")]
    linux::main();
    #[cfg(not(target_os = "linux"))]
    eprintln!("codec_costs needs Linux: it uses PipeWire, V4L2 or FFmpeg's encoders there.");
}
