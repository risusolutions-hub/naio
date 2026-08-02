//! Auth errors — mapped to Niao E-codes in the runtime binder.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    InvalidParameter(String),
    BadCredentials,
    Forbidden(String),
    CsrfMismatch,
    Expired(String),
    BadSession(String),
    Password(String),
    Sign(String),
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidParameter(m) => write!(f, "{m}"),
            Self::BadCredentials => write!(f, "invalid credentials"),
            Self::Forbidden(m) => write!(f, "forbidden: {m}"),
            Self::CsrfMismatch => write!(f, "CSRF token mismatch"),
            Self::Expired(m) => write!(f, "expired: {m}"),
            Self::BadSession(m) => write!(f, "bad session: {m}"),
            Self::Password(m) => write!(f, "password error: {m}"),
            Self::Sign(m) => write!(f, "sign error: {m}"),
        }
    }
}

impl std::error::Error for AuthError {}

pub type AuthResult<T> = Result<T, AuthError>;

impl From<niao_sign::SignError> for AuthError {
    fn from(e: niao_sign::SignError) -> Self {
        match e {
            niao_sign::SignError::Expired { age_secs, max_age } => {
                Self::Expired(format!("age {age_secs}s > max {max_age}s"))
            }
            other => Self::Sign(other.to_string()),
        }
    }
}

impl From<niao_pass::PassError> for AuthError {
    fn from(e: niao_pass::PassError) -> Self {
        Self::Password(e.message())
    }
}
