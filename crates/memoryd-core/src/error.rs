//! Error types for memoryd core operations.

use std::error::Error;
use std::fmt::{Display, Formatter, Result as FormatResult};

/// Result type returned by memoryd core operations.
pub type MemorydResult<T> = Result<T, MemorydError>;

/// Error type returned by memoryd core operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MemorydError {
    /// The caller provided invalid input.
    InvalidInput(String),
    /// A requested record could not be found.
    NotFound(String),
    /// The requested operation conflicts with current state.
    Conflict(String),
    /// The storage adapter failed.
    Storage(String),
}

impl Display for MemorydError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FormatResult {
        match self {
            Self::InvalidInput(message) => write!(formatter, "invalid input: {message}"),
            Self::NotFound(message) => write!(formatter, "not found: {message}"),
            Self::Conflict(message) => write!(formatter, "conflict: {message}"),
            Self::Storage(message) => write!(formatter, "storage error: {message}"),
        }
    }
}

impl Error for MemorydError {}
