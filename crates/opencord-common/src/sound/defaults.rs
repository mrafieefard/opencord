//! The sounds every client ships with (plan §11.8). They are synthesized
//! by `cargo run -p opencord-media --example default_sounds` and released
//! under CC0; the files are bundled in opencord-media, and the server only
//! needs their names and lengths.

use std::hash::{Hash, Hasher};

#[derive(Debug)]
pub struct DefaultSound {
    /// What follows `default:` in a sound reference.
    pub id: &'static str,
    pub name: &'static str,
    pub emoji: &'static str,
    pub duration_ms: u32,
}

impl PartialEq for DefaultSound {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for DefaultSound {}

impl Hash for DefaultSound {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

pub const DEFAULT_SOUNDS: &[DefaultSound] = &[
    DefaultSound {
        id: "chime",
        name: "Chime",
        emoji: "🔔",
        duration_ms: 1_400,
    },
    DefaultSound {
        id: "tada",
        name: "Ta-da",
        emoji: "🎉",
        duration_ms: 1_600,
    },
    DefaultSound {
        id: "air-horn",
        name: "Air horn",
        emoji: "📯",
        duration_ms: 1_750,
    },
    DefaultSound {
        id: "rimshot",
        name: "Ba dum tss",
        emoji: "🥁",
        duration_ms: 1_600,
    },
    DefaultSound {
        id: "applause",
        name: "Applause",
        emoji: "👏",
        duration_ms: 3_000,
    },
    DefaultSound {
        id: "sad-trombone",
        name: "Sad trombone",
        emoji: "🎺",
        duration_ms: 2_800,
    },
    DefaultSound {
        id: "boing",
        name: "Boing",
        emoji: "🪀",
        duration_ms: 900,
    },
    DefaultSound {
        id: "crickets",
        name: "Crickets",
        emoji: "🦗",
        duration_ms: 2_500,
    },
];

pub fn by_id(id: &str) -> Option<&'static DefaultSound> {
    DEFAULT_SOUNDS.iter().find(|sound| sound.id == id)
}
