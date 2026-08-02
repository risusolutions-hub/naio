//! Crate-level errors for IMAP/POP3 — never panic into Niao.

use std::fmt;

#[derive(Debug, Clone)]
pub enum ImapError {
    Io(String),
    Tls(String),
    Protocol(String),
    Auth(String),
    Timeout(String),
    InvalidArg(String),
    NotConnected,
    WrongState(String),
}

pub type Result<T> = std::result::Result<T, ImapError>;

impl ImapError {
    pub fn message(&self) -> String {
        match self {
            ImapError::Io(m)
            | ImapError::Tls(m)
            | ImapError::Protocol(m)
            | ImapError::Auth(m)
            | ImapError::Timeout(m)
            | ImapError::InvalidArg(m)
            | ImapError::WrongState(m) => m.clone(),
            ImapError::NotConnected => "not connected".into(),
        }
    }

    pub fn is_protocol(&self) -> bool {
        matches!(self, ImapError::Protocol(_) | ImapError::Auth(_))
    }
}

impl fmt::Display for ImapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for ImapError {}

impl From<std::io::Error> for ImapError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::TimedOut || e.kind() == std::io::ErrorKind::WouldBlock {
            ImapError::Timeout(e.to_string())
        } else {
            ImapError::Io(e.to_string())
        }
    }
}
