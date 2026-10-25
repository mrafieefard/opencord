//! What CI publishes with each release (docs/releasing.md): `release.json`,
//! and the GitHub releases listing the launcher picks one from.

use std::collections::BTreeMap;

use semver::Version;
use serde::Deserialize;

use crate::track::Track;

/// This system's app archive in `release.json`'s `platforms`.
pub const PLATFORM: Option<&str> = if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
    Some("client-linux-x64")
} else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
    Some("client-windows-x64")
} else if cfg!(target_os = "macos") {
    Some("client-macos-universal")
} else {
    None
};

/// `release.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub schema: u32,
    pub version: Version,
    pub tag: String,
    pub platforms: BTreeMap<String, Asset>,
}

/// One file of a release.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Asset {
    pub file: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

/// What can be wrong with a `release.json`.
#[derive(Debug, PartialEq, Eq)]
pub enum ManifestError {
    Unreadable(String),
    /// A schema this launcher doesn't know.
    Schema(u32),
    /// Not the release it was fetched for.
    WrongRelease {
        wanted: String,
        found: String,
    },
    /// No build for this system.
    NoPlatform,
    /// A hash that isn't 64 hex digits, or a file name with a path in it.
    BadAsset,
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreadable(error) => write!(f, "release.json can't be read: {error}"),
            Self::Schema(schema) => write!(f, "release.json schema {schema} is too new"),
            Self::WrongRelease { wanted, found } => {
                write!(f, "release.json of {found} was served for {wanted}")
            }
            Self::NoPlatform => f.write_str("the release has no build for this system"),
            Self::BadAsset => f.write_str("release.json describes a file wrongly"),
        }
    }
}

impl Manifest {
    /// Reads `release.json` fetched for `tag`, checking it is that
    /// release's and has a sound entry for `platform`.
    pub fn parse(bytes: &[u8], tag: &str, platform: &str) -> Result<Self, ManifestError> {
        let manifest: Self = serde_json::from_slice(bytes)
            .map_err(|error| ManifestError::Unreadable(error.to_string()))?;
        if manifest.schema != 1 {
            return Err(ManifestError::Schema(manifest.schema));
        }
        if manifest.tag != tag || manifest.tag != format!("v{}", manifest.version) {
            return Err(ManifestError::WrongRelease {
                wanted: tag.to_owned(),
                found: manifest.tag,
            });
        }
        let asset = manifest.asset(platform)?;
        let sound_hash =
            asset.sha256.len() == 64 && asset.sha256.bytes().all(|byte| byte.is_ascii_hexdigit());
        let plain_name = !asset.file.is_empty()
            && !asset.file.contains(['/', '\\'])
            && asset.file != "."
            && asset.file != "..";
        if !sound_hash || !plain_name {
            return Err(ManifestError::BadAsset);
        }
        Ok(manifest)
    }

    pub fn asset(&self, platform: &str) -> Result<&Asset, ManifestError> {
        self.platforms
            .get(platform)
            .ok_or(ManifestError::NoPlatform)
    }
}

/// One entry of `GET /repos/{repo}/releases`.
#[derive(Debug, Deserialize)]
pub struct Listed {
    pub tag_name: String,
    #[serde(default)]
    pub draft: bool,
}

/// The newest release `track` takes from a releases listing, by version:
/// SemVer orders `0.0.2-beta.3 < 0.0.2-canary.1 < 0.0.2`, the order the
/// channels release in.
pub fn newest(listing: &[Listed], track: Track) -> Option<(Version, String)> {
    listing
        .iter()
        .filter(|release| !release.draft)
        .filter_map(|release| {
            let version = Version::parse(release.tag_name.strip_prefix('v')?).ok()?;
            let channel = Track::of(&version)?;
            track
                .follows(channel)
                .then(|| (version, release.tag_name.clone()))
        })
        .max_by(|(a, _), (b, _)| a.cmp(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLATFORM: &str = "client-linux-x64";

    fn manifest(tag: &str, version: &str, sha256: &str, file: &str) -> String {
        serde_json::json!({
            "schema": 1,
            "version": version,
            "channel": "beta",
            "tag": tag,
            "commit": "abc",
            "platforms": {
                PLATFORM: {
                    "file": file,
                    "url": format!("https://example.com/{file}"),
                    "size": 10,
                    "sha256": sha256,
                }
            }
        })
        .to_string()
    }

    fn listed(tags: &[&str]) -> Vec<Listed> {
        tags.iter()
            .map(|tag| Listed {
                tag_name: (*tag).to_owned(),
                draft: false,
            })
            .collect()
    }

    #[test]
    fn a_sound_manifest_reads() {
        let hash = "a".repeat(64);
        let text = manifest(
            "v0.0.2-beta.3",
            "0.0.2-beta.3",
            &hash,
            "opencord-linux-x64.tar.gz",
        );
        let read = Manifest::parse(text.as_bytes(), "v0.0.2-beta.3", PLATFORM).unwrap();
        assert_eq!(read.version, Version::parse("0.0.2-beta.3").unwrap());
        assert_eq!(read.asset(PLATFORM).unwrap().size, 10);
    }

    #[test]
    fn a_manifest_for_another_release_is_refused() {
        let hash = "a".repeat(64);
        let text = manifest("v0.0.1", "0.0.1", &hash, "opencord-linux-x64.tar.gz");
        assert!(matches!(
            Manifest::parse(text.as_bytes(), "v0.0.2", PLATFORM),
            Err(ManifestError::WrongRelease { .. })
        ));
        // The tag and the version inside must agree too.
        let text = manifest("v0.0.2", "0.0.1", &hash, "opencord-linux-x64.tar.gz");
        assert!(matches!(
            Manifest::parse(text.as_bytes(), "v0.0.2", PLATFORM),
            Err(ManifestError::WrongRelease { .. })
        ));
    }

    #[test]
    fn a_manifest_without_this_system_or_with_a_bad_entry_is_refused() {
        let hash = "a".repeat(64);
        let text = manifest("v0.0.2", "0.0.2", &hash, "x.tar.gz");
        assert_eq!(
            Manifest::parse(text.as_bytes(), "v0.0.2", "client-windows-x64").unwrap_err(),
            ManifestError::NoPlatform
        );
        let text = manifest("v0.0.2", "0.0.2", "nothex", "x.tar.gz");
        assert_eq!(
            Manifest::parse(text.as_bytes(), "v0.0.2", PLATFORM).unwrap_err(),
            ManifestError::BadAsset
        );
        let text = manifest("v0.0.2", "0.0.2", &hash, "../x.tar.gz");
        assert_eq!(
            Manifest::parse(text.as_bytes(), "v0.0.2", PLATFORM).unwrap_err(),
            ManifestError::BadAsset
        );
    }

    #[test]
    fn each_track_picks_the_newest_build_it_takes() {
        let listing = listed(&[
            "v0.0.2-beta.1",
            "v0.0.1",
            "v0.0.1-canary.2",
            "v0.0.2-canary.1",
        ]);
        let tag = |track| newest(&listing, track).map(|(_, tag)| tag);
        assert_eq!(tag(Track::Beta).as_deref(), Some("v0.0.2-canary.1"));
        assert_eq!(tag(Track::Canary).as_deref(), Some("v0.0.2-canary.1"));
        assert_eq!(tag(Track::Stable).as_deref(), Some("v0.0.1"));
    }

    #[test]
    fn a_stable_release_newer_than_the_betas_wins_for_beta() {
        let listing = listed(&["v0.0.2-beta.4", "v0.0.2"]);
        let (_, tag) = newest(&listing, Track::Beta).unwrap();
        assert_eq!(tag, "v0.0.2");
    }

    #[test]
    fn drafts_and_strange_tags_are_skipped() {
        let mut listing = listed(&["v0.0.1", "nightly", "v0.0.3-rc.1"]);
        listing.push(Listed {
            tag_name: "v0.0.2".to_owned(),
            draft: true,
        });
        let (_, tag) = newest(&listing, Track::Beta).unwrap();
        assert_eq!(tag, "v0.0.1");
        assert!(newest(&[], Track::Beta).is_none());
    }
}
