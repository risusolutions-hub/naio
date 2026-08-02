//! Typed errors for nstats (codes 4020–4029).

use std::fmt;

pub const E4020_NSTATS_ARITY: u32 = 4020;
pub const E4021_NSTATS_ERROR: u32 = 4021;
pub const E4022_NSTATS_TYPE: u32 = 4022;
pub const E4023_NSTATS_DOMAIN: u32 = 4023;
pub const E4024_NSTATS_NON_CONVERGENCE: u32 = 4024;

#[derive(Debug, Clone, PartialEq)]
pub enum StatsError {
    Arity { expected: usize, got: usize },
    Error(String),
    Type(String),
    Domain(String),
    NonConvergence(String),
}

impl StatsError {
    pub fn code(&self) -> u32 {
        match self {
            Self::Arity { .. } => E4020_NSTATS_ARITY,
            Self::Error(_) => E4021_NSTATS_ERROR,
            Self::Type(_) => E4022_NSTATS_TYPE,
            Self::Domain(_) => E4023_NSTATS_DOMAIN,
            Self::NonConvergence(_) => E4024_NSTATS_NON_CONVERGENCE,
        }
    }
}

impl fmt::Display for StatsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arity { expected, got } => {
                write!(f, "expected {expected} argument(s), got {got}")
            }
            Self::Error(msg) | Self::Type(msg) | Self::Domain(msg) | Self::NonConvergence(msg) => {
                f.write_str(msg)
            }
        }
    }
}

impl std::error::Error for StatsError {}

pub type StatsResult<T> = Result<T, StatsError>;
