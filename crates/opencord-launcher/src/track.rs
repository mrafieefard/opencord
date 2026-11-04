//! Release tracks (docs/releasing.md): stable, canary and beta, from the
//! `main`, `canary` and `beta` branches.

use std::fmt;

use semver::Version;

/// The builds a user takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Track {
    #[default]
    Stable,
    Canary,
    Beta,
}

impl Track {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "stable" => Some(Self::Stable),
            "canary" => Some(Self::Canary),
            "beta" => Some(Self::Beta),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Canary => "canary",
            Self::Beta => "beta",
        }
    }

    /// The track this build of the launcher was made on: CI builds each
    /// release's installer from that release's branch.
    pub fn built_in() -> Self {
        option_env!("OPENCORD_TRACK")
            .and_then(Self::parse)
            .unwrap_or_default()
    }

    /// The track a version was released on: its pre-release name, or
    /// stable without one.
    pub fn of(version: &Version) -> Option<Self> {
        if version.pre.is_empty() {
            return Some(Self::Stable);
        }
        let name = version.pre.as_str().split('.').next()?;
        match name {
            "canary" => Some(Self::Canary),
            "beta" => Some(Self::Beta),
            _ => None,
        }
    }
}

impl fmt::Display for Track {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn a_version_names_its_track() {
        assert_eq!(Track::of(&version("0.0.2")), Some(Track::Stable));
        assert_eq!(Track::of(&version("0.0.2-canary.1")), Some(Track::Canary));
        assert_eq!(Track::of(&version("0.0.2-beta.13")), Some(Track::Beta));
        assert_eq!(Track::of(&version("0.0.2-rc.1")), None);
    }

    #[test]
    fn names_read_back() {
        for track in [Track::Stable, Track::Canary, Track::Beta] {
            assert_eq!(Track::parse(track.name()), Some(track));
        }
        assert_eq!(Track::parse(" Canary\n"), Some(Track::Canary));
        assert_eq!(Track::parse("nightly"), None);
    }
}
