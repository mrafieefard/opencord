//! The restart note handed across the restart animation.

use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use semver::Version;
use serde::{Deserialize, Serialize};

use crate::layout::Layout;

/// How long a restart note stays fresh.
pub const RESTART_FOR: Duration = Duration::from_secs(2 * 60);

/// Voice state to rejoin across a restart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceNote {
    pub server_key: String,
    pub channel_id: i64,
}

/// Window placement in physical pixels to restore across a restart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowNote {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// A note left for the new app when restarting for an update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestartNote {
    pub from: Version,
    pub to: Version,
    pub at: u64,
    pub voice: Option<VoiceNote>,
    pub window: Option<WindowNote>,
    pub cover: bool,
}

impl RestartNote {
    /// Whether this restart note was written recently.
    pub fn is_fresh(&self, now: u64) -> bool {
        now.saturating_sub(self.at) < RESTART_FOR.as_millis() as u64
    }
}

impl Layout {
    pub fn restart_file(&self) -> PathBuf {
        self.root().join("restart.json")
    }

    pub fn read_restart(&self) -> Option<RestartNote> {
        let bytes = fs::read(self.restart_file()).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    pub fn write_restart(&self, restart: &RestartNote) -> io::Result<()> {
        self.write_json(&self.restart_file(), restart)
    }

    pub fn remove_restart(&self) -> io::Result<()> {
        match fs::remove_file(self.restart_file()) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fails if RestartNote serde schema does not deserialize tests/fixtures/restart.json
    #[test]
    fn the_restart_fixture_parses() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let fixture_path = manifest_dir.join("tests/fixtures/restart.json");
        let bytes = fs::read(&fixture_path).expect("reading restart.json fixture");
        let note: RestartNote = serde_json::from_slice(&bytes).expect("parsing fixture");
        assert_eq!(note.from, Version::parse("0.0.1").unwrap());
        assert_eq!(note.to, Version::parse("0.0.2").unwrap());
        assert_eq!(note.at, 1700000000000);
        assert_eq!(
            note.voice,
            Some(VoiceNote {
                server_key: "srv123".to_owned(),
                channel_id: 456,
            })
        );
        assert_eq!(
            note.window,
            Some(WindowNote {
                x: 100,
                y: 200,
                width: 1280,
                height: 720,
            })
        );
        assert!(note.cover);
    }
}
