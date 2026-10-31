//! The soundboard (Phase 2 plan §11): playing a sound for everyone in this
//! device's voice channel, and previewing one here only. The core keeps
//! every server's sounds on disk ahead of time and plays the sounds others
//! play.

use opencord_common::sound::defaults::DEFAULT_SOUNDS;

use crate::MobileError;
use crate::convert;
use crate::ids::parse_id;
use crate::mobile::{mobile, on_runtime};
use crate::types::{DefaultSoundRecord, ExternalSoundRecord};

/// The sounds every client has, played as `default:<id>`.
#[uniffi::export]
pub fn soundboard_default_sounds() -> Vec<DefaultSoundRecord> {
    DEFAULT_SOUNDS
        .iter()
        .map(|sound| DefaultSoundRecord {
            id: sound.id.to_owned(),
            name: sound.name.to_owned(),
            emoji: sound.emoji.to_owned(),
            duration_ms: sound.duration_ms,
        })
        .collect()
}

/// Plays a sound in the voice channel this device is in, for everyone
/// there. `sound_ref` is `server:<sha256>`, `external:<sha256>` or
/// `default:<id>`; an `external` sound names the server it comes from.
#[uniffi::export]
pub async fn soundboard_play(
    server_key: String,
    channel_id: String,
    sound_ref: String,
    external: Option<ExternalSoundRecord>,
) -> Result<(), MobileError> {
    let channel_id = parse_id(&channel_id)?;
    let external = external
        .map(|external| {
            convert::core_soundboard_sound(external.sound).map(|sound| (external.server_key, sound))
        })
        .transpose()?;
    on_runtime(move |client| async move {
        Ok(client
            .soundboard_play(&server_key, channel_id, &sound_ref, external)
            .await?)
    })
    .await
}

/// Plays a sound on this device only; nobody else hears it. Another
/// server's sound previews from that server, by its `server:` ref.
#[uniffi::export]
pub async fn soundboard_preview(server_key: String, sound_ref: String) -> Result<(), MobileError> {
    on_runtime(move |client| async move {
        Ok(client.soundboard_preview(&server_key, &sound_ref).await?)
    })
    .await
}

/// The soundboard's volume (0–1) and "Mute all soundboards".
#[uniffi::export]
pub fn soundboard_set_listening(volume: f32, muted: bool) -> Result<(), MobileError> {
    mobile()?.client().soundboard_set_listening(volume, muted);
    Ok(())
}
