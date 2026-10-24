//! Uploaded songs (design 2026-10-24): kept once per server by their
//! SHA-256, owned by whoever uploaded or dropped them, found in search and
//! played by this server itself ([`player`]), never by Lavalink.

use std::path::{Path, PathBuf};

use opencord_common::permissions::Permissions;
use opencord_proto::v1 as proto;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use super::tracks::{Track, TrackInfo};
use crate::attachments::http::{batch_end, read_plain};
use crate::attachments::{open_key, temp};
use crate::db;
use crate::db::local_songs::SongRow;
use crate::error::ApiError;
use crate::state::AppState;

pub mod decode;
pub mod player;

/// The source of uploaded songs' tracks, shown as "Local".
pub const SOURCE: &str = "upload";
/// Uploaded songs a search shows, after Lavalink's results.
pub const SEARCH_MAX: usize = 10;
/// Candidates the database hands over before every word is checked.
const CANDIDATES: i64 = 500;
/// Songs per page of the settings list.
pub const PAGE: i64 = 50;
/// The files the music player takes, by extension.
pub const EXTENSIONS: [&str; 7] = ["mp3", "flac", "m4a", "aac", "ogg", "oga", "wav"];

/// A song as the queue keeps it: a track whose identifier is the song id.
pub fn track(song: &SongRow) -> Track {
    Track {
        encoded: String::new(),
        info: TrackInfo {
            identifier: song.id.to_string(),
            title: song.title.clone(),
            author: song.artist.clone(),
            length_ms: u64::try_from(song.duration_ms).unwrap_or(0),
            is_stream: false,
            is_seekable: true,
            uri: String::new(),
            artwork_url: String::new(),
            source_name: SOURCE.to_owned(),
        },
    }
}

/// The song behind a track, when it is an uploaded one.
pub fn song_id(track: &Track) -> Option<i64> {
    (track.info.source_name == SOURCE)
        .then(|| track.info.identifier.parse().ok())
        .flatten()
}

/// Whether the music player takes a file called `filename`.
pub fn is_song_file(filename: &str) -> bool {
    filename
        .rsplit_once('.')
        .is_some_and(|(_, extension)| EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str()))
}

/// A `LIKE` pattern for text containing `word`, lowercase.
fn contains(word: &str) -> String {
    let mut pattern = String::from("%");
    for c in word.to_lowercase().chars() {
        if matches!(c, '%' | '_' | '\\') {
            pattern.push('\\');
        }
        pattern.push(c);
    }
    pattern.push('%');
    pattern
}

fn words(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(str::to_lowercase)
        .filter(|word| !word.is_empty())
        .collect()
}

fn matches_all(song: &SongRow, words: &[String]) -> bool {
    let text = format!("{} {} {}", song.title, song.artist, song.filename).to_lowercase();
    words.iter().all(|word| text.contains(word.as_str()))
}

/// Songs matching `query` that a member with `permissions` may find:
/// every song with MUSIC_ALL_LOCAL_SONGS, else their own. Each comes with
/// whether they may remove it.
pub async fn search(
    state: &AppState,
    user_id: i64,
    permissions: Permissions,
    query: &str,
) -> Result<Vec<(Track, bool)>, ApiError> {
    let words = words(query);
    // The longest word narrows best in the database; the rest are checked
    // here.
    let Some(longest) = words.iter().max_by_key(|word| word.chars().count()) else {
        return Ok(Vec::new());
    };
    let everything = permissions.contains(Permissions::MUSIC_ALL_LOCAL_SONGS);
    let mut conn = state.db.acquire().await?;
    let candidates = db::local_songs::matching(
        &mut conn,
        &contains(longest),
        (!everything).then_some(user_id),
        CANDIDATES,
    )
    .await?;
    let found: Vec<SongRow> = candidates
        .into_iter()
        .filter(|song| matches_all(song, &words))
        .take(SEARCH_MAX)
        .collect();
    let admin = permissions.contains(Permissions::ADMINISTRATOR);
    let owned = if admin || !everything {
        // Without seeing everything, all they find is their own.
        found.iter().map(|song| song.id).collect()
    } else {
        let ids: Vec<i64> = found.iter().map(|song| song.id).collect();
        db::local_songs::owned_of(&mut conn, user_id, &ids).await?
    };
    Ok(found
        .iter()
        .map(|song| (track(song), owned.contains(&song.id)))
        .collect())
}

/// The settings list: a page of every song, newest first.
pub async fn page(
    state: &AppState,
    query: &str,
    before_id: Option<i64>,
) -> Result<proto::LocalSongs, ApiError> {
    let words = words(query);
    let pattern = words
        .iter()
        .max_by_key(|word| word.chars().count())
        .map_or_else(|| "%".to_owned(), |word| contains(word));
    let mut conn = state.db.acquire().await?;
    let mut songs = Vec::new();
    let mut before = before_id.unwrap_or(i64::MAX);
    // Rows that miss a word are skipped; more pages fill the page up.
    loop {
        let rows = db::local_songs::page(&mut conn, &pattern, before, PAGE).await?;
        let done = rows.len() < PAGE as usize;
        if let Some(last) = rows.last() {
            before = last.id;
        }
        songs.extend(
            rows.iter()
                .filter(|song| matches_all(song, &words))
                .map(to_proto),
        );
        if done || songs.len() >= PAGE as usize {
            break;
        }
    }
    songs.truncate(PAGE as usize);
    Ok(proto::LocalSongs { songs })
}

pub fn to_proto(song: &SongRow) -> proto::LocalSong {
    proto::LocalSong {
        id: song.id,
        title: song.title.clone(),
        artist: song.artist.clone(),
        duration_ms: u64::try_from(song.duration_ms).unwrap_or(0),
        size_bytes: u64::try_from(song.size_bytes).unwrap_or(0),
        filename: song.filename.clone(),
        uploader_id: song.uploader_id,
        created_at_ms: song.created_at,
    }
}

/// Deletes a song for everyone: its row, its owners and its file.
pub async fn delete(state: &AppState, song_id: i64) -> Result<(), ApiError> {
    let mut conn = state.db.acquire().await?;
    // The song and its owners go with the file's row, which queues the
    // file's deletion.
    db::attachments::delete(&mut conn, song_id).await?;
    Ok(())
}

/// A plain copy of a song's file in the temp directory, deleted when this
/// is dropped.
#[derive(Debug)]
pub struct PlainFile {
    path: PathBuf,
}

impl PlainFile {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for PlainFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn plain_path(state: &AppState, name: &str) -> PathBuf {
    state
        .attachments
        .temp_dir()
        .join(format!("music-{name}-{}.plain", crate::random::code(12)))
}

/// Opens the upload `id` of `size` bytes, still in its temp file at
/// `sealed`, into a plain copy; returns it with its SHA-256.
pub async fn open_upload(
    state: &AppState,
    key: &opencord_common::blob::BlobKey,
    id: i64,
    size: u64,
    sealed: &Path,
) -> std::io::Result<(PlainFile, [u8; 32])> {
    let plain = PlainFile {
        path: plain_path(state, &id.to_string()),
    };
    let mut file = tokio::fs::File::create(plain.path()).await?;
    let mut hash = Sha256::new();
    let step = opencord_common::blob::SEGMENT_PLAIN * 16;
    let mut offset = 0;
    while offset < size {
        let len = step.min(size - offset);
        let bytes = temp::read_range(sealed, key, id, size, offset, len).await?;
        hash.update(&bytes);
        file.write_all(&bytes).await?;
        offset += len;
    }
    file.flush().await?;
    Ok((plain, hash.finalize().into()))
}

/// A plain copy of a stored song, to play from; `None` when the song or
/// its file is gone.
pub async fn stored_copy(state: &AppState, song_id: i64) -> Result<Option<PlainFile>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let row = db::attachments::find(&mut conn, song_id).await?;
    drop(conn);
    let Some(row) = row.filter(|row| row.state == "music") else {
        return Ok(None);
    };
    let key = open_key(state.certificates.secrets(), &row)?;
    let Some(backend) = state.attachments.backend(row.storage_id) else {
        return Ok(None);
    };
    let len = u64::try_from(row.size_bytes).unwrap_or(0);
    let plain = PlainFile {
        path: plain_path(state, &song_id.to_string()),
    };
    let failed = |error: String| {
        tracing::warn!(song_id, %error, "an uploaded song could not be read");
        ApiError::internal()
    };
    let mut file = tokio::fs::File::create(plain.path())
        .await
        .map_err(|error| failed(error.to_string()))?;
    let mut start = 0;
    while start < len {
        let end = batch_end(start, len - 1);
        let bytes = read_plain(&backend, &key, row.id, len, start, end)
            .await
            .map_err(failed)?;
        file.write_all(&bytes)
            .await
            .map_err(|error| failed(error.to_string()))?;
        start = end + 1;
    }
    file.flush()
        .await
        .map_err(|error| failed(error.to_string()))?;
    Ok(Some(plain))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(title: &str, artist: &str, filename: &str) -> SongRow {
        SongRow {
            id: 7,
            sha256: vec![0; 32],
            size_bytes: 10,
            filename: filename.to_owned(),
            title: title.to_owned(),
            artist: artist.to_owned(),
            duration_ms: 1_000,
            uploader_id: 1,
            created_at: 0,
        }
    }

    #[test]
    fn a_song_is_a_track_from_the_upload_source() {
        let track = track(&song("Lullaby", "Ana", "lullaby.mp3"));
        assert_eq!(song_id(&track), Some(7));
        assert_eq!(track.info.source_name, "upload");
        assert!(track.info.is_seekable);
        assert_eq!(track.to_proto().source_display_name, "Local");
    }

    #[test]
    fn every_word_must_be_in_the_title_artist_or_filename() {
        let song = song("Lullaby", "Ana", "night-version.mp3");
        assert!(matches_all(&song, &words("ana NIGHT")));
        assert!(!matches_all(&song, &words("ana day")));
    }

    #[test]
    fn like_patterns_escape_wildcards() {
        assert_eq!(contains("50%_off"), r"%50\%\_off%");
    }

    #[test]
    fn songs_are_known_by_their_extension() {
        assert!(is_song_file("Song.MP3"));
        assert!(is_song_file("a.b.flac"));
        assert!(!is_song_file("notes.txt"));
        assert!(!is_song_file("mp3"));
    }
}
