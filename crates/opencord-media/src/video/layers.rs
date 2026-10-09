//! The layers a video track is sent in (plan §8, §9.2): a camera's 180p,
//! 360p and 720p; a screen's main layer at its preset, and a low layer
//! beside it. A screen's layers follow its size as a window is resized,
//! and the planner's cuts (plan §7.10).

use opencord_common::video::{VideoKind, layer_ceiling, scaled_to_fit};

use crate::transport::Layer;

/// The plan's camera layers (§8): name, height, frame rate and bitrate at
/// 16:9.
const LAYERS: [(&str, u32, u32, u32); 3] = [
    ("l", 180, 15, 150_000),
    ("m", 360, 30, 500_000),
    ("h", 720, 30, 1_500_000),
];
/// The layers for a camera of this size: 180, 360 and 720 lines (no more
/// than the camera has), its shape kept, bitrates by pixel count.
pub fn camera_layers(width: u32, height: u32) -> Vec<Layer> {
    let mut layers: Vec<Layer> = Vec::new();
    if width == 0 || height == 0 {
        return layers;
    }
    for (rid, target, fps, bitrate) in LAYERS {
        let layer_height = target.min(height) & !1;
        let layer_width =
            (u64::from(width) * u64::from(layer_height) / u64::from(height)) as u32 & !1;
        let taller = layers.last().is_none_or(|last| last.height < layer_height);
        if layer_height == 0 || layer_width == 0 || !taller {
            continue;
        }
        let reference = u64::from(target * 16 / 9) * u64::from(target);
        let pixels = u64::from(layer_width) * u64::from(layer_height);
        let max_bitrate = (u64::from(bitrate) * pixels / reference).min(u64::from(bitrate)) as u32;
        layers.push(Layer {
            rid: rid.to_owned(),
            width: layer_width,
            height: layer_height,
            fps,
            max_bitrate,
        });
    }
    layers
}

/// A screen share's quality (plan §9.2): its preset's pixel count and its
/// frame rate, both maxima.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenShape {
    pub max_pixels: u32,
    pub fps: u32,
}

/// A screen's main layer above this many pixels gets a low layer too.
const LOW_LAYER_ABOVE: u32 = 854 * 480;
/// The low layer: at most 640×360 at 15 fps (about 300 kbps).
const LOW_LAYER_PIXELS: u32 = 640 * 360;
const LOW_LAYER_FPS: u32 = 15;

/// The layers for a screen of this size: the main layer ("h") at the
/// preset's size and frame rate, and, when that is above 480p, the low
/// layer ("l") below it. Bitrates are the ceilings for their size.
pub fn screen_layers(width: u32, height: u32, shape: ScreenShape) -> Vec<Layer> {
    let sizes = screen_sizes(width, height, shape, 1.0, 1.0);
    let layer = |rid: &str, (width, height, fps): (u32, u32, u32)| Layer {
        rid: rid.to_owned(),
        width,
        height,
        fps,
        max_bitrate: layer_ceiling(VideoKind::Screen, width, height, fps),
    };
    match sizes {
        ScreenSizes::None => Vec::new(),
        ScreenSizes::Main(main) => vec![layer("h", main)],
        ScreenSizes::Both { low, main } => vec![layer("l", low), layer("h", main)],
    }
}

/// Width, height and frame rate of a screen's layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenSizes {
    None,
    Main((u32, u32, u32)),
    Both {
        low: (u32, u32, u32),
        main: (u32, u32, u32),
    },
}

/// A screen of `width` × `height` under `shape`, its main layer cut to
/// `fps_scale` of the frame rate and `size_scale` of the pixels by the
/// planner. The low layer is never larger or faster than the main one.
pub fn screen_sizes(
    width: u32,
    height: u32,
    shape: ScreenShape,
    fps_scale: f32,
    size_scale: f32,
) -> ScreenSizes {
    let (full_width, full_height) = scaled_to_fit(width, height, shape.max_pixels);
    let full = full_width * full_height;
    if full == 0 {
        return ScreenSizes::None;
    }
    let (main_width, main_height) = if size_scale >= 1.0 {
        (full_width, full_height)
    } else {
        let pixels = (f64::from(full) * f64::from(size_scale.max(0.0))) as u32;
        scaled_to_fit(width, height, pixels.max(4))
    };
    let fps = ((shape.fps as f32 * fps_scale.clamp(0.0, 1.0)).round() as u32).max(1);
    let main = (main_width.max(2), main_height.max(2), fps);
    if full <= LOW_LAYER_ABOVE {
        return ScreenSizes::Main(main);
    }
    let (low_width, low_height) = scaled_to_fit(
        width,
        height,
        LOW_LAYER_PIXELS.min(main_width * main_height),
    );
    ScreenSizes::Both {
        low: (low_width.max(2), low_height.max(2), LOW_LAYER_FPS.min(fps)),
        main,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_720p_camera_gets_the_plans_three_layers() {
        let layers = camera_layers(1280, 720);

        let shape: Vec<(&str, u32, u32, u32, u32)> = layers
            .iter()
            .map(|l| (l.rid.as_str(), l.width, l.height, l.fps, l.max_bitrate))
            .collect();
        assert_eq!(
            shape,
            vec![
                ("l", 320, 180, 15, 150_000),
                ("m", 640, 360, 30, 500_000),
                ("h", 1280, 720, 30, 1_500_000),
            ]
        );
    }

    #[test]
    fn layers_keep_the_cameras_shape_and_stay_under_their_ceilings() {
        for (width, height) in [
            (640, 480),
            (1920, 1080),
            (1280, 720),
            (320, 240),
            (1280, 960),
        ] {
            let layers = camera_layers(width, height);

            assert!(!layers.is_empty());
            let top = layers.last().unwrap();
            assert!(top.height <= 720 && top.height <= height);
            for layer in &layers {
                assert!(layer.width % 2 == 0 && layer.height % 2 == 0);
                let shape = f64::from(layer.width) / f64::from(layer.height);
                let camera = f64::from(width) / f64::from(height);
                assert!((shape - camera).abs() < 0.02, "{width}×{height}: {layer:?}");
                let ceiling =
                    layer_ceiling(VideoKind::Camera, layer.width, layer.height, layer.fps);
                assert!(layer.max_bitrate <= ceiling, "{layer:?} over {ceiling}");
            }
            let heights: Vec<u32> = layers.iter().map(|layer| layer.height).collect();
            assert!(
                heights.windows(2).all(|pair| pair[0] < pair[1]),
                "{heights:?}"
            );
        }
        assert_eq!(camera_layers(1920, 1080).last().unwrap().width, 1280);
        assert_eq!(camera_layers(640, 480).last().unwrap().height, 480);
    }

    fn shape_of(layers: &[Layer]) -> Vec<(&str, u32, u32, u32, u32)> {
        layers
            .iter()
            .map(|l| (l.rid.as_str(), l.width, l.height, l.fps, l.max_bitrate))
            .collect()
    }

    #[test]
    fn a_screen_gets_its_presets_main_layer_and_a_low_one_above_480p() {
        let p720 = ScreenShape {
            max_pixels: 1280 * 720,
            fps: 30,
        };
        let p1080_60 = ScreenShape {
            max_pixels: 1920 * 1080,
            fps: 60,
        };
        let p480 = ScreenShape {
            max_pixels: 854 * 480,
            fps: 15,
        };

        assert_eq!(
            shape_of(&screen_layers(1920, 1080, p720)),
            [
                ("l", 640, 360, 15, 300_000),
                ("h", 1280, 720, 30, 2_000_000)
            ]
        );
        assert_eq!(
            shape_of(&screen_layers(2560, 1440, p1080_60)),
            [
                ("l", 640, 360, 15, 300_000),
                ("h", 1920, 1080, 60, 6_500_000)
            ]
        );
        // At 480p, or a small window, the main layer is all there is.
        assert_eq!(
            shape_of(&screen_layers(1920, 1080, p480)),
            [("h", 852, 480, 15, 600_000)]
        );
        assert_eq!(
            shape_of(&screen_layers(640, 480, p720)),
            [("h", 640, 480, 30, 1_000_000)]
        );
        // 800×600 is above 480p's pixel count.
        assert_eq!(
            shape_of(&screen_layers(800, 600, p720)),
            [("l", 554, 414, 15, 300_000), ("h", 800, 600, 30, 2_000_000)]
        );
    }

    #[test]
    fn the_planners_cuts_shrink_and_slow_the_main_layer() {
        let p720 = ScreenShape {
            max_pixels: 1280 * 720,
            fps: 30,
        };

        assert_eq!(
            screen_sizes(1920, 1080, p720, 0.5, 0.25),
            ScreenSizes::Both {
                low: (640, 360, 15),
                main: (640, 360, 15)
            }
        );
        // The low layer stays no larger and no faster than the main one.
        assert_eq!(
            screen_sizes(1920, 1080, p720, 1.0 / 6.0, 0.1),
            ScreenSizes::Both {
                low: (402, 226, 5),
                main: (404, 226, 5)
            }
        );
    }
}
