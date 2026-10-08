//! What a camera costs to send on this machine (plan §16.1: a camera on,
//! 720p30 with its layers and hardware encoding, at most 6 % CPU): a
//! virtual PipeWire camera in YUYV or MJPEG (as webcams send 720p),
//! captured and encoded into its three layers for 20 seconds. Prints the
//! CPU time of the capture and encoding threads, and of the whole process
//! but the virtual camera.
//!
//!   cargo run --release -p opencord-media --features testing --example camera_load -- mjpeg

use std::collections::HashMap;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use opencord_media::video::camera::pipewire::{Capture, Remote};
use opencord_media::video::camera::virtual_camera::{VirtualCamera, VirtualFormat};
use opencord_media::video::sender::{CameraSender, camera_layers};

const RUN: Duration = Duration::from_secs(20);

/// CPU time by thread name, from /proc.
fn thread_times() -> HashMap<String, Duration> {
    let ticks = 100.0;
    let mut times = HashMap::new();
    let Ok(tasks) = std::fs::read_dir("/proc/self/task") else {
        return times;
    };
    for task in tasks.flatten() {
        let path = task.path();
        let name = std::fs::read_to_string(path.join("comm")).unwrap_or_default();
        let stat = std::fs::read_to_string(path.join("stat")).unwrap_or_default();
        // Fields after the parenthesised name: utime and stime are the
        // 12th and 13th.
        let Some(rest) = stat.rsplit_once(')').map(|(_, rest)| rest) else {
            continue;
        };
        let fields: Vec<&str> = rest.split_whitespace().collect();
        let used: f64 = fields
            .get(11..13)
            .map(|both| {
                both.iter()
                    .filter_map(|field| field.parse::<f64>().ok())
                    .sum()
            })
            .unwrap_or(0.0);
        *times.entry(name.trim().to_owned()).or_default() += Duration::from_secs_f64(used / ticks);
    }
    times
}

fn main() {
    let format = match std::env::args().nth(1).as_deref() {
        Some("mjpeg") => VirtualFormat::Mjpeg,
        _ => VirtualFormat::Yuyv,
    };
    let camera = VirtualCamera::start_with(format).expect("a PipeWire server");
    let (frames, captured) = mpsc::sync_channel(4);
    let capture = Capture::start(Remote::Session, Some(&camera.id()), frames).expect("the camera");
    let layers = camera_layers(capture.mode.width, capture.mode.height);
    let (sent, received) = mpsc::channel();
    let sender = CameraSender::start(
        "load".to_owned(),
        layers,
        captured,
        move |frame| {
            let _ = sent.send(frame.layer);
        },
        |_| {},
    );
    // Encoders open with the first frames; measure from then on.
    std::thread::sleep(Duration::from_secs(2));
    while received.try_recv().is_ok() {}
    let before = thread_times();
    let started = Instant::now();
    std::thread::sleep(RUN);
    let after = thread_times();
    let elapsed = started.elapsed().as_secs_f64();
    let mut per_layer = [0u32; 3];
    for layer in received.try_iter() {
        if let Some(count) = per_layer.get_mut(usize::from(layer)) {
            *count += 1;
        }
    }
    drop(sender);
    drop(capture);
    drop(camera);

    let cores = std::thread::available_parallelism().map_or(1, |cores| cores.get()) as f64;
    let share = |name: &str| {
        let used = after.get(name).copied().unwrap_or_default()
            - before.get(name).copied().unwrap_or_default();
        used.as_secs_f64() / elapsed * 100.0
    };
    let total: f64 = after
        .keys()
        .filter(|name| name.as_str() != "virtual-camera")
        .map(|name| share(name))
        .sum();
    println!("{format:?} 1280x720 at 30 fps, {elapsed:.0} s, {cores} logical cores");
    println!(
        "pictures a second by layer: {:.1} / {:.1} / {:.1}",
        f64::from(per_layer[0]) / elapsed,
        f64::from(per_layer[1]) / elapsed,
        f64::from(per_layer[2]) / elapsed
    );
    for name in ["opencord-camera", "opencord-video-"] {
        println!("{name}: {:.1} % of a core", share(name));
    }
    println!(
        "everything but the virtual camera: {total:.1} % of a core, {:.2} % of the machine",
        total / cores
    );
}
