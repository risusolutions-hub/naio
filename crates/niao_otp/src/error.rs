use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OtpError {
    InvalidSecret(String),
    InvalidBase32(String),
    InvalidDigits(u32),
    InvalidInterval(u64),
    InvalidDigest(String),
    InvalidUri(String),
    InvalidToken(String),
    EmptyInput,
}

impl fmt::Display for OtpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSecret(m) => write!(f, "invalid secret: {m}"),
            Self::InvalidBase32(m) => write!(f, "invalid base32: {m}"),
            Self::InvalidDigits(n) => write!(f, "digits must be 1..=10, got {n}"),
            Self::InvalidInterval(n) => write!(f, "interval must be > 0, got {n}"),
            Self::InvalidDigest(m) => write!(f, "invalid digest: {m}"),
            Self::InvalidUri(m) => write!(f, "invalid otpauth URI: {m}"),
            Self::InvalidToken(m) => write!(f, "invalid token: {m}"),
            Self::EmptyInput => write!(f, "empty input"),
        }
    }
}

impl std::error::Error for OtpError {}
