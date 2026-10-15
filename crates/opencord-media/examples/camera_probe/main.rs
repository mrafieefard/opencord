//! Which cameras this computer has and what each way of opening them
//! delivers, for diagnosing a camera that does not work: lists them
//! through the PipeWire session and V4L2, then captures three seconds
//! through each (the camera named, or the first) and says how many
//! pictures came, in what mode, and whether the first one decodes as the
//! sender decodes it.
//! Opens the camera; the portal is not asked.
//!
//!   cargo run -p opencord-media --example camera_probe [-- <camera id>]

#[cfg(target_os = "linux")]
mod linux;

fn main() {
    #[cfg(target_os = "linux")]
    linux::main();
    #[cfg(not(target_os = "linux"))]
    eprintln!("camera_probe needs Linux: it uses PipeWire, V4L2 or FFmpeg's encoders there.");
}
