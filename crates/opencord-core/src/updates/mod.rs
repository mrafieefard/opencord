//! Background updates (design: "The app: checking", "The app:
//! downloading"): the app checks GitHub for its track's newest build and
//! downloads it into the install folder's `downloads/`, throttled while
//! the user is in a call. The launcher then checks the download itself
//! and unpacks it (`--prepare`); nothing here unpacks or starts anything.
//!
//! Which build, the signature check and the install folder's files all
//! come from `opencord-update`, so the app and the launcher can't
//! disagree.
//!
//! What this writes, and what removes it:
//! - `downloads/<file>.part`: renamed to `downloads/<file>` once whole, or
//!   removed when its hash is wrong; otherwise by the launcher's prune.
//! - `downloads/<file>`, `downloads/<file>.json`, `downloads/release.json`
//!   and `downloads/release.json.sig`: by the launcher's prune, once the
//!   release they describe is installed or replaced.
//! - `pending.json`'s `confirmed_at`: the launcher removes the file when
//!   it switches.

mod download;
mod github;
#[cfg(test)]
mod tests;
mod throttle;

pub(crate) use throttle::publish_voice_reading;
pub use throttle::{VoiceReading, VoiceThrottle, voice_readings};

use std::future::Future;
use std::io;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ed25519_dalek::VerifyingKey;
use semver::Version;

use opencord_update::check;
use opencord_update::layout::{self, Layout};
use opencord_update::pending::DownloadNote;
use opencord_update::release::ManifestError;
use opencord_update::signature::{self, KeyError};
use opencord_update::track::Track;

use github::Github;

/// The most an app archive may be. Today's are under 100 MB; a manifest
/// promising more is wrong, signed or not.
const ARCHIVE_LIMIT: u64 = 2 << 30;

/// The release the download is for, and its signature, beside the archive,
/// where `--prepare` reads them.
const MANIFEST_FILE: &str = "release.json";
const SIGNATURE_FILE: &str = "release.json.sig";

/// What slows the download: every chunk waits on [`Throttle::take`] for
/// its size before it is read.
pub trait Throttle: Send + Sync {
    fn take(&self, bytes: usize) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

/// No limit: outside a call.
pub struct Unlimited;

impl Throttle for Unlimited {
    fn take(&self, _bytes: usize) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(std::future::ready(()))
    }
}

/// What one check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckOutcome {
    /// The installed version is the track's newest, or newer.
    UpToDate,
    /// The newest is prepared already and waits; its `confirmed_at` was
    /// set, so it stays fresh.
    AlreadyWaiting(Version),
    /// The newest is downloaded: `download_json` (`downloads/<file>.json`),
    /// `downloads/release.json` and `downloads/release.json.sig` (with a
    /// key) are in place for `--prepare <download_json>`.
    Downloaded {
        version: Version,
        download_json: PathBuf,
    },
}

/// Why a check found nothing to offer. Each is logged by kind, and the
/// next check tries again.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("network: {0}")]
    Network(String),
    /// The track has no build yet.
    #[error("the track has no release")]
    NoRelease,
    #[error("{0}")]
    Manifest(ManifestError),
    #[error("{0}")]
    Signature(&'static str),
    #[error("the download is wrong: {0}")]
    Download(String),
    #[error("the install folder: {0}")]
    Io(#[from] io::Error),
}

impl UpdateError {
    /// A short name for logs.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Network(_) => "network",
            Self::NoRelease => "no_release",
            Self::Manifest(_) => "manifest",
            Self::Signature(_) => "signature",
            Self::Download(_) => "download",
            Self::Io(_) => "io",
        }
    }
}

/// Checks for and downloads the track's newest build into one install
/// folder.
pub struct Updater {
    layout: Layout,
    track: Track,
    installed: Version,
    platform: &'static str,
    http: reqwest::Client,
    throttle: Arc<dyn Throttle>,
    key: Result<Option<VerifyingKey>, KeyError>,
    github: Github,
}

impl Updater {
    /// An updater for the install folder at `root`, with the key built in
    /// (`opencord_update::signature::built_in_key`) and GitHub's releases.
    /// `platform` is this system's entry in `release.json`
    /// (`opencord_update::release::PLATFORM`).
    pub fn new(
        root: PathBuf,
        track: Track,
        installed: Version,
        platform: &'static str,
        http: reqwest::Client,
        throttle: Arc<dyn Throttle>,
    ) -> Self {
        Self {
            layout: Layout::new(root),
            track,
            installed,
            platform,
            http,
            throttle,
            key: signature::built_in_key(),
            github: Github::releases(),
        }
    }

    /// The install layout this updater works inside.
    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// For tests: an updater targeting `base_url` instead of GitHub.
    #[cfg(test)]
    pub(crate) fn new_with_base_url(
        root: PathBuf,
        track: Track,
        installed: Version,
        platform: &'static str,
        http: reqwest::Client,
        throttle: Arc<dyn Throttle>,
        base_url: String,
    ) -> Self {
        Self {
            layout: Layout::new(root),
            track,
            installed,
            platform,
            http,
            throttle,
            key: signature::built_in_key(),
            github: Github {
                web: base_url.clone(),
                api: base_url,
                repository: opencord_update::REPOSITORY.to_owned(),
            },
        }
    }

    /// One check: the track's newest build, and if this user takes it,
    /// either the waiting update confirmed or the download made ready for
    /// `--prepare`. Logs its outcome.
    pub async fn check_once(&self) -> Result<CheckOutcome, UpdateError> {
        let started = Instant::now();
        tracing::info!(track = %self.track, installed = %self.installed, "checking for updates");
        let outcome = self.check().await;
        let after_ms = started.elapsed().as_millis();
        match &outcome {
            Ok(CheckOutcome::UpToDate) => tracing::info!(after_ms, "up to date"),
            Ok(CheckOutcome::AlreadyWaiting(version)) => {
                tracing::info!(%version, after_ms, "the update already waits");
            }
            Ok(CheckOutcome::Downloaded { version, .. }) => {
                tracing::info!(%version, after_ms, "update downloaded");
            }
            Err(error) => {
                tracing::warn!(kind = error.kind(), %error, after_ms, "update check failed");
            }
        }
        outcome
    }

    async fn check(&self) -> Result<CheckOutcome, UpdateError> {
        // A key that doesn't read means no updates, never unsigned ones.
        let key = self
            .key
            .as_ref()
            .map_err(|_| UpdateError::Signature("the built-in update key can't be read"))?;
        let release = self
            .github
            .latest(&self.http, self.track, key.as_ref(), self.platform)
            .await?;
        let version = release.manifest.version.clone();
        if !check::takes(&self.installed, &version, self.track) {
            return Ok(CheckOutcome::UpToDate);
        }
        if self.waiting(&version) && self.layout.confirm_pending(&version, now_ms())? {
            return Ok(CheckOutcome::AlreadyWaiting(version));
        }

        let asset = release
            .manifest
            .asset(self.platform)
            .map_err(UpdateError::Manifest)?;
        if asset.size == 0 || asset.size > ARCHIVE_LIMIT {
            return Err(UpdateError::Download(format!(
                "release.json promises {} bytes",
                asset.size
            )));
        }
        let note = DownloadNote {
            tag: release.manifest.tag.clone(),
            version: version.clone(),
            track: self.track.name().to_owned(),
            file: asset.file.clone(),
            url: asset.url.clone(),
            size: asset.size,
            sha256: asset.sha256.clone(),
        };
        tracing::info!(%version, size = note.size, "downloading the update");
        let download_json =
            download::fetch(&self.http, &self.layout, &note, self.throttle.as_ref()).await?;

        // Last, so `Downloaded` means all three files are there.
        let downloads = self.layout.downloads();
        let signature_file = downloads.join(SIGNATURE_FILE);
        match &release.signature {
            Some(signature) => layout::write_whole(&signature_file, signature)?,
            // A build without a key has a launcher without one, which
            // reads no signature; an old one would only mislead.
            None => download::remove_if_there(&signature_file).await?,
        }
        layout::write_whole(&downloads.join(MANIFEST_FILE), &release.bytes)?;
        Ok(CheckOutcome::Downloaded {
            version,
            download_json,
        })
    }

    /// Whether `version` is prepared already for this track: `pending.json`
    /// names it, and its folder is whole.
    fn waiting(&self, version: &Version) -> bool {
        self.layout.read_pending().is_some_and(|pending| {
            pending.version == *version
                && pending.track == self.track.name()
                && self.layout.is_whole(version)
        })
    }
}

/// Unix milliseconds, as `pending.json` holds them. A clock before 1970
/// reads 0, which only makes a confirmation look old.
fn now_ms() -> u64 {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO);
    u64::try_from(since_epoch.as_millis()).unwrap_or(u64::MAX)
}
