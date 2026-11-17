//! Choosing a release from a listing.

use semver::Version;

use crate::release::Listed;
use crate::track::Track;

/// The newest of `track`'s own releases in a releases listing. Only its
/// own: SemVer sorts `0.0.2-canary.1` above every `0.0.2-beta.N`, however
/// much later the beta was built, so versions say nothing across tracks.
pub fn newest(listing: &[Listed], track: Track) -> Option<&Listed> {
    listing
        .iter()
        .filter(|release| !release.draft)
        .filter_map(|release| {
            let version = Version::parse(release.tag_name.strip_prefix('v')?).ok()?;
            (Track::of(&version)? == track).then_some((version, release))
        })
        .max_by(|(a, _), (b, _)| a.cmp(b))
        .map(|(_, release)| release)
}

/// Whether a user on `track` with `installed` takes `candidate`, `track`'s
/// newest build, as the launcher's update does (D91): never the installed
/// version again, never an older build of the same track, but another
/// track's installed build gives way to `track`'s, even an older one.
pub fn takes(installed: &Version, candidate: &Version, track: Track) -> bool {
    if Track::of(candidate) != Some(track) || candidate == installed {
        return false;
    }
    Track::of(installed) != Some(track) || candidate > installed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listed(tags: &[&str]) -> Vec<Listed> {
        tags.iter()
            .map(|tag| Listed {
                tag_name: (*tag).to_owned(),
                draft: false,
            })
            .collect()
    }

    #[test]
    fn each_track_picks_the_newest_of_its_own_builds() {
        let listing = listed(&[
            "v0.0.2-beta.1",
            "v0.0.1",
            "v0.0.1-canary.2",
            "v0.0.2-canary.1",
        ]);
        let tag = |track| newest(&listing, track).map(|r| r.tag_name.as_str());
        assert_eq!(tag(Track::Beta), Some("v0.0.2-beta.1"));
        assert_eq!(tag(Track::Canary), Some("v0.0.2-canary.1"));
        assert_eq!(tag(Track::Stable), Some("v0.0.1"));
    }

    /// By version alone `canary` sorts after `beta`, so a canary hid every
    /// later beta of the same version from beta users.
    #[test]
    fn a_canary_that_sorts_higher_does_not_win_for_beta() {
        let listing = listed(&["v0.0.1-beta.5", "v0.0.1-canary.4", "v0.0.1-beta.4"]);
        let release = newest(&listing, Track::Beta).unwrap();
        assert_eq!(release.tag_name, "v0.0.1-beta.5");
    }

    #[test]
    fn a_stable_release_does_not_win_for_beta() {
        let listing = listed(&["v0.0.2-beta.4", "v0.0.2"]);
        let release = newest(&listing, Track::Beta).unwrap();
        assert_eq!(release.tag_name, "v0.0.2-beta.4");
    }

    #[test]
    fn a_track_without_a_build_of_its_own_gets_none() {
        let listing = listed(&["v0.0.2-canary.1", "v0.0.1"]);
        assert!(newest(&listing, Track::Beta).is_none());
        let listing = listed(&["v0.0.2-beta.1"]);
        assert!(newest(&listing, Track::Canary).is_none());
        assert!(newest(&listing, Track::Stable).is_none());
    }

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    // Fails if `takes` drops `candidate > installed` (an older beta is
    // taken), or `candidate == installed` (the installed one downloads
    // again).
    #[test]
    fn the_installed_build_or_an_older_one_of_the_track_is_not_taken() {
        let installed = version("0.0.2-beta.3");
        assert!(!takes(&installed, &version("0.0.2-beta.3"), Track::Beta));
        assert!(!takes(&installed, &version("0.0.2-beta.2"), Track::Beta));
        assert!(takes(&installed, &version("0.0.2-beta.4"), Track::Beta));
    }

    // Fails if `takes` compares across tracks (`Track::of(installed) !=
    // Some(track) ||` removed): a canary user switching to beta would never
    // get beta's lower-sorting build.
    #[test]
    fn another_tracks_installed_build_gives_way() {
        let installed = version("0.0.2-canary.1");
        assert!(takes(&installed, &version("0.0.2-beta.9"), Track::Beta));
    }

    // Fails if `takes` drops `Track::of(candidate) != Some(track)`.
    #[test]
    fn a_candidate_of_another_track_is_never_taken() {
        let installed = version("0.0.1-beta.1");
        assert!(!takes(&installed, &version("0.0.2-canary.1"), Track::Beta));
        assert!(!takes(&installed, &version("0.0.2"), Track::Beta));
    }

    #[test]
    fn drafts_and_strange_tags_are_skipped() {
        let mut listing = listed(&["v0.0.1", "nightly", "v0.0.3-rc.1"]);
        listing.push(Listed {
            tag_name: "v0.0.2".to_owned(),
            draft: true,
        });
        let release = newest(&listing, Track::Stable).unwrap();
        assert_eq!(release.tag_name, "v0.0.1");
        assert!(newest(&[], Track::Beta).is_none());
    }
}
