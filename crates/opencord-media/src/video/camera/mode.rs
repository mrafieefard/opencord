//! Which of a camera's modes to capture (plan §8): 1280×720 at 30 fps, or
//! as near as the camera allows. Frame rate comes first, then the smallest
//! size that reaches 720p (else the largest below it), then raw formats
//! over MJPEG, which costs a decode.

use super::RawFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mode {
    pub format: RawFormat,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

/// The mode to capture; `None` if none is usable (odd sizes, no rate).
pub fn choose(modes: &[Mode]) -> Option<Mode> {
    modes
        .iter()
        .copied()
        .filter(|mode| {
            mode.width > 0
                && mode.height > 0
                && mode.fps > 0
                && mode.width.is_multiple_of(2)
                && mode.height.is_multiple_of(2)
        })
        .max_by_key(|mode| score(*mode))
}

fn score(mode: Mode) -> (u8, bool, i64, u8, i64) {
    let rate = match mode.fps {
        30.. => 2,
        24.. => 1,
        _ => 0,
    };
    let area = i64::from(mode.width) * i64::from(mode.height);
    let reaches = mode.width >= 1280 && mode.height >= 720;
    let size = if reaches { -area } else { area };
    let format = match mode.format {
        RawFormat::Nv12 => 3,
        RawFormat::Yuyv | RawFormat::I420 => 2,
        RawFormat::Mjpeg => 1,
        RawFormat::Bgrx | RawFormat::Rgbx => 0,
    };
    let near_thirty = -(i64::from(mode.fps) - 30).abs();
    (rate, reaches, size, format, near_thirty)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(format: RawFormat, width: u32, height: u32, fps: u32) -> Mode {
        Mode {
            format,
            width,
            height,
            fps,
        }
    }

    #[test]
    fn this_machines_webcam_gets_720p_mjpeg() {
        let modes = [
            mode(RawFormat::Mjpeg, 1280, 720, 30),
            mode(RawFormat::Mjpeg, 640, 480, 30),
            mode(RawFormat::Mjpeg, 640, 360, 30),
            mode(RawFormat::Yuyv, 640, 480, 30),
            mode(RawFormat::Yuyv, 1280, 720, 10),
        ];

        assert_eq!(choose(&modes), Some(mode(RawFormat::Mjpeg, 1280, 720, 30)));
    }

    #[test]
    fn raw_wins_over_mjpeg_at_the_same_size_and_rate() {
        let modes = [
            mode(RawFormat::Mjpeg, 1280, 720, 30),
            mode(RawFormat::Yuyv, 1280, 720, 30),
        ];

        assert_eq!(choose(&modes).unwrap().format, RawFormat::Yuyv);
    }

    #[test]
    fn the_smallest_size_that_reaches_720p_wins() {
        let modes = [
            mode(RawFormat::Mjpeg, 1920, 1080, 30),
            mode(RawFormat::Mjpeg, 1280, 720, 30),
            mode(RawFormat::Mjpeg, 2560, 1440, 30),
        ];

        assert_eq!(choose(&modes).unwrap().height, 720);
    }

    #[test]
    fn a_small_camera_gives_its_largest_size() {
        let modes = [
            mode(RawFormat::Yuyv, 320, 240, 30),
            mode(RawFormat::Yuyv, 640, 480, 30),
        ];

        assert_eq!(choose(&modes).unwrap().width, 640);
    }

    #[test]
    fn frame_rate_comes_before_size() {
        let modes = [
            mode(RawFormat::Yuyv, 1920, 1080, 5),
            mode(RawFormat::Yuyv, 640, 480, 30),
        ];

        assert_eq!(choose(&modes).unwrap().fps, 30);
    }

    #[test]
    fn thirty_fps_is_preferred_over_sixty() {
        let modes = [
            mode(RawFormat::Mjpeg, 1280, 720, 60),
            mode(RawFormat::Mjpeg, 1280, 720, 30),
        ];

        assert_eq!(choose(&modes).unwrap().fps, 30);
    }

    #[test]
    fn nothing_offered_is_nothing_chosen() {
        assert_eq!(choose(&[]), None);
        assert_eq!(choose(&[mode(RawFormat::Yuyv, 641, 480, 30)]), None);
    }
}
