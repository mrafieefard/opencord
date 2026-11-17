//! The track's newest release, found as the launcher finds it
//! (docs/releasing.md): stable from `releases/latest`, beta and canary
//! from the releases API, choosing through `opencord_update::check`.

use std::time::Duration;

use ed25519_dalek::VerifyingKey;
use reqwest::StatusCode;
use reqwest::header::{ACCEPT, USER_AGENT};

use opencord_update::REPOSITORY;
use opencord_update::check;
use opencord_update::release::{Listed, Manifest};
use opencord_update::signature;
use opencord_update::track::Track;

use super::UpdateError;

/// How long `release.json`, its signature or the listing may take.
const SMALL_TIMEOUT: Duration = Duration::from_secs(30);
/// The most `release.json`, its signature or the listing may be.
const SMALL_LIMIT: usize = 4 << 20;

/// GitHub asks every API client to name itself.
pub(super) const AGENT: &str = concat!("opencord/", env!("CARGO_PKG_VERSION"));

/// Where releases are published.
pub(super) struct Github {
    /// `https://github.com`, where release files are.
    pub(super) web: String,
    /// `https://api.github.com`, which lists releases.
    pub(super) api: String,
    pub(super) repository: String,
}

/// A release's `release.json`, checked, with the bytes `--prepare` checks
/// again.
pub(super) struct Release {
    pub(super) manifest: Manifest,
    pub(super) bytes: Vec<u8>,
    /// `release.json.sig`, fetched when there is a key.
    pub(super) signature: Option<Vec<u8>>,
}

impl Github {
    /// Opencord's releases on GitHub.
    pub(super) fn releases() -> Self {
        Self {
            web: "https://github.com".to_owned(),
            api: "https://api.github.com".to_owned(),
            repository: REPOSITORY.to_owned(),
        }
    }

    /// `track`'s newest release, signed by `key` when there is one, with
    /// an entry for `platform`.
    pub(super) async fn latest(
        &self,
        http: &reqwest::Client,
        track: Track,
        key: Option<&VerifyingKey>,
        platform: &str,
    ) -> Result<Release, UpdateError> {
        let (web, repo) = (&self.web, &self.repository);
        let release = if track == Track::Stable {
            let url = format!("{web}/{repo}/releases/latest/download/release.json");
            self.manifest(http, &url, None, key, platform).await?
        } else {
            let url = format!("{}/repos/{repo}/releases?per_page=100", self.api);
            let listing = get_small(http, &url).await?;
            let listing: Vec<Listed> = serde_json::from_slice(&listing).map_err(|error| {
                UpdateError::Network(format!("the releases list can't be read: {error}"))
            })?;
            let tag = &check::newest(&listing, track)
                .ok_or(UpdateError::NoRelease)?
                .tag_name;
            let url = format!("{web}/{repo}/releases/download/{tag}/release.json");
            self.manifest(http, &url, Some(tag), key, platform).await?
        };
        // `latest` is always stable, but a manifest is only as good as
        // its checks.
        if Track::of(&release.manifest.version) != Some(track) {
            return Err(UpdateError::NoRelease);
        }
        Ok(release)
    }

    async fn manifest(
        &self,
        http: &reqwest::Client,
        url: &str,
        tag: Option<&str>,
        key: Option<&VerifyingKey>,
        platform: &str,
    ) -> Result<Release, UpdateError> {
        let bytes = get_small(http, url).await?;
        let signature = match key {
            None => None,
            Some(key) => {
                let signature = match get_small(http, &format!("{url}.sig")).await {
                    Ok(signature) => signature,
                    Err(UpdateError::NoRelease) => {
                        return Err(UpdateError::Signature("the release isn't signed"));
                    }
                    Err(error) => return Err(error),
                };
                if !signature::verify(key, &bytes, &signature) {
                    return Err(UpdateError::Signature(
                        "release.json isn't signed by Opencord's key",
                    ));
                }
                Some(signature)
            }
        };
        // `releases/latest` names no tag: the manifest's own is checked
        // against its version instead.
        let tag = match tag {
            Some(tag) => tag.to_owned(),
            None => serde_json::from_slice::<serde_json::Value>(&bytes)
                .ok()
                .and_then(|value| value["tag"].as_str().map(str::to_owned))
                .unwrap_or_default(),
        };
        let manifest = Manifest::parse(&bytes, &tag, platform).map_err(UpdateError::Manifest)?;
        Ok(Release {
            manifest,
            bytes,
            signature,
        })
    }
}

/// A small body, whole: a 404 is [`UpdateError::NoRelease`] (`releases/
/// latest` has nothing until a stable release exists).
async fn get_small(http: &reqwest::Client, url: &str) -> Result<Vec<u8>, UpdateError> {
    let read = async {
        let mut response = http
            .get(url)
            .header(USER_AGENT, AGENT)
            .header(ACCEPT, "application/vnd.github+json, application/json, */*")
            .send()
            .await
            .map_err(network)?;
        let status = response.status();
        if status == StatusCode::NOT_FOUND {
            return Err(UpdateError::NoRelease);
        }
        if !status.is_success() {
            return Err(UpdateError::Network(format!(
                "a release file answered {status}"
            )));
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(network)? {
            if body.len() + chunk.len() > SMALL_LIMIT {
                return Err(UpdateError::Network(
                    "a release file is too large".to_owned(),
                ));
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    };
    tokio::time::timeout(SMALL_TIMEOUT, read)
        .await
        .map_err(|_| UpdateError::Network("a release file timed out".to_owned()))?
}

pub(super) fn network(error: reqwest::Error) -> UpdateError {
    // Without the URL: a CDN's signed link is no one's business in a log.
    UpdateError::Network(error.without_url().to_string())
}
