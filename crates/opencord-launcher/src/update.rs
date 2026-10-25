//! One update: find the newest build of the track, and if it is newer
//! than the installed one, download it, check it and unpack it beside
//! the others. Nothing installed is touched until the new version is
//! whole, so a failed update leaves the app as it was.

use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::Path;

use semver::Version;
use sha2::{Digest, Sha256};

use crate::archive;
use crate::layout::{self, Layout};
use crate::release::{Asset, Manifest, ManifestError};
use crate::track::Track;

/// Where releases come from.
pub trait Source {
    /// The newest release `track` takes.
    fn latest(&self, track: Track) -> Result<Manifest, Error>;
    /// Streams a release file into `out`.
    fn download(&self, url: &str, out: &mut dyn Write) -> Result<(), Error>;
}

#[derive(Debug)]
pub enum Error {
    /// GitHub couldn't be reached, or answered with an error.
    Network(String),
    /// No release the track takes.
    NoRelease,
    Manifest(ManifestError),
    /// `release.json`'s signature doesn't hold.
    Signature,
    /// The file isn't the one `release.json` describes.
    Download(String),
    Install(io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(error) => write!(f, "can't reach GitHub: {error}"),
            Self::NoRelease => f.write_str("no release for this track yet"),
            Self::Manifest(error) => error.fmt(f),
            Self::Signature => f.write_str("release.json isn't signed by Opencord's key"),
            Self::Download(error) => write!(f, "the download is wrong: {error}"),
            Self::Install(error) => write!(f, "can't install: {error}"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Install(error)
    }
}

/// What the splash says.
#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Installing,
    Checking,
    /// Bytes so far, of the whole.
    Downloading(u64, u64),
    Unpacking,
    Starting,
    /// Nothing is installed yet, and the update failed: GitHub couldn't
    /// be reached (`offline`), or something else went wrong.
    Retrying {
        offline: bool,
    },
}

/// Brings the install up to the newest build `track` takes. Returns the
/// version it installed, or `None` when the installed one is newest. A
/// track never goes backwards: a beta user who moves to stable keeps
/// their beta until a stable release passes it.
pub fn update(
    layout: &Layout,
    source: &dyn Source,
    track: Track,
    platform: &str,
    report: &dyn Fn(Status),
) -> Result<Option<Version>, Error> {
    report(Status::Checking);
    let manifest = source.latest(track)?;
    if layout
        .current()
        .is_some_and(|current| current >= manifest.version)
    {
        return Ok(None);
    }
    let asset = manifest.asset(platform).map_err(Error::Manifest)?;
    fs::create_dir_all(layout.downloads())?;
    let download = layout.downloads().join(&asset.file);
    fetch(source, asset, &download, report)?;

    report(Status::Unpacking);
    let version = &manifest.version;
    let partial = layout.partial_dir(version);
    let _ = fs::remove_dir_all(&partial);
    archive::unpack(&download, &asset.file, &partial)?;
    if !layout::app_executable(&partial).is_file() {
        let _ = fs::remove_dir_all(&partial);
        return Err(Error::Download(format!(
            "{} has no {}",
            asset.file,
            layout::app_executable(Path::new("")).display()
        )));
    }
    let _ = fs::remove_dir_all(layout.version_dir(version));
    fs::rename(&partial, layout.version_dir(version))?;
    let _ = fs::remove_file(&download);
    Ok(Some(version.clone()))
}

/// Downloads `asset` to `path`, refusing it unless its size and SHA-256
/// are what `release.json` says.
fn fetch(
    source: &dyn Source,
    asset: &Asset,
    path: &Path,
    report: &dyn Fn(Status),
) -> Result<(), Error> {
    report(Status::Downloading(0, asset.size));
    let file = BufWriter::new(File::create(path)?);
    let mut checked = Checked {
        out: file,
        hash: Sha256::new(),
        written: 0,
        size: asset.size,
        report,
        reported: 0,
    };
    let result = source.download(&asset.url, &mut checked);
    let Checked {
        out, hash, written, ..
    } = checked;
    let flushed = out.into_inner().map_err(|error| error.into_error());
    let outcome = result.and_then(|()| {
        flushed?.sync_all()?;
        if written != asset.size {
            return Err(Error::Download(format!(
                "{} is {written} bytes, not {}",
                asset.file, asset.size
            )));
        }
        let digest = hex::encode(hash.finalize());
        if !digest.eq_ignore_ascii_case(&asset.sha256) {
            return Err(Error::Download(format!(
                "{}'s SHA-256 is {digest}",
                asset.file
            )));
        }
        Ok(())
    });
    if outcome.is_err() {
        let _ = fs::remove_file(path);
    }
    outcome
}

/// Writes a download through, hashing it, reporting progress, and
/// refusing more bytes than the release says it has.
struct Checked<'a, W> {
    out: W,
    hash: Sha256,
    written: u64,
    size: u64,
    report: &'a dyn Fn(Status),
    reported: u64,
}

impl<W: Write> Write for Checked<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.written + buf.len() as u64 > self.size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("more than the {} bytes promised", self.size),
            ));
        }
        let written = self.out.write(buf)?;
        self.hash.update(&buf[..written]);
        self.written += written as u64;
        // About every half a percent, so the splash moves smoothly
        // without being woken for every packet.
        if self.written - self.reported >= (self.size / 200).max(64 << 10)
            || self.written == self.size
        {
            self.reported = self.written;
            (self.report)(Status::Downloading(self.written, self.size));
        }
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::io::Write;

    use super::*;

    const PLATFORM: &str = "client-linux-x64";

    struct Fake {
        manifest: Result<Manifest, ()>,
        file: Vec<u8>,
        downloads: RefCell<u32>,
    }

    impl Source for Fake {
        fn latest(&self, _: Track) -> Result<Manifest, Error> {
            self.manifest
                .clone()
                .map_err(|()| Error::Network("offline".into()))
        }

        fn download(&self, _: &str, out: &mut dyn Write) -> Result<(), Error> {
            *self.downloads.borrow_mut() += 1;
            out.write_all(&self.file)
                .map_err(|error| Error::Download(error.to_string()))
        }
    }

    /// A Linux app archive with the executable where the launcher looks.
    fn app_archive(marker: &[u8]) -> Vec<u8> {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        let mut header = tar::Header::new_gnu();
        header.set_size(marker.len() as u64);
        header.set_mode(0o755);
        builder
            .append_data(&mut header, "opencord/opencord", marker)
            .unwrap();
        builder.into_inner().unwrap().finish().unwrap()
    }

    fn fake(version: &str, file: Vec<u8>, sha256: Option<String>) -> Fake {
        let sha256 = sha256.unwrap_or_else(|| hex::encode(Sha256::digest(&file)));
        let asset = Asset {
            file: "opencord-linux-x64.tar.gz".into(),
            url: "https://example.com/a".into(),
            size: file.len() as u64,
            sha256,
        };
        Fake {
            manifest: Ok(Manifest {
                schema: 1,
                version: Version::parse(version).unwrap(),
                tag: format!("v{version}"),
                platforms: BTreeMap::from([(PLATFORM.to_owned(), asset)]),
            }),
            file,
            downloads: RefCell::new(0),
        }
    }

    fn run(layout: &Layout, source: &Fake) -> (Result<Option<Version>, Error>, Vec<Status>) {
        let statuses = RefCell::new(Vec::new());
        let result = update(layout, source, Track::Beta, PLATFORM, &|status| {
            statuses.borrow_mut().push(status)
        });
        (result, statuses.into_inner())
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    #[test]
    fn a_newer_release_is_downloaded_checked_and_installed() {
        let root = tempfile::tempdir().unwrap();
        let layout = Layout::new(root.path());
        let source = fake("0.0.2-beta.1", app_archive(b"new"), None);

        let (result, statuses) = run(&layout, &source);

        assert_eq!(result.unwrap().unwrap().to_string(), "0.0.2-beta.1");
        assert_eq!(layout.current().unwrap().to_string(), "0.0.2-beta.1");
        let exe = layout::app_executable(&layout.version_dir(&layout.current().unwrap()));
        assert_eq!(fs::read(exe).unwrap(), b"new");
        assert_eq!(statuses.first(), Some(&Status::Checking));
        assert!(statuses.contains(&Status::Unpacking));
        let total = source.file.len() as u64;
        assert!(statuses.contains(&Status::Downloading(total, total)));
        // The download is gone once installed.
        assert!(
            !layout
                .downloads()
                .join("opencord-linux-x64.tar.gz")
                .exists()
        );
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    #[test]
    fn the_installed_version_or_a_newer_one_stays() {
        let root = tempfile::tempdir().unwrap();
        let layout = Layout::new(root.path());
        run(&layout, &fake("0.0.2", app_archive(b"stable"), None))
            .0
            .unwrap();

        // The same, and an older build (a switch to a track behind it).
        for version in ["0.0.2", "0.0.2-beta.9"] {
            let source = fake(version, app_archive(b"other"), None);
            let (result, _) = run(&layout, &source);
            assert!(result.unwrap().is_none());
            assert_eq!(*source.downloads.borrow(), 0);
        }
        assert_eq!(layout.current().unwrap().to_string(), "0.0.2");
    }

    #[test]
    fn a_download_with_the_wrong_hash_installs_nothing() {
        let root = tempfile::tempdir().unwrap();
        let layout = Layout::new(root.path());
        let source = fake("0.0.2", app_archive(b"x"), Some("b".repeat(64)));

        let (result, _) = run(&layout, &source);

        assert!(matches!(result, Err(Error::Download(_))));
        assert!(layout.versions().is_empty());
        assert!(
            !layout
                .downloads()
                .join("opencord-linux-x64.tar.gz")
                .exists()
        );
    }

    #[test]
    fn a_download_longer_than_promised_is_cut_off() {
        let root = tempfile::tempdir().unwrap();
        let layout = Layout::new(root.path());
        let mut source = fake("0.0.2", app_archive(b"x"), None);
        source.file.extend_from_slice(b"and more");

        let (result, _) = run(&layout, &source);

        assert!(matches!(result, Err(Error::Download(_))));
        assert!(layout.versions().is_empty());
    }

    #[test]
    fn an_archive_without_the_app_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let layout = Layout::new(root.path());
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        let mut header = tar::Header::new_gnu();
        header.set_size(1);
        builder
            .append_data(&mut header, "readme", &b"x"[..])
            .unwrap();
        let file = builder.into_inner().unwrap().finish().unwrap();

        let (result, _) = run(&layout, &fake("0.0.2", file, None));

        assert!(matches!(result, Err(Error::Download(_))));
        assert!(layout.versions().is_empty());
        assert!(
            !layout
                .partial_dir(&Version::parse("0.0.2").unwrap())
                .exists()
        );
    }

    #[test]
    fn offline_is_an_error_and_touches_nothing() {
        let root = tempfile::tempdir().unwrap();
        let layout = Layout::new(root.path());
        let mut source = fake("0.0.2", Vec::new(), None);
        source.manifest = Err(());

        let (result, _) = run(&layout, &source);

        assert!(matches!(result, Err(Error::Network(_))));
        assert!(fs::read_dir(root.path()).unwrap().next().is_none());
    }
}
