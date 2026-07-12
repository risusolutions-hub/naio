use std::fmt;

#[derive(Debug)]
pub enum PkgError {
    Io(std::io::Error),
    Json(String),
    NotFound(String),
    AlreadyInstalled(String),
    Message(String),
}

impl fmt::Display for PkgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Json(e) => write!(f, "json error: {e}"),
            Self::NotFound(s) => write!(f, "not found: {s}"),
            Self::AlreadyInstalled(s) => write!(f, "already installed: {s}"),
            Self::Message(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for PkgError {}

impl From<std::io::Error> for PkgError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<String> for PkgError {
    fn from(e: String) -> Self {
        Self::Json(e)
    }
}

pub type PkgResult<T> = Result<T, PkgError>;
