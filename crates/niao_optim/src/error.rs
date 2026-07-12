//! Typed errors for noptim (codes 4030–4039).

use std::fmt;

pub const E4030_NOPTIM_ARITY: u32 = 4030;
pub const E4031_NOPTIM_ERROR: u32 = 4031;
pub const E4032_NOPTIM_TYPE: u32 = 4032;
pub const E4033_NOPTIM_NON_CONVERGENCE: u32 = 4033;
pub const E4034_NOPTIM_BAD_BOUNDS: u32 = 4034;
pub const E4035_NOPTIM_INFEASIBLE: u32 = 4035;
pub const E4036_NOPTIM_UNBOUNDED: u32 = 4036;

#[derive(Debug, Clone, PartialEq)]
pub enum OptimError {
    Arity { expected: usize, got: usize },
    Error(String),
    Type(String),
    NonConvergence(String),
    BadBounds(String),
    Infeasible(String),
    Unbounded(String),
}

impl OptimError {
    pub fn code(&self) -> u32 {
        match self {
            Self::Arity { .. } => E4030_NOPTIM_ARITY,
            Self::Error(_) => E4031_NOPTIM_ERROR,
            Self::Type(_) => E4032_NOPTIM_TYPE,
            Self::NonConvergence(_) => E4033_NOPTIM_NON_CONVERGENCE,
            Self::BadBounds(_) => E4034_NOPTIM_BAD_BOUNDS,
            Self::Infeasible(_) => E4035_NOPTIM_INFEASIBLE,
            Self::Unbounded(_) => E4036_NOPTIM_UNBOUNDED,
        }
    }
}

impl fmt::Display for OptimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arity { expected, got } => {
                write!(f, "expected {expected} argument(s), got {got}")
            }
            Self::Error(msg)
            | Self::Type(msg)
            | Self::NonConvergence(msg)
            | Self::BadBounds(msg)
            | Self::Infeasible(msg)
            | Self::Unbounded(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for OptimError {}

pub type OptimResult<T> = Result<T, OptimError>;
