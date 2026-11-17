//! The resumable download (design: "The app: downloading"): the archive
//! comes in `Range` chunks of [`CHUNK`], each waiting on the throttle and
//! timed on its own, appended to `downloads/<file>.part`. It carries on
//! from the part file while `downloads/<file>.json` names the same
//! release, and is hashed whole once complete, since a resumed file can't
//! be hashed in one pass.

use std::fs::File;
use std::io::{self, Read, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Duration;

use reqwest::StatusCode;
use reqwest::header::{CONTENT_RANGE, RANGE, USER_AGENT};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::time::Instant;

use opencord_update::layout::Layout;
use opencord_update::pending::DownloadNote;

use super::github::{AGENT, network};
use super::{Throttle, UpdateError};

/// One ranged request.
pub(super) const CHUNK: u64 = 1 << 20;
/// How long one chunk may take, from its request to its last byte.
pub(super) const CHUNK_TIMEOUT: Duration = Duration::from_secs(30);

/// Downloads what `note` describes to `downloads/<file>`, resuming from
/// its part file when the last note was for the same release. Returns the
/// note's path, `downloads/<file>.json`.
pub(super) async fn fetch(
    http: &reqwest::Client,
    layout: &Layout,
    note: &DownloadNote,
    throttle: &dyn Throttle,
) -> Result<PathBuf, UpdateError> {
    let downloads = layout.downloads();
    tokio::fs::create_dir_all(&downloads).await?;
    let whole = downloads.join(&note.file);
    let part = downloads.join(format!("{}.part", note.file));
    let note_path = downloads.join(format!("{}.json", note.file));

    // The archive's name is the same in every release: only the note
    // says which one a file holds.
    let same_release = layout
        .read_download_note(&note_path)
        .is_some_and(|last| last.tag == note.tag && last.sha256.eq_ignore_ascii_case(&note.sha256));
    if same_release && tokio::fs::try_exists(&whole).await? {
        // Downloaded already, by a check whose prepare never ran.
        if hash_matches(&whole, note).await? {
            tracing::info!(version = %note.version, "the update was downloaded already");
            return Ok(note_path);
        }
        tracing::warn!(version = %note.version, "a finished download was wrong; starting over");
        remove_if_there(&part).await?;
    } else if !same_release {
        remove_if_there(&part).await?;
    }
    remove_if_there(&whole).await?;
    layout.write_download_note(note)?;

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&part)
        .await?;
    let mut have = file.metadata().await?.len();
    if have > note.size {
        file.set_len(0).await?;
        have = 0;
    }
    file.seek(SeekFrom::Start(have)).await?;
    if have > 0 {
        tracing::info!(version = %note.version, from = have, "resuming the update's download");
    }
    while have < note.size {
        let want = CHUNK.min(note.size - have);
        throttle.take(want as usize).await;
        have = chunk(http, note, &mut file, have, want, throttle).await?;
    }
    file.sync_all().await?;
    drop(file);

    if !hash_matches(&part, note).await? {
        remove_if_there(&part).await?;
        return Err(UpdateError::Download(
            "its SHA-256 isn't release.json's".to_owned(),
        ));
    }
    tokio::fs::rename(&part, &whole).await?;
    Ok(note_path)
}

/// Fetches `want` bytes from `have` and appends them to `file`, returning
/// its new length. A server that ignores `Range` answers 200 with the
/// whole file: the part file starts over from that.
async fn chunk(
    http: &reqwest::Client,
    note: &DownloadNote,
    file: &mut tokio::fs::File,
    have: u64,
    want: u64,
    throttle: &dyn Throttle,
) -> Result<u64, UpdateError> {
    let deadline = Instant::now() + CHUNK_TIMEOUT;
    let last = have + want - 1;
    let request = http
        .get(&note.url)
        .header(USER_AGENT, AGENT)
        .header(RANGE, format!("bytes={have}-{last}"))
        .send();
    let mut response = tokio::time::timeout_at(deadline, request)
        .await
        .map_err(|_| timed_out())?
        .map_err(network)?;
    match response.status() {
        StatusCode::PARTIAL_CONTENT => {
            let range = response
                .headers()
                .get(CONTENT_RANGE)
                .and_then(|value| value.to_str().ok());
            if !range.is_some_and(|range| is_range(range, have, last, note.size)) {
                return Err(UpdateError::Download(
                    "the server sent another range".to_owned(),
                ));
            }
            let end = append(
                &mut response,
                file,
                have,
                have + want,
                want,
                deadline,
                throttle,
            )
            .await?;
            if end < have + want {
                return Err(UpdateError::Download("a chunk ended early".to_owned()));
            }
            Ok(end)
        }
        StatusCode::OK => {
            tracing::info!(
                version = %note.version,
                from = have,
                "the server ignored Range; starting over"
            );
            file.set_len(0).await?;
            file.seek(SeekFrom::Start(0)).await?;
            let end = append(&mut response, file, 0, note.size, want, deadline, throttle).await?;
            if end < note.size {
                return Err(UpdateError::Download("the file ended early".to_owned()));
            }
            Ok(end)
        }
        status => Err(UpdateError::Network(format!(
            "the download answered {status}"
        ))),
    }
}

/// Appends `response`'s body to `file`, which holds `have` bytes, refusing
/// anything past `end`. `allowed` bytes are taken from the throttle
/// already; past them, each further [`CHUNK`] waits on it first and gets
/// its own [`CHUNK_TIMEOUT`].
async fn append(
    response: &mut reqwest::Response,
    file: &mut tokio::fs::File,
    mut have: u64,
    end: u64,
    mut allowed: u64,
    mut deadline: Instant,
    throttle: &dyn Throttle,
) -> Result<u64, UpdateError> {
    loop {
        if allowed == 0 && have < end {
            let more = CHUNK.min(end - have);
            throttle.take(more as usize).await;
            allowed = more;
            deadline = Instant::now() + CHUNK_TIMEOUT;
        }
        let next = tokio::time::timeout_at(deadline, response.chunk())
            .await
            .map_err(|_| timed_out())?
            .map_err(network)?;
        let Some(bytes) = next else {
            break;
        };
        let length = bytes.len() as u64;
        if have + length > end {
            return Err(UpdateError::Download(
                "the server sent more than release.json promises".to_owned(),
            ));
        }
        file.write_all(&bytes).await?;
        have += length;
        allowed = allowed.saturating_sub(length);
    }
    file.flush().await?;
    Ok(have)
}

/// Whether `Content-Range` is `bytes <first>-<last>/<size>` (or `/*`).
fn is_range(header: &str, first: u64, last: u64, size: u64) -> bool {
    let Some((range, total)) = header
        .strip_prefix("bytes ")
        .and_then(|rest| rest.split_once('/'))
    else {
        return false;
    };
    range == format!("{first}-{last}") && (total == "*" || total == size.to_string())
}

/// Whether `path` is `note`'s file: its size and SHA-256. Hashing a whole
/// archive runs off the async threads.
async fn hash_matches(path: &Path, note: &DownloadNote) -> Result<bool, UpdateError> {
    let path = path.to_owned();
    let size = note.size;
    let (hashed, hex) = tokio::task::spawn_blocking(move || sha256_of(&path))
        .await
        .map_err(|error| UpdateError::Io(io::Error::other(error.to_string())))??;
    Ok(hashed == size && hex.eq_ignore_ascii_case(&note.sha256))
}

/// The bytes read and the SHA-256 of `path`, in hex.
fn sha256_of(path: &Path) -> io::Result<(u64, String)> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 64 << 10];
    let mut hashed = 0;
    loop {
        let read = match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        hash.update(&buffer[..read]);
        hashed += read as u64;
    }
    Ok((hashed, hex::encode(hash.finalize())))
}

pub(super) async fn remove_if_there(path: &Path) -> io::Result<()> {
    match tokio::fs::remove_file(path).await {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

fn timed_out() -> UpdateError {
    UpdateError::Network(format!("a chunk took over {} s", CHUNK_TIMEOUT.as_secs()))
}
