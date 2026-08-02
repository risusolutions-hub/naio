//! Error type for `niao_scrape` (never panic for recoverable failures).

use std::fmt;

/// Maximum HTML / XML / robots input size (16 MiB).
pub const MAX_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ScrapeError {
    message: String,
}

impl ScrapeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ScrapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ScrapeError {}

pub type ScrapeResult<T> = Result<T, ScrapeError>;

pub fn check_len(len: usize) -> ScrapeResult<()> {
    if len > MAX_BYTES {
        return Err(ScrapeError::new(format!(
            "input size {len} exceeds limit {MAX_BYTES}"
        )));
    }
    Ok(())
}
