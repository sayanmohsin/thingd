//! Error types for thingd core operations.

use std::error::Error;
use std::fmt::{Display, Formatter, Result as FormatResult};

/// Result type returned by thingd core operations.
pub type ThingdResult<T> = Result<T, ThingdError>;

/// Error type returned by thingd core operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ThingdError {
    /// The caller provided invalid input.
    InvalidInput(String),
    /// A requested record could not be found.
    NotFound(String),
    /// The requested operation conflicts with current state.
    Conflict(String),
    /// The operation was attempted on a protected stream.
    Protected(String),
    /// The storage adapter failed.
    Storage(String),
    /// An encrypted database was opened without a key.
    EncryptionRequired(String),
    /// The supplied encryption key has an invalid format.
    InvalidEncryptionKey(String),
    /// Encrypted data failed authentication.
    EncryptionAuthentication(String),
    /// The encrypted storage format is not supported.
    UnsupportedEncryptionVersion(String),
    /// The native storage format is not supported or is incomplete.
    UnsupportedStorageFormat(String),
    /// The native storage directory failed a read-only validation check.
    StorageValidation(String),
    /// An explicit encryption migration failed.
    EncryptionMigration(String),
}

#[cfg(feature = "persistent-engine")]
impl From<crate::storage_backend::Error> for ThingdError {
    fn from(error: crate::storage_backend::Error) -> Self {
        Self::Storage(error.to_string())
    }
}

impl Display for ThingdError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FormatResult {
        match self {
            Self::InvalidInput(message) => write!(formatter, "invalid input: {message}"),
            Self::NotFound(message) => write!(formatter, "not found: {message}"),
            Self::Conflict(message) => write!(formatter, "conflict: {message}"),
            Self::Protected(message) => write!(formatter, "protected: {message}"),
            Self::Storage(message) => write!(formatter, "storage error: {message}"),
            Self::EncryptionRequired(message) => {
                write!(formatter, "encryption required: {message}")
            },
            Self::InvalidEncryptionKey(message) => {
                write!(formatter, "invalid encryption key: {message}")
            },
            Self::EncryptionAuthentication(message) => {
                write!(formatter, "encryption authentication failed: {message}")
            },
            Self::UnsupportedEncryptionVersion(message) => {
                write!(formatter, "unsupported encryption version: {message}")
            },
            Self::UnsupportedStorageFormat(message) => {
                write!(formatter, "unsupported storage format: {message}")
            },
            Self::StorageValidation(message) => {
                write!(formatter, "storage validation failed: {message}")
            },
            Self::EncryptionMigration(message) => {
                write!(formatter, "encryption migration failed: {message}")
            },
        }
    }
}

impl Error for ThingdError {}
