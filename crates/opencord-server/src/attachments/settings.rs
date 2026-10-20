//! Server-wide upload settings (Phase 4).

use std::collections::HashMap;

use opencord_proto::v1 as proto;
use sqlx::SqliteConnection;

use crate::db::meta;

const PREFIX: &str = "uploads.";
const BLOCKED_EXTENSIONS: &str = "uploads.blocked_extensions";

/// What the owner sets. A voice message's maximum length isn't one of
/// them: it is [`opencord_common::upload::VOICE_MAX_SECONDS`] everywhere.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UploadSettings {
    pub blocked_extensions: Vec<String>,
}

impl UploadSettings {
    pub async fn load(conn: &mut SqliteConnection) -> Result<Self, meta::MetaError> {
        let values: HashMap<String, String> = meta::load_prefixed(conn, PREFIX).await?;
        let defaults = Self::default();
        let blocked_extensions = match values.get(BLOCKED_EXTENSIONS) {
            Some(s) => serde_json::from_str(s).map_err(|_| meta::MetaError::Corrupt {
                key: BLOCKED_EXTENSIONS,
            })?,
            None => defaults.blocked_extensions,
        };
        Ok(Self { blocked_extensions })
    }

    pub async fn save(&self, conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
        let json = serde_json::to_string(&self.blocked_extensions)
            .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
        meta::put(conn, BLOCKED_EXTENSIONS, Some(&json)).await?;
        Ok(())
    }

    pub fn to_proto(&self, max_file_bytes: u64) -> proto::UploadSettings {
        proto::UploadSettings {
            blocked_extensions: self.blocked_extensions.clone(),
            voice_message_max_seconds: opencord_common::upload::VOICE_MAX_SECONDS,
            max_file_bytes,
            max_attachments: 10,
            piece_bytes: opencord_common::upload::PIECE_BYTES,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_pool;

    #[tokio::test]
    async fn upload_settings_round_trip_and_default() {
        let (pool, _dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        let default_settings = UploadSettings::load(&mut conn).await.unwrap();
        assert_eq!(default_settings, UploadSettings::default());

        let custom_settings = UploadSettings {
            blocked_extensions: vec!["exe".to_string(), "bat".to_string()],
        };
        custom_settings.save(&mut conn).await.unwrap();

        let loaded = UploadSettings::load(&mut conn).await.unwrap();
        assert_eq!(loaded, custom_settings);

        let proto = loaded.to_proto(50_000_000);
        assert_eq!(proto.blocked_extensions, vec!["exe", "bat"]);
        assert_eq!(proto.voice_message_max_seconds, 1_800);
        assert_eq!(proto.max_file_bytes, 50_000_000);
        assert_eq!(proto.max_attachments, 10);
        assert_eq!(proto.piece_bytes, 8 * 1024 * 1024);
    }
}
