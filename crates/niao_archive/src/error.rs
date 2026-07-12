#[derive(Debug)]
pub enum Error {
    Message(String),
    Io(std::io::Error),
    CrcMismatch,
    Unsupported(String),
    Truncated,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Message(s) => write!(f, "{s}"),
            Error::Io(e) => write!(f, "{e}"),
            Error::CrcMismatch => write!(f, "checksum mismatch"),
            Error::Unsupported(s) => write!(f, "unsupported: {s}"),
            Error::Truncated => write!(f, "truncated input"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
