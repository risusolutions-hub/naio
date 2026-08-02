use std::fmt;

/// Errors from MIME email compose / parse operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MailError {
    TooLarge(usize),
    Empty,
    Parse(String),
    MissingField(String),
    InvalidAddress(String),
    InvalidHeader(String),
    Io(String),
    Encode(String),
}

impl MailError {
    pub fn message(&self) -> String {
        match self {
            MailError::TooLarge(n) => format!("input exceeds {n} byte limit"),
            MailError::Empty => "empty input".into(),
            MailError::Parse(s) => format!("parse error: {s}"),
            MailError::MissingField(f) => format!("missing required field '{f}'"),
            MailError::InvalidAddress(s) => format!("invalid address: {s}"),
            MailError::InvalidHeader(s) => format!("invalid header: {s}"),
            MailError::Io(s) => s.clone(),
            MailError::Encode(s) => format!("encode error: {s}"),
        }
    }

    pub fn is_parse(&self) -> bool {
        matches!(
            self,
            MailError::Parse(_) | MailError::Empty | MailError::InvalidHeader(_)
        )
    }
}

impl fmt::Display for MailError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for MailError {}

/// Maximum input/output size (64 MiB guard).
pub const MAX_BYTES: usize = 64 * 1024 * 1024;
