//! Error types for password hashing and policy checks.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PassError {
    EmptyPassword,
    PasswordTooLong { max: usize },
    UnknownScheme(String),
    UnsupportedScheme(String),
    InvalidHash(String),
    VerifyFailed,
    HashFailed(String),
    PolicyViolation(String),
    InvalidParameter(String),
}

impl PassError {
    pub fn message(&self) -> String {
        match self {
            Self::EmptyPassword => "password must not be empty".into(),
            Self::PasswordTooLong { max } => format!("password exceeds maximum length ({max})"),
            Self::UnknownScheme(s) => format!("unknown hash scheme: {s}"),
            Self::UnsupportedScheme(s) => format!("unsupported scheme in context: {s}"),
            Self::InvalidHash(msg) => format!("invalid hash string: {msg}"),
            Self::VerifyFailed => "password verification failed".into(),
            Self::HashFailed(msg) => format!("hashing failed: {msg}"),
            Self::PolicyViolation(msg) => msg.clone(),
            Self::InvalidParameter(msg) => format!("invalid parameter: {msg}"),
        }
    }
}

impl fmt::Display for PassError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for PassError {}

pub type PassResult<T> = Result<T, PassError>;

pub const MAX_PASSWORD_BYTES: usize = 1024;

pub fn check_password_len(password: &str) -> PassResult<()> {
    if password.is_empty() {
        return Err(PassError::EmptyPassword);
    }
    if password.len() > MAX_PASSWORD_BYTES {
        return Err(PassError::PasswordTooLong {
            max: MAX_PASSWORD_BYTES,
        });
    }
    Ok(())
}
