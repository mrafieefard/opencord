//! What a screen share costs to send on this machine (plan §16.1: 720p30
//! at most 8 % CPU, 1080p60 at most 15 %, hardware encoding): a virtual
//! 1920×1080 PipeWire screen, captured and encoded into its layers for 20
//! seconds. Prints the CPU time of each thread, and of the whole process
//! but the virtual screen.
//!
//!   cargo run --release -p opencord-media --features testing --example screen_load -- 1080p60

use std::collections::HashMap;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use opencord_media::video::camera::pipewire::Remote;
use opencord_media::video::camera::virtual_camera::{VirtualCamera, VirtualFormat};
use opencord_media::video::layers::{ScreenShape, screen_layers};
use opencord_media::video::screen::capture::{ScreenCapture, Target};
use opencord_media::video::sender::VideoSender;

const RUN: Duration = Duration::from_secs(20);

/// CPU time by thread name, from /proc.
fn thread_times() -> HashMap<String, Duration> {
    let mut times = HashMap::new();
    let Ok(tasks) = std::fs::read_dir("/proc/self/task") else {
        return times;
    };
    for task in tasks.flatten() {
        let path = task.path();
        let name = std::fs::read_to_string(path.join("comm")).unwrap_or_default();
        let stat = std::fs::read_to_string(path.join("stat")).unwrap_or_default();
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
        *times.entry(name.trim().to_owned()).or_default() += Duration::from_secs_f64(used / 100.0);
    }
    times
}

fn main() {
    let shape = match std::env::args().nth(1).as_deref() {
        Some("1080p60") => ScreenShape {
            max_pixels: 1920 * 1080,
            fps: 60,
        },
        _ => ScreenShape {
            max_pixels: 1280 * 720,
            fps: 30,
        },
    };
    let screen = VirtualCamera::start_with(VirtualFormat::Screen {
        width: 1920,
        height: 1080,
        fps: shape.fps,
    })
    .expect("a PipeWire server");
    let (frames, captured) = mpsc::sync_channel(4);
    let capture = ScreenCapture::start(
        Remote::Session,
        Target::Named(screen.name.clone()),
        shape.fps,
        frames,
    )
    .expect("the screen");
    let layers = screen_layers(capture.size.0, capture.size.1, shape);
    let described: Vec<String> = layers
        .iter()
        .map(|layer| {
            format!(
                "{} {}x{}@{}",
                layer.rid, layer.width, layer.height, layer.fps
            )
        })
        .collect();
    let (sent, received) = mpsc::channel();
    let sender = VideoSender::screen(
        "load".to_owned(),
        layers,
        shape,
        captured,
        move |frame| {
            let _ = sent.send(frame.layer);
        },
        |_| {},
    );
    std::thread::sleep(Duration::from_secs(2));
    while received.try_recv().is_ok() {}
    let before = thread_times();
    let started = Instant::now();
    std::thread::sleep(RUN);
    let after = thread_times();
    let elapsed = started.elapsed().as_secs_f64();
    let mut per_layer = [0u32; 2];
    for layer in received.try_iter() {
        if let Some(count) = per_layer.get_mut(usize::from(layer)) {
            *count += 1;
        }
    }
    drop(sender);
    drop(capture);
    drop(screen);

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
    println!("1920x1080 source, layers {described:?}, {elapsed:.0} s, {cores} logical cores");
    println!(
        "pictures a second by layer: {:.1} / {:.1}",
        f64::from(per_layer[0]) / elapsed,
        f64::from(per_layer[1]) / elapsed
    );
    for name in ["opencord-screen", "opencord-video-"] {
        println!("{name}: {:.1} % of a core", share(name));
    }
    println!(
        "everything but the virtual screen: {total:.1} % of a core, {:.2} % of the machine",
        total / cores
    );
}
