use std::fmt;

#[derive(Debug)]
pub enum TarError {
    Io(std::io::Error),
    Format(String),
    NotFound(String),
    UnsafePath(String),
    InvalidMode(String),
    Closed,
}

pub type Result<T> = std::result::Result<T, TarError>;

impl fmt::Display for TarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Format(msg) => write!(f, "{msg}"),
            Self::NotFound(name) => write!(f, "tar member not found: {name}"),
            Self::UnsafePath(path) => write!(f, "unsafe tar path: {path}"),
            Self::InvalidMode(msg) => write!(f, "{msg}"),
            Self::Closed => write!(f, "tar archive handle is closed"),
        }
    }
}

impl std::error::Error for TarError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for TarError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}
