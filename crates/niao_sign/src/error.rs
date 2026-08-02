//! Signing and deserialization errors.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignError {
    BadSignature,
    BadFormat,
    BadPayload(String),
    Expired { age_secs: i64, max_age: u64 },
    MalformedTimestamp,
    TimestampMissing,
    InvalidSeparator,
    PayloadTooLarge,
}

impl fmt::Display for SignError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadSignature => write!(f, "bad signature"),
            Self::BadFormat => write!(f, "invalid signed value format"),
            Self::BadPayload(msg) => write!(f, "invalid payload: {msg}"),
            Self::Expired { age_secs, max_age } => {
                write!(f, "signature expired (age {age_secs}s > max {max_age}s)")
            }
            Self::MalformedTimestamp => write!(f, "malformed timestamp"),
            Self::TimestampMissing => write!(f, "timestamp missing"),
            Self::InvalidSeparator => write!(f, "separator conflicts with base64 alphabet"),
            Self::PayloadTooLarge => write!(f, "payload exceeds maximum size"),
        }
    }
}

impl std::error::Error for SignError {}

/// Result of an unsafe load: validity flag plus optional payload and timestamp.
#[derive(Debug, Clone)]
pub struct UnsafeLoad<T> {
    pub valid: bool,
    pub value: Option<T>,
    pub timestamp: Option<u64>,
    pub expired: bool,
    pub error: Option<String>,
}
