//! Typed errors for nframe (codes 4010–4019).

use std::fmt;

pub const E4010_NFRAME_ARITY: u32 = 4010;
pub const E4011_NFRAME_ERROR: u32 = 4011;
pub const E4012_NFRAME_TYPE: u32 = 4012;
pub const E4013_NFRAME_BAD_COLUMN: u32 = 4013;
pub const E4014_NFRAME_LENGTH: u32 = 4014;
pub const E4015_NFRAME_DTYPE: u32 = 4015;

#[derive(Debug, Clone, PartialEq)]
pub enum FrameError {
    Arity { expected: usize, got: usize },
    Error(String),
    Type(String),
    BadColumn(String),
    LengthMismatch(String),
    Dtype(String),
}

impl FrameError {
    pub fn code(&self) -> u32 {
        match self {
            Self::Arity { .. } => E4010_NFRAME_ARITY,
            Self::Error(_) => E4011_NFRAME_ERROR,
            Self::Type(_) => E4012_NFRAME_TYPE,
            Self::BadColumn(_) => E4013_NFRAME_BAD_COLUMN,
            Self::LengthMismatch(_) => E4014_NFRAME_LENGTH,
            Self::Dtype(_) => E4015_NFRAME_DTYPE,
        }
    }
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arity { expected, got } => {
                write!(f, "expected {expected} argument(s), got {got}")
            }
            Self::Error(msg)
            | Self::Type(msg)
            | Self::BadColumn(msg)
            | Self::LengthMismatch(msg)
            | Self::Dtype(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for FrameError {}

pub type FrameResult<T> = Result<T, FrameError>;
