//! The default sounds' files (plan §11.8), bundled so every client has
//! them without downloading. Their names and lengths are in
//! [`opencord_common::sound::defaults`].

/// The Ogg Opus file of the default sound `id`.
pub fn file(id: &str) -> Option<&'static [u8]> {
    let bytes: &'static [u8] = match id {
        "chime" => include_bytes!("../../sounds/chime.opus"),
        "tada" => include_bytes!("../../sounds/tada.opus"),
        "air-horn" => include_bytes!("../../sounds/air-horn.opus"),
        "rimshot" => include_bytes!("../../sounds/rimshot.opus"),
        "applause" => include_bytes!("../../sounds/applause.opus"),
        "sad-trombone" => include_bytes!("../../sounds/sad-trombone.opus"),
        "boing" => include_bytes!("../../sounds/boing.opus"),
        "crickets" => include_bytes!("../../sounds/crickets.opus"),
        _ => return None,
    };
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use opencord_common::sound::defaults::DEFAULT_SOUNDS;
    use opencord_common::sound::ogg::OpusFile;
    use opencord_common::sound::{MAX_FILE_BYTES, within_duration};

    use super::*;

    #[test]
    fn every_default_sound_is_bundled_as_listed() {
        for sound in DEFAULT_SOUNDS {
            let bytes = file(sound.id).unwrap_or_else(|| panic!("{} has no file", sound.id));
            let info = OpusFile::read(bytes).unwrap().info;

            assert_eq!(info.duration_ms, sound.duration_ms, "{}", sound.id);
            assert!(within_duration(&info), "{}", sound.id);
            assert!(bytes.len() <= MAX_FILE_BYTES, "{}", sound.id);
        }
    }

    #[test]
    fn default_sounds_play_at_a_steady_level() {
        for sound in DEFAULT_SOUNDS {
            let pcm = super::super::decode(file(sound.id).unwrap()).unwrap();
            let peak = pcm.samples.iter().fold(0f32, |peak, s| peak.max(s.abs()));

            assert_eq!(pcm.duration_ms(), sound.duration_ms, "{}", sound.id);
            assert!((0.1..=1.0).contains(&peak), "{} peaks at {peak}", sound.id);
        }
    }
}
