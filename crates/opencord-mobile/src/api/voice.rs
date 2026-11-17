//! Voice: joining, leaving, this device's toggles, and others' volume.

use crate::MobileError;
use crate::convert;
use crate::ids::parse_id;
use crate::mobile::{mobile, on_runtime};
use crate::types::{PhoneAudioSettings, VoiceCueKind, VoiceStateRecord};

/// Joins a voice channel; this device is in one at most, so any other is
/// left first. The app asks for the microphone before calling this.
#[uniffi::export]
pub async fn voice_join(
    server_key: String,
    channel_id: String,
) -> Result<VoiceStateRecord, MobileError> {
    let channel_id = parse_id(&channel_id)?;
    on_runtime(move |client| async move {
        let state = client.voice_join(&server_key, channel_id).await?;
        Ok(convert::voice_state(state))
    })
    .await
}

#[uniffi::export]
pub async fn voice_leave() -> Result<(), MobileError> {
    on_runtime(move |client| async move { Ok(client.voice_leave().await?) }).await
}

/// Holds outside voice too, for the next join.
#[uniffi::export]
pub fn voice_set_mute(muted: bool) -> Result<(), MobileError> {
    mobile()?.client().set_voice_self(Some(muted), None);
    Ok(())
}

/// Holds outside voice too, for the next join.
#[uniffi::export]
pub fn voice_set_deaf(deafened: bool) -> Result<(), MobileError> {
    mobile()?.client().set_voice_self(None, Some(deafened));
    Ok(())
}

/// How loud someone sounds on this device, 0–2 (200 %).
#[uniffi::export]
pub fn voice_set_user_volume(
    server_key: String,
    user_id: String,
    volume: f32,
) -> Result<(), MobileError> {
    let user_id = parse_id(&user_id)?;
    mobile()?
        .client()
        .set_user_volume(&server_key, user_id, volume);
    Ok(())
}

/// Silences someone on this device only.
#[uniffi::export]
pub fn voice_set_user_local_mute(
    server_key: String,
    user_id: String,
    muted: bool,
) -> Result<(), MobileError> {
    let user_id = parse_id(&user_id)?;
    mobile()?
        .client()
        .set_user_local_mute(&server_key, user_id, muted);
    Ok(())
}

/// The user's audio choices, for calls now and later and the microphone
/// test. Only these three change; the rest stays at the core's defaults.
#[uniffi::export]
pub fn audio_settings_apply(settings: PhoneAudioSettings) -> Result<(), MobileError> {
    mobile()?
        .client()
        .apply_audio_settings(convert::core_audio_settings(settings));
    Ok(())
}

/// Hear yourself as others would, Opus and all: through the call when in
/// one, else through an engine of its own that sends nowhere. Outside a
/// call it opens the microphone, so on iOS the audio session must be on
/// first. A microphone that can't open comes back as `AudioProblem`.
#[uniffi::export]
pub async fn mic_test_set(on: bool) -> Result<(), MobileError> {
    let mobile = mobile()?;
    // Opening the devices blocks; not on JavaScript's thread.
    mobile
        .blocking(move || {
            mobile.client().set_mic_test(on);
            Ok(())
        })
        .await
}

/// Sends `InputLevel` while on, at the core's own rate.
#[uniffi::export]
pub fn level_meter_set(on: bool) -> Result<(), MobileError> {
    mobile()?.client().set_level_meter(on);
    Ok(())
}

/// Plays a voice event's sound on this device only, at `volume` (0–1).
#[uniffi::export]
pub fn cue_play(cue: VoiceCueKind, volume: f32) -> Result<(), MobileError> {
    mobile()?.client().play_cue(convert::core_cue(cue), volume);
    Ok(())
}
