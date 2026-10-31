//! Voice messages (media uploads design §10): recording one from the
//! microphone (or, in a call, from the call's own processed microphone),
//! sending it, and playing voice messages and drafts with seeking and speed.

use crate::MobileError;
use crate::convert;
use crate::ids::parse_id;
use crate::mobile::{mobile, on_runtime};
use crate::types::{MessageRecord, PlaybackItemRecord, VoiceDraftRecord};

/// Starts recording; it stops by itself after `max_seconds` with a
/// `VoiceRecordStopped` event. `VoiceRecordLevel` events follow meanwhile.
#[uniffi::export]
pub fn voice_record_start(max_seconds: u32) -> Result<(), MobileError> {
    Ok(mobile()?.client().voice_record_start(max_seconds)?)
}

/// Stops recording and keeps what was recorded as a draft.
#[uniffi::export]
pub fn voice_record_stop() -> Result<VoiceDraftRecord, MobileError> {
    Ok(convert::voice_draft(
        mobile()?.client().voice_record_stop()?,
    ))
}

/// Stops recording and keeps nothing.
#[uniffi::export]
pub fn voice_record_cancel() -> Result<(), MobileError> {
    mobile()?.client().voice_record_cancel();
    Ok(())
}

#[uniffi::export]
pub fn voice_draft_discard(draft_id: f64) -> Result<(), MobileError> {
    mobile()?
        .client()
        .voice_draft_discard(convert::draft_id(draft_id)?);
    Ok(())
}

/// Uploads a draft and sends it as a voice message: one attachment, no text.
#[uniffi::export]
pub async fn voice_message_send(
    server_key: String,
    channel_id: String,
    nonce: String,
    draft_id: f64,
) -> Result<MessageRecord, MobileError> {
    let channel_id = parse_id(&channel_id)?;
    let draft_id = convert::draft_id(draft_id)?;
    on_runtime(move |client| async move {
        let message = client
            .send_voice_message(&server_key, channel_id, nonce, draft_id)
            .await?;
        Ok(convert::message(message))
    })
    .await
}

/// Plays a voice message or a draft from `position_ms` at `speed` (1, 1.5
/// or 2); `Playback` events follow. Whatever played before stops.
#[uniffi::export]
pub async fn voice_play(
    item: PlaybackItemRecord,
    position_ms: u32,
    speed: f32,
) -> Result<(), MobileError> {
    let item = convert::core_playback_item(item)?;
    on_runtime(move |client| async move { Ok(client.voice_play(item, position_ms, speed).await?) })
        .await
}

#[uniffi::export]
pub fn voice_pause() -> Result<(), MobileError> {
    mobile()?.client().voice_pause();
    Ok(())
}

#[uniffi::export]
pub fn voice_resume() -> Result<(), MobileError> {
    mobile()?.client().voice_resume();
    Ok(())
}

#[uniffi::export]
pub fn voice_seek(position_ms: u32) -> Result<(), MobileError> {
    mobile()?.client().voice_seek(position_ms);
    Ok(())
}

#[uniffi::export]
pub fn voice_set_speed(speed: f32) -> Result<(), MobileError> {
    Ok(mobile()?.client().voice_set_speed(speed)?)
}

#[uniffi::export]
pub fn voice_stop() -> Result<(), MobileError> {
    mobile()?.client().voice_stop();
    Ok(())
}
