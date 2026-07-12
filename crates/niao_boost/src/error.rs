//! Typed errors for nboost (codes 4060–4069).

use std::fmt;

pub const E4060_NBOOST_ARITY: u32 = 4060;
pub const E4061_NBOOST_ERROR: u32 = 4061;
pub const E4062_NBOOST_TYPE: u32 = 4062;
pub const E4063_NBOOST_NOT_FITTED: u32 = 4063;
pub const E4064_NBOOST_BAD_PARAM: u32 = 4064;
pub const E4065_NBOOST_SHAPE: u32 = 4065;
pub const E4066_NBOOST_IO: u32 = 4066;
pub const E4067_NBOOST_NON_CONVERGENCE: u32 = 4067;

#[derive(Debug, Clone, PartialEq)]
pub enum BoostError {
    Arity { expected: usize, got: usize },
    Error(String),
    Type(String),
    NotFitted,
    BadParam(String),
    Shape(String),
    Io(String),
    NonConvergence(String),
}

impl BoostError {
    pub fn code(&self) -> u32 {
        match self {
            Self::Arity { .. } => E4060_NBOOST_ARITY,
            Self::Error(_) => E4061_NBOOST_ERROR,
            Self::Type(_) => E4062_NBOOST_TYPE,
            Self::NotFitted => E4063_NBOOST_NOT_FITTED,
            Self::BadParam(_) => E4064_NBOOST_BAD_PARAM,
            Self::Shape(_) => E4065_NBOOST_SHAPE,
            Self::Io(_) => E4066_NBOOST_IO,
            Self::NonConvergence(_) => E4067_NBOOST_NON_CONVERGENCE,
        }
    }
}

impl fmt::Display for BoostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arity { expected, got } => {
                write!(f, "expected {expected} argument(s), got {got}")
            }
            Self::NotFitted => f.write_str("model is not fitted"),
            Self::Error(msg)
            | Self::Type(msg)
            | Self::BadParam(msg)
            | Self::Shape(msg)
            | Self::Io(msg)
            | Self::NonConvergence(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for BoostError {}

pub type BoostResult<T> = Result<T, BoostError>;
