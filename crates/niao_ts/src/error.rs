//! Typed errors for nts (codes 4070–4079).

use std::fmt;

pub const E4070_NTS_ARITY: u32 = 4070;
pub const E4071_NTS_ERROR: u32 = 4071;
pub const E4072_NTS_TYPE: u32 = 4072;
pub const E4073_NTS_NOT_FITTED: u32 = 4073;
pub const E4074_NTS_NON_STATIONARY: u32 = 4074;
pub const E4075_NTS_NON_CONVERGENCE: u32 = 4075;
pub const E4076_NTS_DOMAIN: u32 = 4076;
pub const E4077_NTS_SHAPE: u32 = 4077;

#[derive(Debug, Clone, PartialEq)]
pub enum TsError {
    Arity { expected: usize, got: usize },
    Error(String),
    Type(String),
    NotFitted(String),
    NonStationary(String),
    NonConvergence(String),
    Domain(String),
    Shape(String),
}

impl TsError {
    pub fn code(&self) -> u32 {
        match self {
            Self::Arity { .. } => E4070_NTS_ARITY,
            Self::Error(_) => E4071_NTS_ERROR,
            Self::Type(_) => E4072_NTS_TYPE,
            Self::NotFitted(_) => E4073_NTS_NOT_FITTED,
            Self::NonStationary(_) => E4074_NTS_NON_STATIONARY,
            Self::NonConvergence(_) => E4075_NTS_NON_CONVERGENCE,
            Self::Domain(_) => E4076_NTS_DOMAIN,
            Self::Shape(_) => E4077_NTS_SHAPE,
        }
    }
}

impl fmt::Display for TsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arity { expected, got } => {
                write!(f, "expected {expected} argument(s), got {got}")
            }
            Self::Error(msg)
            | Self::Type(msg)
            | Self::NotFitted(msg)
            | Self::NonStationary(msg)
            | Self::NonConvergence(msg)
            | Self::Domain(msg)
            | Self::Shape(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for TsError {}

pub type TsResult<T> = Result<T, TsError>;
