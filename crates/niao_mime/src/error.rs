//! Error types for `niao_mime`.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MimeError {
    EmptyInput,
    InvalidMime(String),
    InvalidExtension(String),
    InvalidMagic(String),
    Io(String),
    PathNotFound(String),
    TooLarge(usize),
    OffsetOutOfRange { offset: usize, len: usize },
}

impl MimeError {
    pub fn message(&self) -> String {
        match self {
            Self::EmptyInput => "empty input".into(),
            Self::InvalidMime(m) => format!("invalid MIME type: {m}"),
            Self::InvalidExtension(e) => format!("invalid extension: {e}"),
            Self::InvalidMagic(m) => format!("invalid magic bytes: {m}"),
            Self::Io(e) => format!("I/O error: {e}"),
            Self::PathNotFound(p) => format!("path not found: {p}"),
            Self::TooLarge(n) => format!("input size {n} exceeds sniff limit"),
            Self::OffsetOutOfRange { offset, len } => {
                format!("magic offset {offset} out of range for {len} byte buffer")
            }
        }
    }
}

pub type MimeResult<T> = Result<T, MimeError>;
