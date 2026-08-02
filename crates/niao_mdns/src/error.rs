//! Domain errors for niao_mdns (no panics; surfaced as Niao errors at the boundary).

use std::fmt;

/// Result alias for the core crate.
pub type MdnsResult<T> = Result<T, MdnsError>;

/// Recoverable mDNS / DNS-SD errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MdnsError {
    /// Invalid argument or malformed input.
    Invalid(String),
    /// Wire / protocol decode failure.
    Decode(String),
    /// Encode failure (oversized names, bad lengths).
    Encode(String),
    /// Socket / network I/O failure.
    Io(String),
    /// Operation timed out waiting for responses.
    Timeout(String),
}

impl MdnsError {
    pub fn message(&self) -> &str {
        match self {
            Self::Invalid(s)
            | Self::Decode(s)
            | Self::Encode(s)
            | Self::Io(s)
            | Self::Timeout(s) => s.as_str(),
        }
    }
}

impl fmt::Display for MdnsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for MdnsError {}

impl From<std::io::Error> for MdnsError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}
