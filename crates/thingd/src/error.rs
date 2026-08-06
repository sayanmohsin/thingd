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
    /// An explicit encryption migration failed.
    EncryptionMigration(String),
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
            Self::EncryptionMigration(message) => {
                write!(formatter, "encryption migration failed: {message}")
            },
        }
    }
}

impl Error for ThingdError {}

#[cfg(feature = "persistent")]
impl From<fjall::Error> for ThingdError {
    fn from(error: fjall::Error) -> Self {
        Self::Storage(error.to_string())
    }
}
