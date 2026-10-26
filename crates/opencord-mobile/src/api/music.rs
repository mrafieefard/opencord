//! A voice channel's music player, uploaded songs, and this person's music
//! volume.

use crate::MobileError;
use crate::convert;
use crate::ids::parse_id;
use crate::mobile::{mobile, on_runtime};
use crate::types::{
    MusicAction, MusicPlacement, MusicRepeat, MusicSearchResultsRecord, MusicSessionRecord,
    MusicSettingsRecord,
};

#[uniffi::export]
pub async fn music_settings(server_key: String) -> Result<MusicSettingsRecord, MobileError> {
    on_runtime(move |client| async move {
        Ok(convert::music_settings(
            client.music_settings(&server_key).await?,
        ))
    })
    .await
}

/// The channel's player; its changes then arrive as `MusicSession` events
/// even when not connected to it.
#[uniffi::export]
pub async fn music_subscribe(
    server_key: String,
    channel_id: String,
) -> Result<MusicSessionRecord, MobileError> {
    let channel_id = parse_id(&channel_id)?;
    on_runtime(move |client| async move {
        let session = client
            .music_subscribe(&server_key, channel_id, true)
            .await?;
        Ok(convert::session(session))
    })
    .await
}

/// Stops what `music_subscribe` started.
#[uniffi::export]
pub async fn music_unsubscribe(server_key: String, channel_id: String) -> Result<(), MobileError> {
    let channel_id = parse_id(&channel_id)?;
    on_runtime(move |client| async move {
        client
            .music_subscribe(&server_key, channel_id, false)
            .await?;
        Ok(())
    })
    .await
}

/// Searches, or loads a link; `source` empty for the server's default.
#[uniffi::export]
pub async fn music_search(
    server_key: String,
    channel_id: String,
    query: String,
    source: String,
) -> Result<MusicSearchResultsRecord, MobileError> {
    let channel_id = parse_id(&channel_id)?;
    on_runtime(move |client| async move {
        let results = client
            .music_search(&server_key, channel_id, query, source)
            .await?;
        Ok(convert::search_results(results))
    })
    .await
}

/// Adds search results, or tracks from the history ("Add again").
#[uniffi::export]
pub async fn music_enqueue(
    server_key: String,
    channel_id: String,
    result_ids: Vec<String>,
    history_item_ids: Vec<String>,
    before_item_id: Option<String>,
    placement: MusicPlacement,
) -> Result<MusicSessionRecord, MobileError> {
    let channel_id = parse_id(&channel_id)?;
    on_runtime(move |client| async move {
        let session = client
            .music_enqueue(
                &server_key,
                channel_id,
                result_ids,
                history_item_ids,
                before_item_id,
                convert::placement(placement),
            )
            .await?;
        Ok(convert::session(session))
    })
    .await
}

/// Pause, resume, next, previous, seek (to `position_ms`) or stop. With
/// `expected_item_id`, nothing happens when another item plays by then.
#[uniffi::export]
pub async fn music_playback(
    server_key: String,
    channel_id: String,
    action: MusicAction,
    position_ms: Option<f64>,
    expected_item_id: Option<String>,
) -> Result<MusicSessionRecord, MobileError> {
    let channel_id = parse_id(&channel_id)?;
    let position_ms = position_ms.map(milliseconds).transpose()?;
    on_runtime(move |client| async move {
        let session = client
            .music_playback(
                &server_key,
                channel_id,
                convert::action(action),
                position_ms,
                expected_item_id,
            )
            .await?;
        Ok(convert::session(session))
    })
    .await
}

/// Repeat and shuffle, shared by everyone in the channel; `None` stays.
#[uniffi::export]
pub async fn music_set_mode(
    server_key: String,
    channel_id: String,
    repeat: Option<MusicRepeat>,
    shuffle: Option<bool>,
) -> Result<MusicSessionRecord, MobileError> {
    let channel_id = parse_id(&channel_id)?;
    on_runtime(move |client| async move {
        let session = client
            .music_set_mode(
                &server_key,
                channel_id,
                repeat.map(convert::core_repeat),
                shuffle,
            )
            .await?;
        Ok(convert::session(session))
    })
    .await
}

/// Puts an item before another, or at the end.
#[uniffi::export]
pub async fn music_queue_move(
    server_key: String,
    channel_id: String,
    item_id: String,
    before_item_id: Option<String>,
) -> Result<MusicSessionRecord, MobileError> {
    let channel_id = parse_id(&channel_id)?;
    on_runtime(move |client| async move {
        let session = client
            .music_queue_move(&server_key, channel_id, item_id, before_item_id)
            .await?;
        Ok(convert::session(session))
    })
    .await
}

#[uniffi::export]
pub async fn music_queue_remove(
    server_key: String,
    channel_id: String,
    item_ids: Vec<String>,
) -> Result<MusicSessionRecord, MobileError> {
    let channel_id = parse_id(&channel_id)?;
    on_runtime(move |client| async move {
        let session = client
            .music_queue_remove(&server_key, channel_id, item_ids)
            .await?;
        Ok(convert::session(session))
    })
    .await
}

#[uniffi::export]
pub async fn music_queue_clear(
    server_key: String,
    channel_id: String,
) -> Result<MusicSessionRecord, MobileError> {
    let channel_id = parse_id(&channel_id)?;
    on_runtime(move |client| async move {
        let session = client.music_queue_clear(&server_key, channel_id).await?;
        Ok(convert::session(session))
    })
    .await
}

#[uniffi::export]
pub async fn music_vote_skip(
    server_key: String,
    channel_id: String,
    expected_item_id: String,
) -> Result<MusicSessionRecord, MobileError> {
    let channel_id = parse_id(&channel_id)?;
    on_runtime(move |client| async move {
        let session = client
            .music_vote_skip(&server_key, channel_id, expected_item_id)
            .await?;
        Ok(convert::session(session))
    })
    .await
}

/// This person's music volume (0–2), mute and "Lower music while people
/// talk", for every server; kept on this device and never sent.
#[uniffi::export]
pub fn music_set_listening(volume: f32, muted: bool, duck: bool) -> Result<(), MobileError> {
    mobile()?.client().music_set_listening(volume, muted, duck);
    Ok(())
}

/// A track's artwork at 128 or 512 px: the path of a file on this device.
#[uniffi::export]
pub async fn music_artwork(
    server_key: String,
    artwork_id: String,
    size: u32,
) -> Result<String, MobileError> {
    on_runtime(move |client| async move {
        Ok(client.music_artwork(&server_key, &artwork_id, size).await?)
    })
    .await
}

/// Queues the song at `path`, uploading it first unless the server has it;
/// `MusicUpload` events follow under `upload_id`. Resolves once queued.
#[uniffi::export]
pub async fn music_upload_song(
    server_key: String,
    channel_id: String,
    upload_id: String,
    path: String,
    play_now: bool,
) -> Result<(), MobileError> {
    let channel_id = parse_id(&channel_id)?;
    on_runtime(move |client| async move {
        Ok(client
            .music_upload_song(&server_key, channel_id, upload_id, path, play_now)
            .await?)
    })
    .await
}

/// Stops a song's upload; `music_upload_song` then fails with `Cancelled`.
#[uniffi::export]
pub fn music_cancel_song(server_key: String, upload_id: String) -> Result<(), MobileError> {
    mobile()?
        .client()
        .music_cancel_song(&server_key, &upload_id);
    Ok(())
}

/// A position from JavaScript, in whole milliseconds.
fn milliseconds(value: f64) -> Result<u64, MobileError> {
    if !value.is_finite() || value < 0.0 {
        return Err(MobileError::InvalidInput {
            message: format!("not a position: {value}"),
        });
    }
    Ok(value as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_must_be_finite_and_not_negative() {
        assert_eq!(milliseconds(1500.7).unwrap(), 1500);
        assert_eq!(milliseconds(0.0).unwrap(), 0);
        for wrong in [-1.0, f64::NAN, f64::INFINITY] {
            assert!(matches!(
                milliseconds(wrong),
                Err(MobileError::InvalidInput { .. })
            ));
        }
    }
}
