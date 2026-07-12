//! Typed errors for nlearn (codes 4050–4059).

use std::fmt;

pub const E4050_NLEARN_ARITY: u32 = 4050;
pub const E4051_NLEARN_ERROR: u32 = 4051;
pub const E4052_NLEARN_TYPE: u32 = 4052;
pub const E4053_NLEARN_NOT_FITTED: u32 = 4053;
pub const E4054_NLEARN_SHAPE: u32 = 4054;
pub const E4055_NLEARN_NON_CONVERGENCE: u32 = 4055;

#[derive(Debug, Clone, PartialEq)]
pub enum LearnError {
    Arity { expected: usize, got: usize },
    Error(String),
    Type(String),
    NotFitted(String),
    Shape(String),
    NonConvergence(String),
}

impl LearnError {
    pub fn code(&self) -> u32 {
        match self {
            Self::Arity { .. } => E4050_NLEARN_ARITY,
            Self::Error(_) => E4051_NLEARN_ERROR,
            Self::Type(_) => E4052_NLEARN_TYPE,
            Self::NotFitted(_) => E4053_NLEARN_NOT_FITTED,
            Self::Shape(_) => E4054_NLEARN_SHAPE,
            Self::NonConvergence(_) => E4055_NLEARN_NON_CONVERGENCE,
        }
    }
}

impl fmt::Display for LearnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arity { expected, got } => {
                write!(f, "expected {expected} argument(s), got {got}")
            }
            Self::Error(msg)
            | Self::Type(msg)
            | Self::NotFitted(msg)
            | Self::Shape(msg)
            | Self::NonConvergence(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for LearnError {}

pub type LearnResult<T> = Result<T, LearnError>;
