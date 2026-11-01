//! Private messages (design 2026-10-22): end-to-end encrypted messages
//! between people, relayed by any server they share, and kept in this
//! device's own encrypted database.

pub mod actions;
pub mod chat_key;
pub mod contacts;
pub mod device;
pub mod inbox;
pub mod members;
pub mod outbox;
pub mod own_devices;
pub mod payload;
pub mod seal;
pub mod service;
pub mod sessions;
pub mod store;
pub mod trust;

use crate::api::types::CoreError;

#[derive(Debug, thiserror::Error)]
pub enum PrivateError {
    #[error("private messages storage failed: {0}")]
    Storage(String),
    #[error("{0}")]
    Invalid(String),
    #[error("private messages encryption failed: {0}")]
    Crypto(String),
    /// A request to a server failed.
    #[error(transparent)]
    Request(CoreError),
}

impl From<sqlx::Error> for PrivateError {
    fn from(error: sqlx::Error) -> Self {
        Self::Storage(error.to_string())
    }
}

impl From<std::io::Error> for PrivateError {
    fn from(error: std::io::Error) -> Self {
        Self::Storage(error.to_string())
    }
}

impl From<PrivateError> for CoreError {
    fn from(error: PrivateError) -> Self {
        match error {
            PrivateError::Invalid(message) => Self::InvalidInput { message },
            PrivateError::Request(error) => error,
            PrivateError::Storage(_) | PrivateError::Crypto(_) => Self::Storage {
                message: error.to_string(),
            },
        }
    }
}
