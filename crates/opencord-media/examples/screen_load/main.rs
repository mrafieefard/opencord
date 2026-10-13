//! What a screen share costs to send on this machine (plan §16.1: 720p30
//! at most 8 % CPU, 1080p60 at most 15 %, hardware encoding): a virtual
//! 1920×1080 PipeWire screen, captured and encoded into its layers for 20
//! seconds. Prints the CPU time of each thread, and of the whole process
//! but the virtual screen.
//!
//!   cargo run --release -p opencord-media --features testing --example screen_load -- 1080p60

#[cfg(target_os = "linux")]
mod linux;

fn main() {
    #[cfg(target_os = "linux")]
    linux::main();
    #[cfg(not(target_os = "linux"))]
    eprintln!("screen_load needs Linux: it uses PipeWire, V4L2 or FFmpeg's encoders there.");
}
