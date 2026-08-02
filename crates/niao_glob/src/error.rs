//! Error types for glob / pathspec operations.

use std::fmt;

/// Unified error surfaced by `niao_glob`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobError {
    InvalidPattern(String),
    Io(String),
}

impl fmt::Display for GlobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GlobError::InvalidPattern(msg) => write!(f, "invalid glob pattern: {msg}"),
            GlobError::Io(msg) => write!(f, "io error: {msg}"),
        }
    }
}

impl std::error::Error for GlobError {}

impl From<globset::Error> for GlobError {
    fn from(e: globset::Error) -> Self {
        GlobError::InvalidPattern(e.to_string())
    }
}

impl From<std::io::Error> for GlobError {
    fn from(e: std::io::Error) -> Self {
        GlobError::Io(e.to_string())
    }
}
