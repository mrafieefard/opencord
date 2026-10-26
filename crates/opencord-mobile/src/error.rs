//! `MobileError`: the core's `CoreError`, variant for variant, as JavaScript
//! sees it. A server's `ErrorCode` crosses as its name in snake case, so
//! the facade doesn't mirror sixty codes.

use opencord_core::api::types::{CoreError, ErrorCode};

use crate::types::FailureKind;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum MobileError {
    #[error("the core has not been initialized")]
    NotInitialized,
    #[error("no identity has been loaded")]
    NoIdentity,
    #[error("that server is not in the list")]
    UnknownServer,
    #[error("not connected to that server right now")]
    NotConnected,
    #[error("the server did not answer in time")]
    Timeout,
    #[error("{message}")]
    InvalidInput { message: String },
    #[error("{message}")]
    Server {
        code: String,
        message: String,
        retry_after_ms: Option<u32>,
    },
    #[error("the server's certificate changed: expected {expected}, got {presented}")]
    FingerprintMismatch { expected: String, presented: String },
    #[error("{message}")]
    Rejected {
        reason: FailureKind,
        message: String,
    },
    #[error("could not connect: {message}")]
    Connection { message: String },
    #[error("could not save local data: {message}")]
    Storage { message: String },
    #[error("cancelled")]
    Cancelled,
    /// Anything the phone apps have no use for yet (camera, screen).
    #[error("{message}")]
    Other { message: String },
}

impl From<CoreError> for MobileError {
    fn from(error: CoreError) -> Self {
        match error {
            CoreError::NotInitialized => Self::NotInitialized,
            CoreError::NoIdentity => Self::NoIdentity,
            CoreError::UnknownServer => Self::UnknownServer,
            CoreError::NotConnected => Self::NotConnected,
            CoreError::Timeout => Self::Timeout,
            CoreError::InvalidInput { message } => Self::InvalidInput { message },
            CoreError::Server {
                code,
                message,
                retry_after_ms,
            } => Self::Server {
                code: code_name(code),
                message,
                retry_after_ms,
            },
            CoreError::FingerprintMismatch {
                expected,
                presented,
            } => Self::FingerprintMismatch {
                expected,
                presented,
            },
            CoreError::Rejected { reason, message } => Self::Rejected {
                reason: reason.into(),
                message,
            },
            CoreError::Connection { message } => Self::Connection { message },
            CoreError::Storage { message } => Self::Storage { message },
            CoreError::Screen { message, .. } | CoreError::Camera { message, .. } => {
                Self::Other { message }
            }
            CoreError::Cancelled => Self::Cancelled,
        }
    }
}

/// `RateLimited` → `rate_limited`.
fn code_name(code: ErrorCode) -> String {
    let name = format!("{code:?}");
    let mut snake = String::with_capacity(name.len() + 4);
    for (index, char) in name.chars().enumerate() {
        if char.is_ascii_uppercase() {
            if index > 0 {
                snake.push('_');
            }
            snake.push(char.to_ascii_lowercase());
        } else {
            snake.push(char);
        }
    }
    snake
}

#[cfg(test)]
mod tests {
    use opencord_core::api::types::{CameraProblem, FailureReason, ScreenProblem};

    use super::*;

    #[test]
    fn every_core_error_has_its_mobile_error() {
        let message = || "words".to_owned();
        let cases = [
            (CoreError::NotInitialized, MobileError::NotInitialized),
            (CoreError::NoIdentity, MobileError::NoIdentity),
            (CoreError::UnknownServer, MobileError::UnknownServer),
            (CoreError::NotConnected, MobileError::NotConnected),
            (CoreError::Timeout, MobileError::Timeout),
            (
                CoreError::InvalidInput { message: message() },
                MobileError::InvalidInput { message: message() },
            ),
            (
                CoreError::Server {
                    code: ErrorCode::RateLimited,
                    message: message(),
                    retry_after_ms: Some(500),
                },
                MobileError::Server {
                    code: "rate_limited".to_owned(),
                    message: message(),
                    retry_after_ms: Some(500),
                },
            ),
            (
                CoreError::Server {
                    code: ErrorCode::MusicNotInChannel,
                    message: message(),
                    retry_after_ms: None,
                },
                MobileError::Server {
                    code: "music_not_in_channel".to_owned(),
                    message: message(),
                    retry_after_ms: None,
                },
            ),
            (
                CoreError::FingerprintMismatch {
                    expected: "a".to_owned(),
                    presented: "b".to_owned(),
                },
                MobileError::FingerprintMismatch {
                    expected: "a".to_owned(),
                    presented: "b".to_owned(),
                },
            ),
            (
                CoreError::Rejected {
                    reason: FailureReason::Banned,
                    message: message(),
                },
                MobileError::Rejected {
                    reason: FailureKind::Banned,
                    message: message(),
                },
            ),
            (
                CoreError::Connection { message: message() },
                MobileError::Connection { message: message() },
            ),
            (
                CoreError::Storage { message: message() },
                MobileError::Storage { message: message() },
            ),
            (
                CoreError::Screen {
                    problem: ScreenProblem::NotSupported,
                    message: message(),
                },
                MobileError::Other { message: message() },
            ),
            (
                CoreError::Camera {
                    problem: CameraProblem::NotSupported,
                    message: message(),
                },
                MobileError::Other { message: message() },
            ),
            (CoreError::Cancelled, MobileError::Cancelled),
        ];
        for (core, mobile) in cases {
            assert_eq!(MobileError::from(core), mobile);
        }
    }
}
