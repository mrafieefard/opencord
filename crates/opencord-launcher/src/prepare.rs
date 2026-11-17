//! `opencord --prepare <downloads/<file>.json>` (design: "`--prepare`"):
//! the app has downloaded a release in the background, and the launcher
//! checks it itself and unpacks it beside the installed versions, for the
//! next start to switch to. It shows no window, starts nothing, doesn't
//! prune and never touches `installed.json`.
//!
//! The result goes back to the app as the exit code ([`PREPARED`],
//! [`FAILED`], [`BUSY`]), and on a failure as `downloads/<file>.error` too,
//! one line: on macOS the app starts it through `open` and can't see the
//! code. That file is removed by the next prepare of the same download, or
//! by prune with the rest of `downloads/`.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;
use std::time::{Duration, Instant};

use ed25519_dalek::VerifyingKey;
use semver::Version;
use sha2::{Digest, Sha256};

use opencord_update::archive;
use opencord_update::layout::{self, Layout};
use opencord_update::pending::{DownloadNote, Pending};
use opencord_update::release::{Manifest, ManifestError};
use opencord_update::signature::{self, KeyError};
use opencord_update::track::Track;

use crate::{log, now_ms, update};

/// The download is unpacked and waits in `pending.json`.
pub const PREPARED: i32 = 0;
/// Something was wrong; `downloads/<file>.error` says what.
pub const FAILED: i32 = 1;
/// Another prepare, a switch or an update held the install folder for
/// [`LOCK_WAIT`].
pub const BUSY: i32 = 3;
/// How long a prepare waits for the install folder.
pub const LOCK_WAIT: Duration = Duration::from_secs(10 * 60);

/// The release the app downloaded for, and its signature, beside the
/// archive.
const MANIFEST_FILE: &str = "release.json";
const SIGNATURE_FILE: &str = "release.json.sig";

/// Limits on what is read whole. A `release.json` is under 2 KB today, a
/// note under 1 KB, and a signature is 88 bytes of base64.
const MANIFEST_LIMIT: u64 = 1 << 20;
const NOTE_LIMIT: u64 = 64 << 10;
const SIGNATURE_LIMIT: u64 = 1 << 10;

/// Why a download wasn't prepared. The kind starts the error file's line.
#[derive(Debug)]
enum Failure {
    /// The download's note can't be read, or names another file.
    Note(String),
    /// The built-in key can't be read: nothing is prepared, never
    /// unsigned.
    Key(KeyError),
    /// `release.json.sig` is missing, or doesn't sign `release.json`.
    Signature(&'static str),
    Manifest(ManifestError),
    /// `release.json` describes another download than the note.
    Mismatch(&'static str),
    /// The archive isn't the one `release.json` describes.
    Download(String),
    Unpack(String),
    Io(io::Error),
}

impl Failure {
    fn kind(&self) -> &'static str {
        match self {
            Self::Note(_) => "note",
            Self::Key(_) => "key",
            Self::Signature(_) => "signature",
            Self::Manifest(_) => "manifest",
            Self::Mismatch(_) => "mismatch",
            Self::Download(_) => "download",
            Self::Unpack(_) => "unpack",
            Self::Io(_) => "io",
        }
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Note(error) => write!(f, "the download's note is wrong: {error}"),
            Self::Key(error) => error.fmt(f),
            Self::Signature(error) => f.write_str(error),
            Self::Manifest(error) => error.fmt(f),
            Self::Mismatch(what) => write!(f, "release.json and the note differ in the {what}"),
            Self::Download(error) => write!(f, "the download is wrong: {error}"),
            Self::Unpack(error) => write!(f, "can't unpack: {error}"),
            Self::Io(error) => write!(f, "can't prepare: {error}"),
        }
    }
}

impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Prepares the download `note` describes, holding the install lock for
/// the whole run, and returns the exit code. `key` is the built-in key,
/// `platform` this system's entry in `release.json`, and `wait` how long
/// to wait for the lock.
pub fn run(
    layout: &Layout,
    note: &Path,
    key: Result<Option<VerifyingKey>, KeyError>,
    platform: Option<&str>,
    wait: Duration,
) -> i32 {
    let started = Instant::now();
    let file = match download_named(layout, note) {
        Ok(file) => file,
        Err(error) => {
            log::line(format!("not preparing: {error}"));
            return FAILED;
        }
    };
    let _lock = match update::lock_install_within(layout, wait) {
        Ok(Some(lock)) => lock,
        Ok(None) => {
            log::line(format!(
                "not preparing: the install folder stayed locked for {} s",
                wait.as_secs()
            ));
            return BUSY;
        }
        Err(error) => {
            return failed(layout, &file, &Failure::Io(error), started);
        }
    };
    // What the last prepare of this download said is stale now.
    let error_file = error_file(layout, &file);
    if let Err(error) = remove_file_if_there(&error_file) {
        log::line(format!(
            "not preparing: can't remove the last error: {error}"
        ));
        return FAILED;
    }
    let prepared = key.map_err(Failure::Key).and_then(|key| {
        let platform = platform.ok_or(Failure::Manifest(ManifestError::NoPlatform))?;
        prepare(layout, &file, key.as_ref(), platform, now_ms())
    });
    match prepared {
        Ok(version) => {
            log::line(format!(
                "prepared {version}, after_ms={}",
                started.elapsed().as_millis()
            ));
            PREPARED
        }
        Err(failure) => failed(layout, &file, &failure, started),
    }
}

/// `<file>` for a note at `downloads/<file>.json`; anything else is refused,
/// since the archive and `release.json` are read beside it.
fn download_named(layout: &Layout, note: &Path) -> Result<String, String> {
    let file = note
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_suffix(".json"))
        .filter(|file| plain_name(file))
        .ok_or("the note isn't a <file>.json")?;
    let folder = note
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty())
        .ok_or("the note has no folder")?;
    let folder = fs::canonicalize(folder).map_err(|error| format!("the note's folder: {error}"))?;
    let downloads = fs::canonicalize(layout.downloads())
        .map_err(|error| format!("the downloads folder: {error}"))?;
    if folder != downloads {
        return Err("the note isn't in the install folder's downloads".into());
    }
    Ok(file.to_owned())
}

/// A file name with no path in it.
fn plain_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\\']) && name != "." && name != ".."
}

fn error_file(layout: &Layout, file: &str) -> std::path::PathBuf {
    layout.downloads().join(format!("{file}.error"))
}

/// Logs `failure` and writes it to `downloads/<file>.error`, on one line.
fn failed(layout: &Layout, file: &str, failure: &Failure, started: Instant) -> i32 {
    let line = one_line(&format!("{}: {failure}", failure.kind()));
    log::line(format!(
        "prepare failed: {line}, after_ms={}",
        started.elapsed().as_millis()
    ));
    if let Err(error) =
        layout::write_whole(&error_file(layout, file), format!("{line}\n").as_bytes())
    {
        log::line(format!("can't write the prepare's error file: {error}"));
    }
    FAILED
}

/// `text` with control characters, newlines among them, made spaces.
fn one_line(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// The checks and the unpacking, in the design's order. Returns the
/// version now waiting.
fn prepare(
    layout: &Layout,
    file: &str,
    key: Option<&VerifyingKey>,
    platform: &str,
    now: u64,
) -> Result<Version, Failure> {
    let downloads = layout.downloads();
    let note = read_note(&downloads.join(format!("{file}.json")))?;
    if note.file != file {
        return Err(Failure::Note("it names another file".into()));
    }
    log::line(format!("preparing {} ({} bytes)", note.version, note.size));

    let manifest = read_limited(&downloads.join(MANIFEST_FILE), MANIFEST_LIMIT)?;
    if let Some(key) = key {
        check_signature(key, &manifest, &downloads.join(SIGNATURE_FILE))?;
    }
    let manifest = Manifest::parse(&manifest, &note.tag, platform).map_err(Failure::Manifest)?;
    let track = matching(&manifest, &note, platform)?;

    let archive = downloads.join(&note.file);
    check_archive(&archive, &note)?;
    unpack(layout, &note.version, &archive, &note.file)?;

    layout.write_pending(&Pending {
        version: note.version.clone(),
        track: track.name().to_owned(),
        prepared_at: now,
        confirmed_at: None,
    })?;
    Ok(note.version)
}

fn read_note(path: &Path) -> Result<DownloadNote, Failure> {
    let bytes = read_limited(path, NOTE_LIMIT).map_err(|error| Failure::Note(error.to_string()))?;
    serde_json::from_slice(&bytes).map_err(|error| Failure::Note(error.to_string()))
}

/// Reads `path` whole, refusing it past `limit` bytes.
fn read_limited(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} is over {limit} bytes",
                path.file_name().unwrap_or_default().to_string_lossy()
            ),
        ));
    }
    Ok(bytes)
}

fn check_signature(key: &VerifyingKey, manifest: &[u8], path: &Path) -> Result<(), Failure> {
    let sig = match read_limited(path, SIGNATURE_LIMIT) {
        Ok(sig) => sig,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(Failure::Signature("release.json.sig is missing"));
        }
        Err(error) => return Err(Failure::Io(error)),
    };
    if !signature::verify(key, manifest, &sig) {
        return Err(Failure::Signature(
            "release.json isn't signed by Opencord's key",
        ));
    }
    Ok(())
}

/// Checks `release.json` describes the download the note does: the same
/// version, a track the note names, and this system's file with its size
/// and SHA-256. Returns the track.
fn matching(manifest: &Manifest, note: &DownloadNote, platform: &str) -> Result<Track, Failure> {
    if manifest.version != note.version {
        return Err(Failure::Mismatch("version"));
    }
    let track = Track::of(&manifest.version)
        .filter(|track| Track::parse(&note.track) == Some(*track))
        .ok_or(Failure::Mismatch("track"))?;
    let asset = manifest.asset(platform).map_err(Failure::Manifest)?;
    if asset.file != note.file {
        return Err(Failure::Mismatch("file"));
    }
    if asset.size != note.size {
        return Err(Failure::Mismatch("size"));
    }
    if !asset.sha256.eq_ignore_ascii_case(&note.sha256) {
        return Err(Failure::Mismatch("SHA-256"));
    }
    Ok(track)
}

/// Checks the archive's size and SHA-256 against the note, which
/// [`matching`] has checked against `release.json`.
fn check_archive(path: &Path, note: &DownloadNote) -> Result<(), Failure> {
    let size = fs::metadata(path)
        .map_err(|error| Failure::Download(format!("{}: {error}", note.file)))?
        .len();
    if size != note.size {
        return Err(Failure::Download(format!(
            "{} is {size} bytes, not {}",
            note.file, note.size
        )));
    }
    let hash = sha256_of(&mut File::open(path)?.take(note.size))?;
    if hash.hashed != note.size {
        return Err(Failure::Download(format!(
            "{} ended after {} bytes",
            note.file, hash.hashed
        )));
    }
    if !hash.hex.eq_ignore_ascii_case(&note.sha256) {
        return Err(Failure::Download(format!(
            "{}'s SHA-256 isn't release.json's",
            note.file
        )));
    }
    Ok(())
}

/// What [`sha256_of`] read: how many bytes, and their SHA-256 in hex.
struct Hashed {
    hashed: u64,
    hex: String,
}

/// Hashes `reader` to its end, a buffer at a time: an archive is too big
/// to read whole.
fn sha256_of(reader: &mut impl Read) -> io::Result<Hashed> {
    let mut hash = Sha256::new();
    let mut buf = vec![0; 64 << 10];
    let mut hashed = 0;
    loop {
        let read = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        hash.update(&buf[..read]);
        hashed += read as u64;
    }
    Ok(Hashed {
        hashed,
        hex: hex::encode(hash.finalize()),
    })
}

/// Unpacks `archive` to `app-<version>.partial`, checks the app is there,
/// and renames the folder. A folder already whole is left as it is: it may
/// be the version running now. A failure leaves no `.partial` folder.
fn unpack(layout: &Layout, version: &Version, archive: &Path, name: &str) -> Result<(), Failure> {
    if layout.is_whole(version) {
        log::line(format!("{version} is already unpacked"));
        return Ok(());
    }
    let partial = layout.partial_dir(version);
    // Left by a prepare or an update that was killed part way.
    remove_dir_if_there(&partial)?;
    let unpacked = archive::unpack(archive, name, &partial)
        .map_err(|error| Failure::Unpack(error.to_string()))
        .and_then(|()| {
            if layout::app_executable(&partial).is_file() {
                Ok(())
            } else {
                Err(Failure::Unpack(format!(
                    "{name} has no {}",
                    layout::app_executable(Path::new("")).display()
                )))
            }
        })
        .and_then(|()| {
            // Not whole, so no version runs from it: an empty folder at
            // most.
            remove_dir_if_there(&layout.version_dir(version))?;
            fs::rename(&partial, layout.version_dir(version))?;
            Ok(())
        });
    if unpacked.is_err()
        && let Err(error) = remove_dir_if_there(&partial)
    {
        log::line(format!("can't remove the half-unpacked folder: {error}"));
    }
    unpacked
}

fn remove_dir_if_there(path: &Path) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

fn remove_file_if_there(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use data_encoding::BASE64;
    use ed25519_dalek::{Signer, SigningKey};
    use opencord_update::layout::Installed;
    use opencord_update::pending;

    use super::*;

    const PLATFORM: &str = "client-linux-x64";
    const FILE: &str = "opencord-linux-x64.tar.gz";
    /// Long enough that a test never sees it run out.
    const WAIT: Duration = Duration::from_secs(30);

    fn v(version: &str) -> Version {
        Version::parse(version).unwrap()
    }

    /// The key CI would sign with: a fixed test key, never a real one.
    fn ci_key() -> SigningKey {
        SigningKey::from_bytes(&[7; 32])
    }

    fn key() -> Result<Option<VerifyingKey>, KeyError> {
        Ok(Some(ci_key().verifying_key()))
    }

    /// An app archive with the executable where this system's launcher
    /// looks for it. A `.tar.gz` unpacks on every system.
    fn app_archive(marker: &[u8]) -> Vec<u8> {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        let mut header = tar::Header::new_gnu();
        header.set_size(marker.len() as u64);
        header.set_mode(0o755);
        builder
            .append_data(&mut header, layout::app_executable(Path::new("")), marker)
            .unwrap();
        builder.into_inner().unwrap().finish().unwrap()
    }

    /// What the app leaves in `downloads/` once a download is done.
    struct Download {
        version: String,
        archive: Vec<u8>,
        /// `release.json`'s platform entry.
        platform: &'static str,
        signer: SigningKey,
        /// The note's track.
        track: &'static str,
    }

    impl Download {
        fn of(version: &str, archive: Vec<u8>) -> Self {
            Self {
                version: version.into(),
                archive,
                platform: PLATFORM,
                signer: ci_key(),
                track: "beta",
            }
        }

        /// Writes the archive, `release.json`, its signature and the note;
        /// returns the note's path.
        fn write(&self, layout: &Layout) -> std::path::PathBuf {
            let downloads = layout.downloads();
            fs::create_dir_all(&downloads).unwrap();
            fs::write(downloads.join(FILE), &self.archive).unwrap();
            let sha256 = hex::encode(Sha256::digest(&self.archive));
            let manifest = serde_json::json!({
                "schema": 1,
                "version": self.version,
                "channel": "beta",
                "tag": format!("v{}", self.version),
                "commit": "abc",
                "platforms": {
                    self.platform: {
                        "file": FILE,
                        "url": format!("https://example.com/{FILE}"),
                        "size": self.archive.len(),
                        "sha256": sha256,
                    }
                }
            })
            .to_string();
            fs::write(downloads.join(MANIFEST_FILE), &manifest).unwrap();
            let signature = BASE64.encode(&self.signer.sign(manifest.as_bytes()).to_bytes());
            fs::write(downloads.join(SIGNATURE_FILE), format!("{signature}\n")).unwrap();
            layout
                .write_download_note(&DownloadNote {
                    tag: format!("v{}", self.version),
                    version: v(&self.version),
                    track: self.track.into(),
                    file: FILE.into(),
                    url: format!("https://example.com/{FILE}"),
                    size: self.archive.len() as u64,
                    sha256,
                })
                .unwrap();
            downloads.join(format!("{FILE}.json"))
        }
    }

    /// An install folder with `version` installed on beta.
    fn installed(dir: &Path, version: &str) -> Layout {
        let layout = Layout::new(dir);
        whole_app(&layout, version, b"installed");
        layout
            .write_installed(&Installed {
                version: Some(v(version)),
                track: Some("beta".into()),
            })
            .unwrap();
        layout
    }

    fn whole_app(layout: &Layout, version: &str, marker: &[u8]) {
        let exe = layout::app_executable(&layout.version_dir(&v(version)));
        fs::create_dir_all(exe.parent().unwrap()).unwrap();
        fs::write(exe, marker).unwrap();
    }

    fn prepare_with(
        layout: &Layout,
        note: &Path,
        key: Result<Option<VerifyingKey>, KeyError>,
    ) -> i32 {
        run(layout, note, key, Some(PLATFORM), WAIT)
    }

    /// The error file's line, if there is one.
    fn error_line(layout: &Layout) -> Option<String> {
        fs::read_to_string(error_file(layout, FILE)).ok()
    }

    /// Nothing was prepared: no waiting update, no folder for `version`,
    /// whole or half, and the lock is free again.
    fn nothing_prepared(layout: &Layout, version: &str) {
        assert!(layout.read_pending().is_none());
        assert!(!layout.version_dir(&v(version)).exists());
        assert!(!layout.partial_dir(&v(version)).exists());
        assert!(
            update::lock_install_within(layout, Duration::from_millis(500))
                .unwrap()
                .is_some(),
            "the lock is released"
        );
    }

    // Fails if `prepare` leaves out `layout.write_pending`, or if `unpack`
    // doesn't rename the partial folder into place.
    #[test]
    fn a_good_download_is_prepared_and_waits() {
        let dir = tempfile::tempdir().unwrap();
        let layout = installed(dir.path(), "0.0.2-beta.1");
        // Prune would remove this one; a prepare doesn't prune.
        whole_app(&layout, "0.0.1-beta.9", b"older");
        whole_app(&layout, "0.0.2-beta.0", b"old");
        let note = Download::of("0.0.2-beta.2", app_archive(b"new")).write(&layout);
        let before = now_ms();

        assert_eq!(prepare_with(&layout, &note, key()), PREPARED);

        let waiting = layout.read_pending().unwrap();
        assert_eq!(waiting.version, v("0.0.2-beta.2"));
        assert_eq!(waiting.track, "beta");
        assert!((before..=now_ms()).contains(&waiting.prepared_at));
        assert_eq!(waiting.confirmed_at, None);
        let exe = layout::app_executable(&layout.version_dir(&v("0.0.2-beta.2")));
        assert_eq!(fs::read(exe).unwrap(), b"new");
        assert!(!layout.partial_dir(&v("0.0.2-beta.2")).exists());
        assert!(error_line(&layout).is_none());
        // installed.json and the other versions are as they were.
        assert_eq!(layout.read_installed().version, Some(v("0.0.2-beta.1")));
        assert!(layout.is_whole(&v("0.0.1-beta.9")));
        assert!(layout.is_whole(&v("0.0.2-beta.0")));
        // The next start's rule takes it.
        assert_eq!(
            pending::switchable(&layout, Track::Beta, now_ms(), None),
            Some(v("0.0.2-beta.2"))
        );
    }

    // Fails if `prepare` skips `check_signature` when it has a key.
    #[test]
    fn a_bad_signature_prepares_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let layout = installed(dir.path(), "0.0.2-beta.1");
        let mut download = Download::of("0.0.2-beta.2", app_archive(b"new"));
        download.signer = SigningKey::from_bytes(&[8; 32]);
        let note = download.write(&layout);

        assert_eq!(prepare_with(&layout, &note, key()), FAILED);

        nothing_prepared(&layout, "0.0.2-beta.2");
        assert!(error_line(&layout).unwrap().starts_with("signature: "));

        // A missing signature is refused the same way.
        fs::remove_file(layout.downloads().join(SIGNATURE_FILE)).unwrap();
        assert_eq!(prepare_with(&layout, &note, key()), FAILED);
        nothing_prepared(&layout, "0.0.2-beta.2");
        assert_eq!(
            error_line(&layout).unwrap(),
            "signature: release.json.sig is missing\n"
        );

        // The other side: a launcher built without a key trusts HTTPS, and
        // the same download prepares.
        assert_eq!(prepare_with(&layout, &note, Ok(None)), PREPARED);
        assert!(layout.is_whole(&v("0.0.2-beta.2")));
    }

    // Fails if `run` takes a key it can't read as no key
    // (`key.unwrap_or(None)`), and so prepares unsigned.
    #[test]
    fn an_unreadable_key_prepares_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let layout = installed(dir.path(), "0.0.2-beta.1");
        let note = Download::of("0.0.2-beta.2", app_archive(b"new")).write(&layout);

        assert_eq!(prepare_with(&layout, &note, Err(KeyError)), FAILED);

        nothing_prepared(&layout, "0.0.2-beta.2");
        assert!(error_line(&layout).unwrap().starts_with("key: "));
    }

    // Fails if `check_archive` doesn't compare the SHA-256: the note and
    // release.json agree, and the size is right.
    #[test]
    fn a_wrong_hash_prepares_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let layout = installed(dir.path(), "0.0.2-beta.1");
        let note = Download::of("0.0.2-beta.2", app_archive(b"new")).write(&layout);
        let mut archive = fs::read(layout.downloads().join(FILE)).unwrap();
        let last = archive.len() - 1;
        archive[last] ^= 0xff;
        fs::write(layout.downloads().join(FILE), &archive).unwrap();

        assert_eq!(prepare_with(&layout, &note, key()), FAILED);

        nothing_prepared(&layout, "0.0.2-beta.2");
        assert_eq!(
            error_line(&layout).unwrap(),
            format!("download: the download is wrong: {FILE}'s SHA-256 isn't release.json's\n")
        );
    }

    // Fails if `prepare` takes any platform's entry from release.json
    // rather than `platform`'s (`Manifest::parse(…, platform)`).
    #[test]
    fn another_platforms_manifest_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let layout = installed(dir.path(), "0.0.2-beta.1");
        let mut download = Download::of("0.0.2-beta.2", app_archive(b"new"));
        download.platform = "client-windows-x64";
        let note = download.write(&layout);

        assert_eq!(prepare_with(&layout, &note, key()), FAILED);

        nothing_prepared(&layout, "0.0.2-beta.2");
        assert_eq!(
            error_line(&layout).unwrap(),
            "manifest: the release has no build for this system\n"
        );
    }

    // Fails if `matching` leaves out its track check: the signed
    // release.json is beta's, and the note says canary.
    #[test]
    fn a_note_for_another_track_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let layout = installed(dir.path(), "0.0.2-beta.1");
        let mut download = Download::of("0.0.2-beta.2", app_archive(b"new"));
        download.track = "canary";
        let note = download.write(&layout);

        assert_eq!(prepare_with(&layout, &note, key()), FAILED);

        nothing_prepared(&layout, "0.0.2-beta.2");
        assert_eq!(
            error_line(&layout).unwrap(),
            "mismatch: release.json and the note differ in the track\n"
        );
    }

    // Fails if `run` doesn't take the lock, or takes it after preparing:
    // the prepare would finish while the test holds it.
    #[test]
    fn a_second_prepare_waits_for_the_lock() {
        let dir = tempfile::tempdir().unwrap();
        let layout = installed(dir.path(), "0.0.2-beta.1");
        let note = Download::of("0.0.2-beta.2", app_archive(b"new")).write(&layout);
        let first = update::lock_install(&layout).unwrap();

        let second = {
            let layout = layout.clone();
            std::thread::spawn(move || prepare_with(&layout, &note, key()))
        };
        std::thread::sleep(Duration::from_millis(500));
        assert!(!second.is_finished(), "the prepare waits for the lock");
        assert!(layout.read_pending().is_none());
        assert!(!layout.partial_dir(&v("0.0.2-beta.2")).exists());
        drop(first);

        assert_eq!(second.join().unwrap(), PREPARED);
        assert_eq!(layout.read_pending().unwrap().version, v("0.0.2-beta.2"));
    }

    // Fails if `run` waits for the lock without a limit
    // (`update::lock_install`): the test never ends.
    #[test]
    fn a_prepare_that_cant_get_the_lock_gives_up_with_3() {
        let dir = tempfile::tempdir().unwrap();
        let layout = installed(dir.path(), "0.0.2-beta.1");
        let note = Download::of("0.0.2-beta.2", app_archive(b"new")).write(&layout);
        let _held = update::lock_install(&layout).unwrap();

        let code = run(
            &layout,
            &note,
            key(),
            Some(PLATFORM),
            Duration::from_millis(200),
        );

        assert_eq!(code, BUSY);
        assert!(layout.read_pending().is_none());
        assert!(!layout.version_dir(&v("0.0.2-beta.2")).exists());
    }

    // Fails if `unpack` leaves the partial folder behind on an error, or if
    // `failed` writes no error file; the last part fails if `run` keeps the
    // last prepare's error file.
    #[test]
    fn a_failed_prepare_writes_its_error_file_and_leaves_no_partial() {
        let dir = tempfile::tempdir().unwrap();
        let layout = installed(dir.path(), "0.0.2-beta.1");
        // An archive without the app: it unpacks, then fails the check.
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        let mut header = tar::Header::new_gnu();
        header.set_size(1);
        builder
            .append_data(&mut header, "opencord/readme", &b"x"[..])
            .unwrap();
        let archive = builder.into_inner().unwrap().finish().unwrap();
        let note = Download::of("0.0.2-beta.2", archive).write(&layout);

        assert_eq!(prepare_with(&layout, &note, key()), FAILED);

        nothing_prepared(&layout, "0.0.2-beta.2");
        let line = error_line(&layout).unwrap();
        assert!(line.starts_with("unpack: "), "{line}");
        assert_eq!(line.lines().count(), 1);
        assert!(
            !layout
                .downloads()
                .join(format!("{FILE}.error.partial"))
                .exists()
        );

        // The app downloads it again, whole: the old error goes.
        let note = Download::of("0.0.2-beta.2", app_archive(b"new")).write(&layout);
        assert_eq!(prepare_with(&layout, &note, key()), PREPARED);
        assert!(error_line(&layout).is_none());
    }

    // Fails if `unpack` replaces a version that is already whole: it may be
    // the one running.
    #[test]
    fn a_version_already_unpacked_is_left_as_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let layout = installed(dir.path(), "0.0.2-beta.1");
        whole_app(&layout, "0.0.2-beta.2", b"running");
        let note = Download::of("0.0.2-beta.2", app_archive(b"new")).write(&layout);

        assert_eq!(prepare_with(&layout, &note, key()), PREPARED);

        let exe = layout::app_executable(&layout.version_dir(&v("0.0.2-beta.2")));
        assert_eq!(fs::read(exe).unwrap(), b"running");
        assert_eq!(layout.read_pending().unwrap().version, v("0.0.2-beta.2"));
    }

    // Fails if `download_named` reads a note outside `downloads/`: the
    // archive and release.json beside it would be another folder's.
    #[test]
    fn a_note_outside_downloads_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let layout = installed(dir.path(), "0.0.2-beta.1");
        let note = Download::of("0.0.2-beta.2", app_archive(b"new")).write(&layout);
        let elsewhere = dir.path().join("elsewhere");
        fs::create_dir_all(&elsewhere).unwrap();
        for name in [
            format!("{FILE}.json"),
            FILE.into(),
            MANIFEST_FILE.into(),
            SIGNATURE_FILE.into(),
        ] {
            fs::copy(layout.downloads().join(&name), elsewhere.join(&name)).unwrap();
        }

        let outside = elsewhere.join(format!("{FILE}.json"));
        assert_eq!(prepare_with(&layout, &outside, key()), FAILED);
        assert_eq!(prepare_with(&layout, Path::new(""), key()), FAILED);

        nothing_prepared(&layout, "0.0.2-beta.2");
        assert!(error_line(&layout).is_none());
        // The other side: the same note in downloads/ prepares.
        assert_eq!(prepare_with(&layout, &note, key()), PREPARED);
    }
}
