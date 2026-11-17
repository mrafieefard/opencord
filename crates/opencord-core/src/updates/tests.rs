//! `Updater::check_once` against a local server standing in for GitHub
//! and its CDN, with releases signed by a fixed test key.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use axum::extract::State;
use axum::http::header::{CONTENT_RANGE, RANGE};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use data_encoding::BASE64;
use ed25519_dalek::{Signer, SigningKey};
use semver::Version;
use sha2::{Digest, Sha256};

use opencord_update::layout::{self, Layout};
use opencord_update::pending::{DownloadNote, Pending};
use opencord_update::track::Track;

use super::download::CHUNK;
use super::github::Github;
use super::{CheckOutcome, Unlimited, UpdateError, Updater};

const PLATFORM: &str = "client-linux-x64";
const FILE: &str = "opencord-linux-x64.tar.gz";
const NEWEST: &str = "0.0.2-beta.2";

/// What the server holds, and every request it was sent: the path with
/// its query, and the `Range` header.
struct Served {
    files: HashMap<String, Vec<u8>>,
    honours_range: bool,
    asked: Mutex<Vec<(String, Option<String>)>>,
}

struct Server {
    base: String,
    served: Arc<Served>,
}

impl Server {
    /// The archive requests made, with their `Range` headers.
    fn archive_requests(&self) -> Vec<Option<String>> {
        self.served
            .asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|(path, _)| path.starts_with("/assets/"))
            .map(|(_, range)| range.clone())
            .collect()
    }
}

async fn serve(State(served): State<Arc<Served>>, uri: Uri, headers: HeaderMap) -> Response {
    let path = uri
        .path_and_query()
        .map(|path| path.as_str().to_owned())
        .unwrap_or_default();
    let range = headers
        .get(RANGE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    served
        .asked
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push((path.clone(), range.clone()));
    let Some(body) = served.files.get(&path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let wanted = range.filter(|_| served.honours_range).and_then(|range| {
        let (first, last) = range.strip_prefix("bytes=")?.split_once('-')?;
        Some((first.parse::<usize>().ok()?, last.parse::<usize>().ok()?))
    });
    match wanted {
        Some((first, last)) => {
            let last = last.min(body.len() - 1);
            (
                StatusCode::PARTIAL_CONTENT,
                [(
                    CONTENT_RANGE,
                    format!("bytes {first}-{last}/{}", body.len()),
                )],
                body[first..=last].to_vec(),
            )
                .into_response()
        }
        None => (StatusCode::OK, body.clone()).into_response(),
    }
}

/// Starts a server whose files `files` makes from its base URL.
async fn start(
    honours_range: bool,
    files: impl FnOnce(&str) -> HashMap<String, Vec<u8>>,
) -> Server {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let served = Arc::new(Served {
        files: files(&base),
        honours_range,
        asked: Mutex::new(Vec::new()),
    });
    let app = axum::Router::new()
        .fallback(serve)
        .with_state(Arc::clone(&served));
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    Server { base, served }
}

fn signer() -> SigningKey {
    SigningKey::from_bytes(&[5; 32])
}

fn sign(key: &SigningKey, bytes: &[u8]) -> Vec<u8> {
    BASE64.encode(&key.sign(bytes).to_bytes()).into_bytes()
}

/// An archive over two chunks long, so a download takes several requests.
fn archive() -> Vec<u8> {
    (0..(CHUNK as usize * 5 / 2))
        .map(|i| (i * 31 % 251) as u8)
        .collect()
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn asset_url(base: &str, version: &str) -> String {
    format!("{base}/assets/{version}/{FILE}")
}

fn manifest(base: &str, version: &str, archive: &[u8]) -> Vec<u8> {
    serde_json::json!({
        "schema": 1,
        "version": version,
        "tag": format!("v{version}"),
        "platforms": {
            PLATFORM: {
                "file": FILE,
                "url": asset_url(base, version),
                "size": archive.len(),
                "sha256": sha256(archive),
            }
        }
    })
    .to_string()
    .into_bytes()
}

fn release_url(version: &str) -> String {
    format!("/web/o/r/releases/download/v{version}/release.json")
}

const LISTING: &str = "/api/repos/o/r/releases?per_page=100";

/// A beta listing whose newest is [`NEWEST`], with its manifest, its
/// archive, and the signature `signature` makes of the manifest (none
/// when it gives `None`).
fn beta_release(
    base: &str,
    archive: &[u8],
    signature: impl FnOnce(&[u8]) -> Option<Vec<u8>>,
) -> HashMap<String, Vec<u8>> {
    let manifest = manifest(base, NEWEST, archive);
    let mut files = HashMap::new();
    files.insert(
        LISTING.to_owned(),
        br#"[{"tag_name":"v0.0.2-beta.2"},{"tag_name":"v0.0.2-beta.1"},{"tag_name":"v0.0.1"}]"#
            .to_vec(),
    );
    if let Some(signature) = signature(&manifest) {
        files.insert(format!("{}.sig", release_url(NEWEST)), signature);
    }
    files.insert(release_url(NEWEST), manifest);
    files.insert(format!("/assets/{NEWEST}/{FILE}"), archive.to_vec());
    files
}

fn client() -> reqwest::Client {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let tls = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(rustls::RootCertStore::empty())
        .with_no_client_auth();
    reqwest::Client::builder()
        .tls_backend_preconfigured(tls)
        .build()
        .unwrap()
}

/// The updater as the app makes it, with the test key and the local
/// server in place of the built-in key and GitHub.
fn updater(root: &Path, server: &Server, track: Track, installed: &str) -> Updater {
    Updater {
        key: Ok(Some(signer().verifying_key())),
        github: Github {
            web: format!("{}/web", server.base),
            api: format!("{}/api", server.base),
            repository: "o/r".to_owned(),
        },
        ..Updater::new(
            root.to_owned(),
            track,
            Version::parse(installed).unwrap(),
            PLATFORM,
            client(),
            Arc::new(Unlimited),
        )
    }
}

/// The note the updater writes for [`NEWEST`]'s archive.
fn note(server: &Server, archive: &[u8]) -> DownloadNote {
    DownloadNote {
        tag: format!("v{NEWEST}"),
        version: Version::parse(NEWEST).unwrap(),
        track: "beta".to_owned(),
        file: FILE.to_owned(),
        url: asset_url(&server.base, NEWEST),
        size: archive.len() as u64,
        sha256: sha256(archive),
    }
}

/// A part file left by an earlier check, with the note it wrote.
fn left_part(layout: &Layout, note: &DownloadNote, part: &[u8]) {
    layout.write_download_note(note).unwrap();
    fs::write(layout.downloads().join(format!("{FILE}.part")), part).unwrap();
}

fn unpacked(layout: &Layout, version: &Version) {
    let exe = layout::app_executable(&layout.version_dir(version));
    fs::create_dir_all(exe.parent().unwrap()).unwrap();
    fs::write(exe, b"app").unwrap();
}

// Fails if `check` returns `Downloaded` before writing `release.json.sig`
// or `release.json`, or if `fetch` stops ranging in `CHUNK`s.
#[tokio::test]
async fn a_newer_signed_release_is_downloaded_for_prepare() {
    let archive = archive();
    let server = start(true, |base| {
        beta_release(base, &archive, |manifest| Some(sign(&signer(), manifest)))
    })
    .await;
    let root = tempfile::tempdir().unwrap();
    let layout = Layout::new(root.path());

    let outcome = updater(root.path(), &server, Track::Beta, "0.0.2-beta.1")
        .check_once()
        .await
        .unwrap();

    let downloads = layout.downloads();
    assert_eq!(
        outcome,
        CheckOutcome::Downloaded {
            version: Version::parse(NEWEST).unwrap(),
            download_json: downloads.join(format!("{FILE}.json")),
        }
    );
    assert_eq!(fs::read(downloads.join(FILE)).unwrap(), archive);
    assert!(!downloads.join(format!("{FILE}.part")).exists());
    let manifest = fs::read(downloads.join("release.json")).unwrap();
    assert_eq!(manifest, server.served.files[&release_url(NEWEST)]);
    assert_eq!(
        fs::read(downloads.join("release.json.sig")).unwrap(),
        sign(&signer(), &manifest)
    );
    assert_eq!(
        layout.read_download_note(&downloads.join(format!("{FILE}.json"))),
        Some(note(&server, &archive))
    );
    let size = archive.len() as u64;
    assert_eq!(
        server.archive_requests(),
        [
            Some(format!("bytes=0-{}", CHUNK - 1)),
            Some(format!("bytes={CHUNK}-{}", 2 * CHUNK - 1)),
            Some(format!("bytes={}-{}", 2 * CHUNK, size - 1)),
        ]
    );
}

// Fails if `manifest` skips `signature::verify` (the forged case), or
// ignores the key and fetches no `.sig` (`match key` taking `None`: the
// unsigned case).
#[tokio::test]
async fn an_unsigned_release_is_refused_with_a_key() {
    let archive = archive();
    let forger = SigningKey::from_bytes(&[6; 32]);
    type Signs = Box<dyn Fn(&[u8]) -> Option<Vec<u8>> + Send>;
    let cases: [(&str, Signs); 2] = [
        ("unsigned", Box::new(|_| None)),
        (
            "forged",
            Box::new(move |manifest| Some(sign(&forger, manifest))),
        ),
    ];
    for (case, signature) in cases {
        let server = start(true, |base| beta_release(base, &archive, signature)).await;
        let root = tempfile::tempdir().unwrap();

        let outcome = updater(root.path(), &server, Track::Beta, "0.0.2-beta.1")
            .check_once()
            .await;

        assert!(
            matches!(outcome, Err(UpdateError::Signature(_))),
            "{case}: {outcome:?}"
        );
        assert_eq!(
            server.archive_requests(),
            Vec::<Option<String>>::new(),
            "{case}"
        );
        assert!(!Layout::new(root.path()).downloads().exists(), "{case}");
    }
}

// Fails if `check` downloads without asking `waiting` first, or if
// `waiting` ignores whether the folder is whole (the second half).
#[tokio::test]
async fn the_same_waiting_version_is_confirmed_not_downloaded() {
    let archive = archive();
    let server = start(true, |base| {
        beta_release(base, &archive, |manifest| Some(sign(&signer(), manifest)))
    })
    .await;
    let root = tempfile::tempdir().unwrap();
    let layout = Layout::new(root.path());
    let newest = Version::parse(NEWEST).unwrap();
    unpacked(&layout, &newest);
    // Prepared over a day ago, so only a confirmation keeps it fresh.
    layout
        .write_pending(&Pending {
            version: newest.clone(),
            track: "beta".to_owned(),
            prepared_at: 1_000,
            confirmed_at: None,
        })
        .unwrap();
    let updater = updater(root.path(), &server, Track::Beta, "0.0.2-beta.1");

    let outcome = updater.check_once().await.unwrap();

    assert_eq!(outcome, CheckOutcome::AlreadyWaiting(newest.clone()));
    let confirmed = layout.read_pending().unwrap().confirmed_at.unwrap();
    assert!(confirmed > 1_000_000_000_000, "confirmed at {confirmed}");
    assert_eq!(server.archive_requests(), Vec::<Option<String>>::new());

    // The same note with its folder gone (pruned, or half unpacked) is no
    // waiting update: it downloads.
    fs::remove_dir_all(layout.version_dir(&newest)).unwrap();
    let outcome = updater.check_once().await.unwrap();
    assert!(
        matches!(outcome, CheckOutcome::Downloaded { .. }),
        "{outcome:?}"
    );
}

// Fails if `fetch` drops the part file although the note names the same
// release (the first request would ask from byte 0).
#[tokio::test]
async fn a_download_resumes_from_its_part_file() {
    let archive = archive();
    let server = start(true, |base| {
        beta_release(base, &archive, |manifest| Some(sign(&signer(), manifest)))
    })
    .await;
    let root = tempfile::tempdir().unwrap();
    let layout = Layout::new(root.path());
    let have = CHUNK as usize + 100;
    left_part(&layout, &note(&server, &archive), &archive[..have]);

    let outcome = updater(root.path(), &server, Track::Beta, "0.0.2-beta.1")
        .check_once()
        .await
        .unwrap();

    assert!(
        matches!(outcome, CheckOutcome::Downloaded { .. }),
        "{outcome:?}"
    );
    assert_eq!(fs::read(layout.downloads().join(FILE)).unwrap(), archive);
    let size = archive.len();
    assert_eq!(
        server.archive_requests(),
        [
            Some(format!("bytes={have}-{}", have + CHUNK as usize - 1)),
            Some(format!("bytes={}-{}", have + CHUNK as usize, size - 1)),
        ]
    );
}

// Review Focus 3. Fails if the 200 branch of `chunk` appends to the part
// file instead of `file.set_len(0)` first: the stale bytes stay in front
// and the hash refuses the file.
#[tokio::test]
async fn a_server_ignoring_range_starts_over() {
    let archive = archive();
    let server = start(false, |base| {
        beta_release(base, &archive, |manifest| Some(sign(&signer(), manifest)))
    })
    .await;
    let root = tempfile::tempdir().unwrap();
    let layout = Layout::new(root.path());
    left_part(&layout, &note(&server, &archive), &[0xAA; 1_000]);

    let outcome = updater(root.path(), &server, Track::Beta, "0.0.2-beta.1")
        .check_once()
        .await
        .unwrap();

    assert!(
        matches!(outcome, CheckOutcome::Downloaded { .. }),
        "{outcome:?}"
    );
    assert_eq!(fs::read(layout.downloads().join(FILE)).unwrap(), archive);
    // One answer with the whole file, though it asked from byte 1000.
    assert_eq!(
        server.archive_requests(),
        [Some(format!("bytes=1000-{}", 1000 + CHUNK - 1))]
    );
}

// Review Focus 3. Fails if `fetch` renames the part file without
// `hash_matches`, or keeps a part file whose hash was wrong.
#[tokio::test]
async fn a_resumed_file_with_a_wrong_byte_is_refused() {
    let archive = archive();
    let server = start(true, |base| {
        beta_release(base, &archive, |manifest| Some(sign(&signer(), manifest)))
    })
    .await;
    let root = tempfile::tempdir().unwrap();
    let layout = Layout::new(root.path());
    let mut part = archive[..CHUNK as usize + 5].to_vec();
    part[10] ^= 0xFF;
    left_part(&layout, &note(&server, &archive), &part);

    let outcome = updater(root.path(), &server, Track::Beta, "0.0.2-beta.1")
        .check_once()
        .await;

    assert!(
        matches!(outcome, Err(UpdateError::Download(_))),
        "{outcome:?}"
    );
    let downloads = layout.downloads();
    assert!(!downloads.join(FILE).exists());
    assert!(!downloads.join(format!("{FILE}.part")).exists());
    assert!(!downloads.join("release.json").exists());
}

// Fails if `latest` drops its `Track::of(..) != Some(track)` check (the
// stable case answers `UpToDate` then), or if `check::newest` stops
// filtering by track (the beta case downloads the canary).
#[tokio::test]
async fn another_tracks_release_is_never_taken() {
    let archive = archive();
    let server = start(true, |base| {
        let mut files = HashMap::new();
        files.insert(
            LISTING.to_owned(),
            br#"[{"tag_name":"v0.0.3-canary.1"},{"tag_name":"v0.0.3"}]"#.to_vec(),
        );
        for version in ["0.0.3-canary.1", "0.0.3"] {
            let manifest = manifest(base, version, &archive);
            files.insert(
                format!("{}.sig", release_url(version)),
                sign(&signer(), &manifest),
            );
            files.insert(release_url(version), manifest);
            files.insert(format!("/assets/{version}/{FILE}"), archive.clone());
        }
        // `releases/latest` serving a beta.
        let latest = "/web/o/r/releases/latest/download/release.json";
        let manifest = manifest(base, "0.0.3-beta.1", &archive);
        files.insert(format!("{latest}.sig"), sign(&signer(), &manifest));
        files.insert(latest.to_owned(), manifest);
        files
    })
    .await;
    let root = tempfile::tempdir().unwrap();

    let beta = updater(root.path(), &server, Track::Beta, "0.0.1-beta.1")
        .check_once()
        .await;
    let stable = updater(root.path(), &server, Track::Stable, "0.0.1")
        .check_once()
        .await;

    assert!(matches!(beta, Err(UpdateError::NoRelease)), "{beta:?}");
    assert!(matches!(stable, Err(UpdateError::NoRelease)), "{stable:?}");
    assert_eq!(server.archive_requests(), Vec::<Option<String>>::new());
}

// Fails if `check` skips `check::takes` and downloads the installed
// version again.
#[tokio::test]
async fn the_installed_newest_is_up_to_date() {
    let archive = archive();
    let server = start(true, |base| {
        beta_release(base, &archive, |manifest| Some(sign(&signer(), manifest)))
    })
    .await;
    let root = tempfile::tempdir().unwrap();

    let outcome = updater(root.path(), &server, Track::Beta, NEWEST)
        .check_once()
        .await
        .unwrap();

    assert_eq!(outcome, CheckOutcome::UpToDate);
    assert_eq!(server.archive_requests(), Vec::<Option<String>>::new());
}
