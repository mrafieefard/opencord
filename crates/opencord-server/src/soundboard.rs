//! The soundboard (Phase 2 plan §11): this server's sounds and the files
//! they are kept in, what an upload must be, and when a sound may play.
//! Sounds play on the clients; the server only checks and announces.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use opencord_common::sound::ogg::OpusFile;
use opencord_common::sound::{
    self, DURATION_TOLERANCE_MS, MAX_DURATION_MS, MAX_FILE_BYTES, MAX_PLAYING_PER_CHANNEL,
};
use opencord_proto::v1 as proto;
use sha2::{Digest, Sha256};

use crate::db::sounds::SoundRow;
use crate::error::ApiError;

/// Where sound files are kept, inside the data directory.
pub const SOUNDS_DIR: &str = "sounds";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sound {
    pub id: i64,
    pub name: String,
    pub emoji: Option<String>,
    /// Percent.
    pub volume: u32,
    pub sha256: [u8; 32],
    pub size_bytes: u32,
    pub duration_ms: u32,
    pub channels: u8,
    pub uploader_id: Option<i64>,
    pub created_at: i64,
}

impl Sound {
    pub fn from_row(row: SoundRow) -> Option<Self> {
        Some(Self {
            id: row.id,
            name: row.name,
            emoji: row.emoji,
            volume: u32::try_from(row.volume).ok()?,
            sha256: row.sha256.try_into().ok()?,
            size_bytes: u32::try_from(row.size_bytes).ok()?,
            duration_ms: u32::try_from(row.duration_ms).ok()?,
            channels: u8::try_from(row.channel_count).ok()?,
            uploader_id: row.uploader_id,
            created_at: row.created_at,
        })
    }

    pub fn to_row(&self) -> SoundRow {
        SoundRow {
            id: self.id,
            name: self.name.clone(),
            emoji: self.emoji.clone(),
            volume: i64::from(self.volume),
            sha256: self.sha256.to_vec(),
            size_bytes: i64::from(self.size_bytes),
            duration_ms: i64::from(self.duration_ms),
            channel_count: i64::from(self.channels),
            uploader_id: self.uploader_id,
            created_at: self.created_at,
        }
    }

    pub fn to_proto(&self) -> proto::SoundboardSound {
        proto::SoundboardSound {
            id: self.id,
            name: self.name.clone(),
            emoji: self.emoji.clone(),
            volume: self.volume,
            sha256: self.sha256.to_vec(),
            size_bytes: self.size_bytes,
            duration_ms: self.duration_ms,
            uploader_id: self.uploader_id,
            created_at_ms: self.created_at,
        }
    }

    pub fn sound_ref(&self) -> String {
        sound::SoundRef::Server(self.sha256).to_string()
    }
}

/// An uploaded file that passed every check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    pub sha256: [u8; 32],
    pub size_bytes: u32,
    pub duration_ms: u32,
    pub channels: u8,
}

/// Parses the file itself, whatever the client said it was (plan §11.3):
/// Ogg Opus, 48 kHz, at most two channels, 256 KB and 5 seconds.
pub fn check_file(bytes: &[u8]) -> Result<Checked, ApiError> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(ApiError::sound_invalid(format!(
            "the file is larger than {} KB",
            MAX_FILE_BYTES / 1024
        )));
    }
    let file = OpusFile::read(bytes).map_err(|error| ApiError::sound_invalid(error.to_string()))?;
    if !sound::within_duration(&file.info) {
        return Err(ApiError::sound_too_long(
            file.info.duration_ms.max(file.info.decoded_ms),
        ));
    }
    Ok(Checked {
        sha256: Sha256::digest(bytes).into(),
        size_bytes: u32::try_from(bytes.len()).unwrap_or(u32::MAX),
        duration_ms: file
            .info
            .duration_ms
            .min(MAX_DURATION_MS + DURATION_TOLERANCE_MS),
        channels: file.info.channels,
    })
}

pub fn file_path(data_dir: &Path, sha256: &[u8; 32]) -> PathBuf {
    data_dir
        .join(SOUNDS_DIR)
        .join(format!("{}.opus", hex::encode(sha256)))
}

/// Writes a sound's file unless it is already there. Files are named by
/// their hash, so one that exists has these bytes.
pub async fn store(data_dir: &Path, sha256: &[u8; 32], bytes: &[u8]) -> io::Result<()> {
    let path = file_path(data_dir, sha256);
    if tokio::fs::try_exists(&path).await? {
        return Ok(());
    }
    tokio::fs::create_dir_all(data_dir.join(SOUNDS_DIR)).await?;
    let partial = path.with_extension("opus.partial");
    tokio::fs::write(&partial, bytes).await?;
    tokio::fs::rename(&partial, &path).await
}

pub async fn remove_file(data_dir: &Path, sha256: &[u8; 32]) {
    let path = file_path(data_dir, sha256);
    if let Err(error) = tokio::fs::remove_file(&path).await
        && error.kind() != io::ErrorKind::NotFound
    {
        tracing::warn!(%error, path = %path.display(), "a sound file could not be removed");
    }
}

/// Who played last and what is playing: the per-user cooldown and the
/// limit on sounds at once in a channel (plan §11.2).
#[derive(Debug, Default)]
pub struct Plays {
    last_by_user: HashMap<i64, Instant>,
    /// When each sound playing in a channel ends.
    ends_by_channel: HashMap<i64, Vec<Instant>>,
}

impl Plays {
    /// Counts a sound as started, or says why it cannot be.
    pub fn start(
        &mut self,
        now: Instant,
        user_id: i64,
        channel_id: i64,
        cooldown: Duration,
        length: Duration,
    ) -> Result<(), ApiError> {
        if let Some(last) = self.last_by_user.get(&user_id) {
            let ready = *last + cooldown;
            if now < ready {
                return Err(ApiError::sound_cooldown(
                    "Wait for the cooldown to play another sound",
                    Some(ready - now),
                ));
            }
        }
        let playing = self.ends_by_channel.entry(channel_id).or_default();
        playing.retain(|end| *end > now);
        if playing.len() >= MAX_PLAYING_PER_CHANNEL {
            let first_end = playing.iter().min().copied().unwrap_or(now);
            return Err(ApiError::sound_cooldown(
                format!("{MAX_PLAYING_PER_CHANNEL} sounds are already playing here"),
                Some(first_end - now),
            ));
        }
        playing.push(now + length);
        self.last_by_user.insert(user_id, now);
        Ok(())
    }

    /// Forgets cooldowns that ran out; called now and then so the map does
    /// not grow with everyone who ever played a sound.
    pub fn prune(&mut self, now: Instant, longest_cooldown: Duration) {
        self.last_by_user
            .retain(|_, last| now.duration_since(*last) < longest_cooldown);
        self.ends_by_channel.retain(|_, ends| {
            ends.retain(|end| *end > now);
            !ends.is_empty()
        });
    }
}

#[cfg(test)]
mod tests {
    use opencord_common::sound::ogg;

    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    fn file(seconds: f64) -> Vec<u8> {
        let samples = (seconds * 48_000.0) as u64;
        let packets: Vec<Vec<u8>> = (0..samples.div_ceil(960))
            .map(|_| vec![31 << 3, 0])
            .collect();
        ogg::write(1, 0, &packets, samples)
    }

    #[test]
    fn a_six_second_file_is_too_long() {
        let error = check_file(&file(6.0)).unwrap_err();

        assert_eq!(error.code, proto::ErrorCode::SoundTooLong);
        assert_eq!(check_file(&file(5.0)).unwrap().duration_ms, 5_000);
    }

    #[test]
    fn what_is_not_ogg_opus_is_invalid() {
        for bytes in [b"RIFF....WAVEfmt ".to_vec(), file(1.0)[..100].to_vec()] {
            assert_eq!(
                check_file(&bytes).unwrap_err().code,
                proto::ErrorCode::SoundInvalid
            );
        }
        let huge = vec![0; MAX_FILE_BYTES + 1];
        assert_eq!(
            check_file(&huge).unwrap_err().code,
            proto::ErrorCode::SoundInvalid
        );
    }

    #[test]
    fn a_user_waits_out_the_cooldown() {
        let mut plays = Plays::default();
        let now = Instant::now();
        plays.start(now, 1, 10, 3 * SECOND, SECOND).unwrap();

        let error = plays
            .start(now + 2 * SECOND, 1, 10, 3 * SECOND, SECOND)
            .unwrap_err();
        assert_eq!(error.code, proto::ErrorCode::SoundCooldown);
        assert_eq!(error.retry_after_ms, Some(1_000));
        assert!(
            plays
                .start(now + 3 * SECOND, 1, 10, 3 * SECOND, SECOND)
                .is_ok()
        );
    }

    #[test]
    fn three_sounds_at_once_fill_a_channel() {
        let mut plays = Plays::default();
        let now = Instant::now();
        for user in 1..=3 {
            plays.start(now, user, 10, SECOND, 4 * SECOND).unwrap();
        }

        let full = plays.start(now, 4, 10, SECOND, 4 * SECOND).unwrap_err();
        assert_eq!(full.code, proto::ErrorCode::SoundCooldown);
        // Another channel is not affected, and the first channel has room
        // once a sound ends.
        assert!(plays.start(now, 4, 11, SECOND, 4 * SECOND).is_ok());
        assert!(plays.start(now + 4 * SECOND, 5, 10, SECOND, SECOND).is_ok());
    }
}
