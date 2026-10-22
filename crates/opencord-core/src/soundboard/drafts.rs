//! New sounds made from a file (plan §11.3): decoded once for the trim
//! editor, then previewed and uploaded by the 5-second window chosen.

use std::sync::Arc;

use opencord_common::sound::{MAX_DURATION_MS, VOLUME_MAX};
use opencord_common::validation;
use opencord_media::soundboard::{self as audio, Pcm, input};
use opencord_proto::v1 as proto;

use super::{Soundboard, expect, f32_percent, invalid};
use crate::api::types::{CoreError, SoundDraft, SoundSelection, SoundboardSound};
use crate::client::Client;
use crate::convert;

/// Slices in a draft's waveform: enough for a wide editor.
const WAVEFORM_BUCKETS: usize = 600;

impl Soundboard {
    /// Decodes the file at `path` for the trim editor.
    pub async fn open_draft(&self, path: String) -> Result<SoundDraft, CoreError> {
        let pcm = self
            .runtime
            .spawn_blocking(move || {
                let bytes = std::fs::read(&path).map_err(|error| invalid(error.to_string()))?;
                input::decode_input(bytes).map_err(|error| invalid(error.to_string()))
            })
            .await
            .map_err(|error| invalid(error.to_string()))??;
        let waveform = audio::waveform(&pcm, WAVEFORM_BUCKETS);
        let duration_ms = pcm.duration_ms();
        let mut state = self.lock();
        state.next_draft += 1;
        let draft_id = state.next_draft;
        state.drafts.insert(draft_id, pcm);
        Ok(SoundDraft {
            draft_id,
            duration_ms,
            waveform,
        })
    }

    pub fn close_draft(&self, draft_id: u64) {
        self.lock().drafts.remove(&draft_id);
    }

    /// Plays the chosen window here, as it would be uploaded.
    pub async fn preview_draft(
        &self,
        client: &Client,
        selection: SoundSelection,
    ) -> Result<(), CoreError> {
        let window = self.window(&selection).await?;
        let gain = f32_percent(selection.volume) * self.lock().listening.volume;
        if let Some(media) = client.media() {
            media.preview(Arc::from(window.samples), gain);
        }
        Ok(())
    }

    /// Uploads the chosen window as a new sound on the server at `key`.
    pub async fn upload(
        &self,
        client: &Client,
        key: &str,
        selection: SoundSelection,
        name: String,
        emoji: Option<String>,
    ) -> Result<SoundboardSound, CoreError> {
        let name = validation::sound_name(&name).map_err(|error| invalid(error.to_string()))?;
        let emoji = validation::sound_emoji(emoji.as_deref().unwrap_or_default())
            .map_err(|error| invalid(error.to_string()))?;
        let window = self.window(&selection).await?;
        let file = self
            .runtime
            .spawn_blocking(move || audio::encode(&window))
            .await
            .map_err(|error| invalid(error.to_string()))?
            .map_err(|error| invalid(error.to_string()))?;
        let mut path = format!(
            "/media/sounds?name={}&volume={}",
            query_value(&name),
            selection.volume
        );
        if let Some(emoji) = &emoji {
            path.push_str(&format!("&emoji={}", query_value(emoji)));
        }
        let response = self.http(client, key, "POST", &path, &file).await?;
        let sound = expect(response, |result| match result {
            proto::response::Result::SoundboardSound(sound) => Some(sound),
            _ => None,
        })?;
        if let Ok(hash) = <[u8; 32]>::try_from(sound.sha256.as_slice()) {
            let _ = self.cache.put(&hash, &file);
        }
        self.close_draft(selection.draft_id);
        Ok(convert::soundboard_sound(sound))
    }

    /// The selected window, normalized when asked.
    async fn window(&self, selection: &SoundSelection) -> Result<Pcm, CoreError> {
        if selection.length_ms == 0 || selection.length_ms > MAX_DURATION_MS {
            return Err(invalid(format!(
                "a sound is at most {} seconds long",
                MAX_DURATION_MS / 1_000
            )));
        }
        if selection.volume > VOLUME_MAX {
            return Err(invalid(format!("a sound's volume is 0 to {VOLUME_MAX} %")));
        }
        let pcm = self
            .lock()
            .drafts
            .get(&selection.draft_id)
            .cloned()
            .ok_or_else(|| invalid("that file is no longer open".to_owned()))?;
        let (start, length, normalize) =
            (selection.start_ms, selection.length_ms, selection.normalize);
        self.runtime
            .spawn_blocking(move || {
                let window = pcm.trimmed(start, length);
                if window.frames() == 0 {
                    return Err(invalid("the window is past the end of the file".to_owned()));
                }
                if !normalize {
                    return Ok(window);
                }
                let gain =
                    audio::normalizing_gain(&window).map_err(|error| invalid(error.to_string()))?;
                Ok(window.scaled(gain))
            })
            .await
            .map_err(|error| invalid(error.to_string()))?
    }
}

/// Percent-encodes a query value (RFC 3986 unreserved characters stay).
fn query_value(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(byte).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_values_are_percent_encoded() {
        assert_eq!(query_value("Ta-da!"), "Ta-da%21");
        assert_eq!(query_value("a b&c=d"), "a%20b%26c%3Dd");
        assert_eq!(query_value("🔔"), "%F0%9F%94%94");
    }
}
