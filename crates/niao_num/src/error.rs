//! Typed errors for nnum (codes 4000–4009).

use std::fmt;

pub const E4000_NNUM_ARITY: u32 = 4000;
pub const E4001_NNUM_ERROR: u32 = 4001;
pub const E4002_NNUM_TYPE: u32 = 4002;
pub const E4003_NNUM_SHAPE: u32 = 4003;
pub const E4004_NNUM_SINGULAR: u32 = 4004;
pub const E4005_NNUM_NON_CONVERGENCE: u32 = 4005;

#[derive(Debug, Clone, PartialEq)]
pub enum NumError {
    Arity { expected: usize, got: usize },
    Error(String),
    Type(String),
    ShapeMismatch(String),
    Singular(String),
    NonConvergence(String),
}

impl NumError {
    pub fn code(&self) -> u32 {
        match self {
            Self::Arity { .. } => E4000_NNUM_ARITY,
            Self::Error(_) => E4001_NNUM_ERROR,
            Self::Type(_) => E4002_NNUM_TYPE,
            Self::ShapeMismatch(_) => E4003_NNUM_SHAPE,
            Self::Singular(_) => E4004_NNUM_SINGULAR,
            Self::NonConvergence(_) => E4005_NNUM_NON_CONVERGENCE,
        }
    }
}

impl fmt::Display for NumError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arity { expected, got } => {
                write!(f, "expected {expected} argument(s), got {got}")
            }
            Self::Error(msg)
            | Self::Type(msg)
            | Self::ShapeMismatch(msg)
            | Self::Singular(msg)
            | Self::NonConvergence(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for NumError {}

pub type NumResult<T> = Result<T, NumError>;
