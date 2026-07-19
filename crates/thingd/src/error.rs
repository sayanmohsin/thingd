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
}

impl Display for ThingdError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FormatResult {
        match self {
            Self::InvalidInput(message) => write!(formatter, "invalid input: {message}"),
            Self::NotFound(message) => write!(formatter, "not found: {message}"),
            Self::Conflict(message) => write!(formatter, "conflict: {message}"),
            Self::Protected(message) => write!(formatter, "protected: {message}"),
            Self::Storage(message) => write!(formatter, "storage error: {message}"),
        }
    }
}

impl Error for ThingdError {}

#[cfg(feature = "fjall")]
impl From<fjall::Error> for ThingdError {
    fn from(error: fjall::Error) -> Self {
        Self::Storage(error.to_string())
    }
}
