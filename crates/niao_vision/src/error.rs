//! Typed errors for nvision (codes 4090–4099).

use std::fmt;

pub const E4090_NVISION_ARITY: u32 = 4090;
pub const E4091_NVISION_ERROR: u32 = 4091;
pub const E4092_NVISION_TYPE: u32 = 4092;
pub const E4093_NVISION_CODEC: u32 = 4093;
pub const E4094_NVISION_SHAPE: u32 = 4094;
pub const E4095_NVISION_MISSING: u32 = 4095;

#[derive(Debug, Clone, PartialEq)]
pub enum VisionError {
    Arity { expected: usize, got: usize },
    Error(String),
    Type(String),
    Codec(String),
    Shape(String),
    MissingFile(String),
}

impl VisionError {
    pub fn code(&self) -> u32 {
        match self {
            Self::Arity { .. } => E4090_NVISION_ARITY,
            Self::Error(_) => E4091_NVISION_ERROR,
            Self::Type(_) => E4092_NVISION_TYPE,
            Self::Codec(_) => E4093_NVISION_CODEC,
            Self::Shape(_) => E4094_NVISION_SHAPE,
            Self::MissingFile(_) => E4095_NVISION_MISSING,
        }
    }
}

impl fmt::Display for VisionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arity { expected, got } => {
                write!(f, "expected {expected} argument(s), got {got}")
            }
            Self::Error(m)
            | Self::Type(m)
            | Self::Codec(m)
            | Self::Shape(m)
            | Self::MissingFile(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for VisionError {}

impl From<niao_tensor::TensorError> for VisionError {
    fn from(e: niao_tensor::TensorError) -> Self {
        VisionError::Shape(e.to_string())
    }
}

impl From<niao_num::NumError> for VisionError {
    fn from(e: niao_num::NumError) -> Self {
        VisionError::Error(e.to_string())
    }
}

pub type VisionResult<T> = Result<T, VisionError>;
