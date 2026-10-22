//! Errors returned to clients.

use std::time::Duration;

use opencord_common::validation::ValidationError;
use opencord_proto::v1 as proto;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{code:?}: {message}")]
pub struct ApiError {
    pub code: proto::ErrorCode,
    pub message: String,
    pub retry_after_ms: Option<u64>,
}

impl ApiError {
    pub fn new(code: proto::ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retry_after_ms: None,
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(proto::ErrorCode::Unauthorized, message)
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(proto::ErrorCode::Forbidden, message)
    }

    pub fn not_found(what: &str) -> Self {
        Self::new(proto::ErrorCode::NotFound, format!("{what} not found"))
    }

    pub fn invalid_argument(message: impl Into<String>) -> Self {
        Self::new(proto::ErrorCode::InvalidArgument, message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(proto::ErrorCode::Conflict, message)
    }

    pub fn invalid_session() -> Self {
        Self::new(
            proto::ErrorCode::InvalidSession,
            "the session cannot be resumed; identify again",
        )
    }

    pub fn rate_limited(retry_after: Duration) -> Self {
        Self {
            retry_after_ms: Some(u64::try_from(retry_after.as_millis()).unwrap_or(u64::MAX)),
            ..Self::new(proto::ErrorCode::RateLimited, "slow down")
        }
    }

    /// This error, to be tried again after `wait`.
    #[must_use]
    pub fn retry_after(self, wait: Duration) -> Self {
        Self {
            retry_after_ms: Some(u64::try_from(wait.as_millis()).unwrap_or(u64::MAX)),
            ..self
        }
    }

    pub fn voice_channel_full() -> Self {
        Self::new(
            proto::ErrorCode::VoiceChannelFull,
            "that voice channel is full",
        )
    }

    pub fn voice_not_connected(who: &str) -> Self {
        Self::new(
            proto::ErrorCode::VoiceNotConnected,
            format!("{who} not in a voice channel"),
        )
    }

    pub fn quality_limit(message: impl Into<String>) -> Self {
        Self::new(proto::ErrorCode::QualityLimit, message)
    }

    /// The plan's wording (§9.5).
    pub fn stream_viewer_limit(max_viewers: u32) -> Self {
        Self::new(
            proto::ErrorCode::StreamViewerLimit,
            format!("This stream is full ({max_viewers} viewers)"),
        )
    }

    pub fn camera_limit() -> Self {
        Self::new(
            proto::ErrorCode::CameraLimit,
            "the camera limit for this channel has been reached",
        )
    }

    /// A user's cooldown, or a channel already playing its most sounds.
    pub fn sound_cooldown(message: impl Into<String>, retry_after: Option<Duration>) -> Self {
        Self {
            retry_after_ms: retry_after
                .map(|wait| u64::try_from(wait.as_millis()).unwrap_or(u64::MAX)),
            ..Self::new(proto::ErrorCode::SoundCooldown, message)
        }
    }

    pub fn sound_too_long(duration_ms: u32) -> Self {
        Self::new(
            proto::ErrorCode::SoundTooLong,
            format!(
                "sounds can be at most 5 seconds long, not {:.1}",
                f64::from(duration_ms) / 1_000.0
            ),
        )
    }

    pub fn sound_invalid(message: impl Into<String>) -> Self {
        Self::new(proto::ErrorCode::SoundInvalid, message)
    }

    pub fn soundboard_full(max_sounds: u32) -> Self {
        Self::new(
            proto::ErrorCode::SoundboardFull,
            format!("this server's soundboard is full ({max_sounds} sounds)"),
        )
    }

    pub fn file_too_large(limit_bytes: u64) -> Self {
        let n = (limit_bytes / (1024 * 1024)).max(1);
        Self::new(
            proto::ErrorCode::FileTooLarge,
            format!("files can be at most {n} MB here"),
        )
    }

    pub fn file_type_blocked(extension: &str) -> Self {
        Self::new(
            proto::ErrorCode::FileTypeBlocked,
            format!("{extension} files can't be uploaded here"),
        )
    }

    pub fn upload_offset_mismatch() -> Self {
        Self::new(
            proto::ErrorCode::UploadOffsetMismatch,
            "upload offset mismatch",
        )
    }

    pub fn storage_unavailable(message: impl Into<String>) -> Self {
        Self::new(proto::ErrorCode::StorageUnavailable, message)
    }

    pub fn storage_full() -> Self {
        Self::new(
            proto::ErrorCode::StorageFull,
            "the server is out of space for uploads",
        )
    }

    pub fn voice_message_invalid(message: impl Into<String>) -> Self {
        Self::new(proto::ErrorCode::VoiceMessageInvalid, message)
    }

    pub fn voice_message_too_long(max_seconds: u32) -> Self {
        let m = max_seconds / 60;
        let ss = max_seconds % 60;
        Self::new(
            proto::ErrorCode::VoiceMessageTooLong,
            format!("voice messages can be at most {m}:{ss:02} long"),
        )
    }

    pub fn internal() -> Self {
        Self::new(proto::ErrorCode::Internal, "internal server error")
    }

    pub fn to_proto(&self) -> proto::Error {
        proto::Error {
            code: self.code as i32,
            message: self.message.clone(),
            retry_after_ms: self.retry_after_ms,
        }
    }
}

impl From<ValidationError> for ApiError {
    fn from(error: ValidationError) -> Self {
        Self::invalid_argument(error.to_string())
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        tracing::error!(%error, "database error");
        Self::internal()
    }
}

impl From<crate::db::meta::MetaError> for ApiError {
    fn from(error: crate::db::meta::MetaError) -> Self {
        tracing::error!(%error, "server metadata error");
        Self::internal()
    }
}

impl From<crate::attachments::storage::StorageError> for ApiError {
    fn from(error: crate::attachments::storage::StorageError) -> Self {
        match error {
            crate::attachments::storage::StorageError::NotFound => {
                Self::not_found("attachment object")
            }
            crate::attachments::storage::StorageError::Unavailable(message) => {
                // Paths and S3 replies stay in the log, never in the reply.
                tracing::warn!(%message, "attachment storage failed");
                Self::storage_unavailable("the server's file storage isn't working right now")
            }
            crate::attachments::storage::StorageError::Full => Self::storage_full(),
        }
    }
}
