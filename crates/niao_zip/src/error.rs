use std::fmt;

#[derive(Debug)]
pub enum ZipError {
    Io(std::io::Error),
    Archive(String),
    NotFound(String),
    PasswordRequired(String),
    BadPassword(String),
    InvalidArchive(String),
    InvalidMode(String),
    EntryBusy,
    Closed,
}

pub type ZipResult<T> = Result<T, ZipError>;

impl fmt::Display for ZipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Archive(msg) => write!(f, "{msg}"),
            Self::NotFound(name) => write!(f, "zip entry not found: {name}"),
            Self::PasswordRequired(name) => write!(f, "password required for entry: {name}"),
            Self::BadPassword(name) => write!(f, "bad password for entry: {name}"),
            Self::InvalidArchive(msg) => write!(f, "invalid zip archive: {msg}"),
            Self::InvalidMode(msg) => write!(f, "invalid zip mode: {msg}"),
            Self::EntryBusy => write!(f, "another entry stream is open"),
            Self::Closed => write!(f, "zip handle is closed"),
        }
    }
}

impl std::error::Error for ZipError {}

impl From<std::io::Error> for ZipError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<zip::result::ZipError> for ZipError {
    fn from(value: zip::result::ZipError) -> Self {
        let msg = value.to_string();
        if msg.contains("password") || msg.contains("Password") {
            Self::PasswordRequired(msg)
        } else if msg.contains("Invalid") || msg.contains("invalid") {
            Self::InvalidArchive(msg)
        } else {
            Self::Archive(msg)
        }
    }
}
