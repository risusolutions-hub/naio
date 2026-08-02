//! Error type for the FTS engine (surfaced as Niao error values at the VM boundary).

use std::fmt;

/// Recoverable engine error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FtsError {
    pub message: String,
}

impl FtsError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }
}

impl fmt::Display for FtsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for FtsError {}

impl From<std::io::Error> for FtsError {
    fn from(e: std::io::Error) -> Self {
        Self::new(format!("io error: {e}"))
    }
}

impl From<serde_json::Error> for FtsError {
    fn from(e: serde_json::Error) -> Self {
        Self::new(format!("persist error: {e}"))
    }
}

pub type FtsResult<T> = Result<T, FtsError>;
