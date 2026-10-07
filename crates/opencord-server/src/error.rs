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

    pub fn camera_limit() -> Self {
        Self::new(
            proto::ErrorCode::CameraLimit,
            "the camera limit for this channel has been reached",
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
