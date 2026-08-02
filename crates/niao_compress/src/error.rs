use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompressError {
    UnknownCodec(String),
    InvalidLevel { codec: String, level: i32 },
    TooLarge(usize),
    Corrupt(String),
    Io(String),
    SizeMismatch { expected: u64, actual: usize },
    Other(String),
}

impl CompressError {
    pub fn message(&self) -> String {
        match self {
            Self::UnknownCodec(c) => format!("unknown codec: {c}"),
            Self::InvalidLevel { codec, level } => {
                format!("invalid compression level {level} for {codec}")
            }
            Self::TooLarge(n) => format!("data size {n} exceeds limit {MAX_BYTES}"),
            Self::Corrupt(m) => format!("corrupt or invalid compressed data: {m}"),
            Self::Io(m) => format!("io error: {m}"),
            Self::SizeMismatch { expected, actual } => format!(
                "decompressed size {actual} does not match declared content size {expected}"
            ),
            Self::Other(m) => m.clone(),
        }
    }
}

impl fmt::Display for CompressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for CompressError {}

pub type CompressResult<T> = Result<T, CompressError>;

/// Maximum single-buffer input/output size (256 MiB guard).
pub const MAX_BYTES: usize = 256 * 1024 * 1024;

pub fn check_len(len: usize) -> CompressResult<()> {
    if len > MAX_BYTES {
        return Err(CompressError::TooLarge(len));
    }
    Ok(())
}
