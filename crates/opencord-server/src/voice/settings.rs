//! Server-wide voice, video and soundboard settings (Phase 2 plan §5.2,
//! §11.7), kept in `server_meta` under `voice.*` keys.

use std::collections::HashMap;

use opencord_common::limits::{
    AFK_TIMEOUT_CHOICES_S, AFK_TIMEOUT_DEFAULT_S, CAMERA_PARTICIPANTS_DEFAULT,
    CAMERA_PARTICIPANTS_MAX, MAX_SOUNDS_DEFAULT, MAX_SOUNDS_LIMIT, SCREEN_SHARE_FPS_CHOICES,
    SCREEN_SHARE_FPS_DEFAULT, SERVER_BITRATE_DEFAULT, SERVER_BITRATE_MAX, SERVER_BITRATE_MIN,
    SOUND_COOLDOWN_DEFAULT_S, SOUND_COOLDOWN_MAX_S, STREAM_VIEWERS_DEFAULT, STREAM_VIEWERS_MAX,
};
use opencord_proto::v1 as proto;
use sqlx::SqliteConnection;

use crate::db::meta;
use crate::error::ApiError;

/// A screen share preset: a maximum pixel count. Ordered from the smallest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Resolution {
    P480,
    P720,
    P1080,
    P1440,
    Source,
}

impl Resolution {
    const ALL: [Self; 5] = [
        Self::P480,
        Self::P720,
        Self::P1080,
        Self::P1440,
        Self::Source,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::P480 => "480p",
            Self::P720 => "720p",
            Self::P1080 => "1080p",
            Self::P1440 => "1440p",
            Self::Source => "source",
        }
    }

    pub fn to_proto(self) -> proto::ScreenShareResolution {
        match self {
            Self::P480 => proto::ScreenShareResolution::ScreenShareResolution480p,
            Self::P720 => proto::ScreenShareResolution::ScreenShareResolution720p,
            Self::P1080 => proto::ScreenShareResolution::ScreenShareResolution1080p,
            Self::P1440 => proto::ScreenShareResolution::ScreenShareResolution1440p,
            Self::Source => proto::ScreenShareResolution::Source,
        }
    }

    pub fn from_proto(value: i32) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|resolution| resolution.to_proto() as i32 == value)
    }
}

const PREFIX: &str = "voice.";
const SCREEN_SHARE_MAX_RESOLUTION: &str = "screen_share_max_resolution";
const SCREEN_SHARE_MAX_FPS: &str = "screen_share_max_fps";
const MAX_STREAM_VIEWERS: &str = "max_stream_viewers";
const CAMERA_ALLOWED: &str = "camera_allowed";
const MAX_CAMERA_PARTICIPANTS: &str = "max_camera_participants";
const MAX_VOICE_BITRATE: &str = "max_voice_bitrate";
const AFK_CHANNEL_ID: &str = "afk_channel_id";
const AFK_TIMEOUT_S: &str = "afk_timeout_s";
const SOUNDBOARD_ENABLED: &str = "soundboard_enabled";
const ALLOW_DEFAULT_SOUNDS: &str = "allow_default_sounds";
const ALLOW_EXTERNAL_SOUNDS: &str = "allow_external_sounds";
const SOUND_COOLDOWN_S: &str = "sound_cooldown_s";
const MAX_SOUNDS: &str = "max_sounds";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceSettings {
    pub screen_share_max_resolution: Resolution,
    pub screen_share_max_fps: u32,
    pub max_stream_viewers: u32,
    pub camera_allowed: bool,
    pub max_camera_participants: u32,
    /// Bits per second.
    pub max_voice_bitrate: u32,
    pub afk_channel_id: Option<i64>,
    pub afk_timeout_s: u32,
    pub soundboard_enabled: bool,
    pub allow_default_sounds: bool,
    pub allow_external_sounds: bool,
    pub sound_cooldown_s: u32,
    pub max_sounds: u32,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            screen_share_max_resolution: Resolution::P720,
            screen_share_max_fps: SCREEN_SHARE_FPS_DEFAULT,
            max_stream_viewers: STREAM_VIEWERS_DEFAULT,
            camera_allowed: true,
            max_camera_participants: CAMERA_PARTICIPANTS_DEFAULT,
            max_voice_bitrate: SERVER_BITRATE_DEFAULT,
            afk_channel_id: None,
            afk_timeout_s: AFK_TIMEOUT_DEFAULT_S,
            soundboard_enabled: true,
            allow_default_sounds: true,
            allow_external_sounds: true,
            sound_cooldown_s: SOUND_COOLDOWN_DEFAULT_S,
            max_sounds: MAX_SOUNDS_DEFAULT,
        }
    }
}

impl VoiceSettings {
    /// These settings with `update` applied, if every given value is in
    /// range. `is_voice_channel` checks a proposed AFK channel.
    pub fn apply(
        &self,
        update: &proto::UpdateVoiceSettings,
        is_voice_channel: impl Fn(i64) -> bool,
    ) -> Result<Self, ApiError> {
        let resolution = update
            .screen_share_max_resolution
            .map(|value| {
                Resolution::from_proto(value)
                    .ok_or_else(|| ApiError::invalid_argument("unknown screen share resolution"))
            })
            .transpose()?;
        let afk_channel_id = match update.afk_channel_id {
            None => self.afk_channel_id,
            Some(0) => None,
            Some(id) if is_voice_channel(id) => Some(id),
            Some(_) => {
                return Err(ApiError::invalid_argument(
                    "the AFK channel must be a voice channel",
                ));
            }
        };
        Ok(Self {
            screen_share_max_resolution: resolution.unwrap_or(self.screen_share_max_resolution),
            screen_share_max_fps: one_of(
                "the screen share frame rate",
                update.screen_share_max_fps,
                &SCREEN_SHARE_FPS_CHOICES,
            )?
            .unwrap_or(self.screen_share_max_fps),
            max_stream_viewers: within(
                "viewers per stream",
                update.max_stream_viewers,
                1,
                STREAM_VIEWERS_MAX,
            )?
            .unwrap_or(self.max_stream_viewers),
            camera_allowed: update.camera_allowed.unwrap_or(self.camera_allowed),
            max_camera_participants: within(
                "people with their camera on",
                update.max_camera_participants,
                1,
                CAMERA_PARTICIPANTS_MAX,
            )?
            .unwrap_or(self.max_camera_participants),
            max_voice_bitrate: within(
                "the voice bitrate",
                update.max_voice_bitrate,
                SERVER_BITRATE_MIN,
                SERVER_BITRATE_MAX,
            )?
            .unwrap_or(self.max_voice_bitrate),
            afk_channel_id,
            afk_timeout_s: one_of(
                "the AFK timeout",
                update.afk_timeout_s,
                &AFK_TIMEOUT_CHOICES_S,
            )?
            .unwrap_or(self.afk_timeout_s),
            soundboard_enabled: update.soundboard_enabled.unwrap_or(self.soundboard_enabled),
            allow_default_sounds: update
                .allow_default_sounds
                .unwrap_or(self.allow_default_sounds),
            allow_external_sounds: update
                .allow_external_sounds
                .unwrap_or(self.allow_external_sounds),
            sound_cooldown_s: within(
                "the sound cooldown",
                update.sound_cooldown_s,
                0,
                SOUND_COOLDOWN_MAX_S,
            )?
            .unwrap_or(self.sound_cooldown_s),
            max_sounds: within("sounds", update.max_sounds, 0, MAX_SOUNDS_LIMIT)?
                .unwrap_or(self.max_sounds),
        })
    }

    pub fn to_proto(&self) -> proto::VoiceSettings {
        proto::VoiceSettings {
            screen_share_max_resolution: self.screen_share_max_resolution.to_proto() as i32,
            screen_share_max_fps: self.screen_share_max_fps,
            max_stream_viewers: self.max_stream_viewers,
            camera_allowed: self.camera_allowed,
            max_camera_participants: self.max_camera_participants,
            max_voice_bitrate: self.max_voice_bitrate,
            afk_channel_id: self.afk_channel_id,
            afk_timeout_s: self.afk_timeout_s,
            soundboard_enabled: self.soundboard_enabled,
            allow_default_sounds: self.allow_default_sounds,
            allow_external_sounds: self.allow_external_sounds,
            sound_cooldown_s: self.sound_cooldown_s,
            max_sounds: self.max_sounds,
        }
    }

    /// Stored settings, with defaults for anything never saved.
    pub async fn load(conn: &mut SqliteConnection) -> Result<Self, meta::MetaError> {
        let values: HashMap<String, String> = meta::load_prefixed(conn, PREFIX).await?;
        let defaults = Self::default();
        let get = |key: &'static str| values.get(&format!("{PREFIX}{key}"));
        let number = |key: &'static str, default: u32| {
            get(key).map_or(Ok(default), |value| {
                value.parse().map_err(|_| meta::MetaError::Corrupt { key })
            })
        };
        let flag = |key: &'static str, default: bool| {
            get(key).map_or(Ok(default), |value| match value.as_str() {
                "true" => Ok(true),
                "false" => Ok(false),
                _ => Err(meta::MetaError::Corrupt { key }),
            })
        };
        Ok(Self {
            screen_share_max_resolution: get(SCREEN_SHARE_MAX_RESOLUTION).map_or(
                Ok(defaults.screen_share_max_resolution),
                |value| {
                    Resolution::ALL
                        .into_iter()
                        .find(|resolution| resolution.as_str() == value)
                        .ok_or(meta::MetaError::Corrupt {
                            key: SCREEN_SHARE_MAX_RESOLUTION,
                        })
                },
            )?,
            screen_share_max_fps: number(SCREEN_SHARE_MAX_FPS, defaults.screen_share_max_fps)?,
            max_stream_viewers: number(MAX_STREAM_VIEWERS, defaults.max_stream_viewers)?,
            camera_allowed: flag(CAMERA_ALLOWED, defaults.camera_allowed)?,
            max_camera_participants: number(
                MAX_CAMERA_PARTICIPANTS,
                defaults.max_camera_participants,
            )?,
            max_voice_bitrate: number(MAX_VOICE_BITRATE, defaults.max_voice_bitrate)?,
            afk_channel_id: get(AFK_CHANNEL_ID)
                .map(|value| {
                    value.parse().map_err(|_| meta::MetaError::Corrupt {
                        key: AFK_CHANNEL_ID,
                    })
                })
                .transpose()?,
            afk_timeout_s: number(AFK_TIMEOUT_S, defaults.afk_timeout_s)?,
            soundboard_enabled: flag(SOUNDBOARD_ENABLED, defaults.soundboard_enabled)?,
            allow_default_sounds: flag(ALLOW_DEFAULT_SOUNDS, defaults.allow_default_sounds)?,
            allow_external_sounds: flag(ALLOW_EXTERNAL_SOUNDS, defaults.allow_external_sounds)?,
            sound_cooldown_s: number(SOUND_COOLDOWN_S, defaults.sound_cooldown_s)?,
            max_sounds: number(MAX_SOUNDS, defaults.max_sounds)?,
        })
    }

    pub async fn save(&self, conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
        let entries = [
            (
                SCREEN_SHARE_MAX_RESOLUTION,
                Some(self.screen_share_max_resolution.as_str().to_owned()),
            ),
            (
                SCREEN_SHARE_MAX_FPS,
                Some(self.screen_share_max_fps.to_string()),
            ),
            (
                MAX_STREAM_VIEWERS,
                Some(self.max_stream_viewers.to_string()),
            ),
            (CAMERA_ALLOWED, Some(self.camera_allowed.to_string())),
            (
                MAX_CAMERA_PARTICIPANTS,
                Some(self.max_camera_participants.to_string()),
            ),
            (MAX_VOICE_BITRATE, Some(self.max_voice_bitrate.to_string())),
            (AFK_CHANNEL_ID, self.afk_channel_id.map(|id| id.to_string())),
            (AFK_TIMEOUT_S, Some(self.afk_timeout_s.to_string())),
            (
                SOUNDBOARD_ENABLED,
                Some(self.soundboard_enabled.to_string()),
            ),
            (
                ALLOW_DEFAULT_SOUNDS,
                Some(self.allow_default_sounds.to_string()),
            ),
            (
                ALLOW_EXTERNAL_SOUNDS,
                Some(self.allow_external_sounds.to_string()),
            ),
            (SOUND_COOLDOWN_S, Some(self.sound_cooldown_s.to_string())),
            (MAX_SOUNDS, Some(self.max_sounds.to_string())),
        ];
        for (key, value) in entries {
            meta::put(conn, &format!("{PREFIX}{key}"), value.as_deref()).await?;
        }
        Ok(())
    }
}

fn within(what: &str, value: Option<u32>, min: u32, max: u32) -> Result<Option<u32>, ApiError> {
    match value {
        Some(value) if !(min..=max).contains(&value) => Err(ApiError::invalid_argument(format!(
            "{what} must be from {min} to {max}"
        ))),
        value => Ok(value),
    }
}

fn one_of(what: &str, value: Option<u32>, choices: &[u32]) -> Result<Option<u32>, ApiError> {
    match value {
        Some(value) if !choices.contains(&value) => Err(ApiError::invalid_argument(format!(
            "{what} must be one of {choices:?}"
        ))),
        value => Ok(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_pool;

    const VOICE_CHANNEL: i64 = 10;

    fn is_voice(id: i64) -> bool {
        id == VOICE_CHANNEL
    }

    fn rejected(update: proto::UpdateVoiceSettings) -> bool {
        VoiceSettings::default().apply(&update, is_voice).is_err()
    }

    #[test]
    fn defaults_follow_the_plan() {
        let settings = VoiceSettings::default().to_proto();

        assert_eq!(
            settings.screen_share_max_resolution,
            proto::ScreenShareResolution::ScreenShareResolution720p as i32
        );
        assert_eq!(settings.screen_share_max_fps, 30);
        assert_eq!(settings.max_stream_viewers, 50);
        assert!(settings.camera_allowed);
        assert_eq!(settings.max_camera_participants, 25);
        assert_eq!(settings.max_voice_bitrate, 96_000);
        assert_eq!(settings.afk_channel_id, None);
        assert_eq!(settings.afk_timeout_s, 300);
        assert!(settings.soundboard_enabled);
        assert_eq!(settings.sound_cooldown_s, 3);
        assert_eq!(settings.max_sounds, 48);
    }

    #[test]
    fn updates_change_only_what_they_name() {
        let update = proto::UpdateVoiceSettings {
            screen_share_max_resolution: Some(
                proto::ScreenShareResolution::ScreenShareResolution1080p as i32,
            ),
            screen_share_max_fps: Some(60),
            afk_channel_id: Some(VOICE_CHANNEL),
            afk_timeout_s: Some(900),
            ..Default::default()
        };

        let updated = VoiceSettings::default().apply(&update, is_voice).unwrap();

        assert_eq!(updated.screen_share_max_resolution, Resolution::P1080);
        assert_eq!(updated.screen_share_max_fps, 60);
        assert_eq!(updated.afk_channel_id, Some(VOICE_CHANNEL));
        assert_eq!(updated.afk_timeout_s, 900);
        assert_eq!(updated.max_stream_viewers, 50);
    }

    #[test]
    fn afk_channel_zero_clears_it() {
        let with_afk = VoiceSettings {
            afk_channel_id: Some(VOICE_CHANNEL),
            ..VoiceSettings::default()
        };
        let update = proto::UpdateVoiceSettings {
            afk_channel_id: Some(0),
            ..Default::default()
        };

        assert_eq!(
            with_afk.apply(&update, is_voice).unwrap().afk_channel_id,
            None
        );
    }

    #[test]
    fn values_out_of_range_are_rejected() {
        let cases = [
            proto::UpdateVoiceSettings {
                screen_share_max_resolution: Some(0),
                ..Default::default()
            },
            proto::UpdateVoiceSettings {
                screen_share_max_fps: Some(24),
                ..Default::default()
            },
            proto::UpdateVoiceSettings {
                max_stream_viewers: Some(0),
                ..Default::default()
            },
            proto::UpdateVoiceSettings {
                max_stream_viewers: Some(201),
                ..Default::default()
            },
            proto::UpdateVoiceSettings {
                max_camera_participants: Some(51),
                ..Default::default()
            },
            proto::UpdateVoiceSettings {
                max_voice_bitrate: Some(31_999),
                ..Default::default()
            },
            proto::UpdateVoiceSettings {
                max_voice_bitrate: Some(256_001),
                ..Default::default()
            },
            proto::UpdateVoiceSettings {
                afk_channel_id: Some(VOICE_CHANNEL + 1),
                ..Default::default()
            },
            proto::UpdateVoiceSettings {
                afk_timeout_s: Some(120),
                ..Default::default()
            },
            proto::UpdateVoiceSettings {
                sound_cooldown_s: Some(31),
                ..Default::default()
            },
            proto::UpdateVoiceSettings {
                max_sounds: Some(201),
                ..Default::default()
            },
        ];

        for case in cases {
            assert!(rejected(case), "{case:?} was accepted");
        }
    }

    #[tokio::test]
    async fn saved_settings_load_back_and_missing_ones_default() {
        let (pool, _dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(
            VoiceSettings::load(&mut conn).await.unwrap(),
            VoiceSettings::default()
        );
        let changed = VoiceSettings {
            screen_share_max_resolution: Resolution::Source,
            camera_allowed: false,
            afk_channel_id: Some(VOICE_CHANNEL),
            soundboard_enabled: false,
            max_sounds: 0,
            ..VoiceSettings::default()
        };

        changed.save(&mut conn).await.unwrap();

        assert_eq!(VoiceSettings::load(&mut conn).await.unwrap(), changed);
        VoiceSettings::default().save(&mut conn).await.unwrap();
        assert_eq!(
            VoiceSettings::load(&mut conn).await.unwrap(),
            VoiceSettings::default()
        );
    }
}
