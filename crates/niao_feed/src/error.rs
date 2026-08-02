/// Errors from feed parse / emit / build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedError {
    Empty,
    TooLarge(usize),
    Parse(String),
    Emit(String),
    InvalidField(String),
    InvalidDate(String),
    UnknownFormat(String),
    Io(String),
}

impl FeedError {
    pub fn message(&self) -> String {
        match self {
            Self::Empty => "empty input".into(),
            Self::TooLarge(n) => format!("input size {n} exceeds limit {MAX_BYTES}"),
            Self::Parse(m) => m.clone(),
            Self::Emit(m) => m.clone(),
            Self::InvalidField(m) => m.clone(),
            Self::InvalidDate(m) => m.clone(),
            Self::UnknownFormat(m) => m.clone(),
            Self::Io(m) => m.clone(),
        }
    }
}

pub type FeedResult<T> = Result<T, FeedError>;

/// Maximum feed document size (16 MiB).
pub const MAX_BYTES: usize = 16 * 1024 * 1024;

pub fn check_len(len: usize) -> FeedResult<()> {
    if len == 0 {
        return Err(FeedError::Empty);
    }
    if len > MAX_BYTES {
        return Err(FeedError::TooLarge(len));
    }
    Ok(())
}
