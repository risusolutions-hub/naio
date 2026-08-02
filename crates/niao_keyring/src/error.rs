use std::fmt;

/// Result type for keyring operations.
pub type KeyringResult<T> = Result<T, KeyringError>;

/// Errors from credential store operations.
#[derive(Debug)]
pub enum KeyringError {
    /// Credential not found (maps to Python returning `None` for get, error for delete).
    NotFound,
    /// OS store unavailable or access denied.
    Access(String),
    /// Invalid service/username or payload too large.
    Invalid(String),
    /// Platform store returned malformed data.
    BadData(String),
    /// Other platform-specific failure.
    Platform(String),
}

impl fmt::Display for KeyringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "credential not found"),
            Self::Access(msg) => write!(f, "credential store access denied: {msg}"),
            Self::Invalid(msg) => write!(f, "invalid credential: {msg}"),
            Self::BadData(msg) => write!(f, "malformed credential data: {msg}"),
            Self::Platform(msg) => write!(f, "platform keyring error: {msg}"),
        }
    }
}

impl std::error::Error for KeyringError {}

impl From<keyring::Error> for KeyringError {
    fn from(err: keyring::Error) -> Self {
        match &err {
            keyring::Error::NoEntry => Self::NotFound,
            keyring::Error::NoStorageAccess(e) => Self::Access(e.to_string()),
            keyring::Error::Invalid(a, b) => Self::Invalid(format!("{a}: {b}")),
            keyring::Error::TooLong(a, n) => Self::Invalid(format!("{a} exceeds max length ({n})")),
            keyring::Error::BadEncoding(_) => Self::BadData(err.to_string()),
            keyring::Error::PlatformFailure(e) => Self::Platform(e.to_string()),
            keyring::Error::Ambiguous(_) => Self::Platform(err.to_string()),
            _ => Self::Platform(err.to_string()),
        }
    }
}
