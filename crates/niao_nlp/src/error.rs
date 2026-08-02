//! Typed errors for nnlp (codes 4080–4089).

use std::fmt;

pub const E4080_NNLP_ARITY: u32 = 4080;
pub const E4081_NNLP_ERROR: u32 = 4081;
pub const E4082_NNLP_TYPE: u32 = 4082;
pub const E4083_NNLP_NOT_FITTED: u32 = 4083;
pub const E4084_NNLP_EMPTY_VOCAB: u32 = 4084;
pub const E4085_NNLP_SHAPE: u32 = 4085;
pub const E4086_NNLP_OOV: u32 = 4086;

#[derive(Debug, Clone, PartialEq)]
pub enum NlpError {
    Arity { expected: usize, got: usize },
    Error(String),
    Type(String),
    NotFitted,
    EmptyVocab,
    Shape(String),
    Oov(String),
}

impl NlpError {
    pub fn code(&self) -> u32 {
        match self {
            Self::Arity { .. } => E4080_NNLP_ARITY,
            Self::Error(_) => E4081_NNLP_ERROR,
            Self::Type(_) => E4082_NNLP_TYPE,
            Self::NotFitted => E4083_NNLP_NOT_FITTED,
            Self::EmptyVocab => E4084_NNLP_EMPTY_VOCAB,
            Self::Shape(_) => E4085_NNLP_SHAPE,
            Self::Oov(_) => E4086_NNLP_OOV,
        }
    }
}

impl fmt::Display for NlpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arity { expected, got } => {
                write!(f, "expected {expected} argument(s), got {got}")
            }
            Self::Error(msg) | Self::Type(msg) | Self::Shape(msg) | Self::Oov(msg) => {
                f.write_str(msg)
            }
            Self::NotFitted => f.write_str("estimator is not fitted"),
            Self::EmptyVocab => f.write_str("vocabulary is empty after pruning"),
        }
    }
}

impl std::error::Error for NlpError {}

pub type NlpResult<T> = Result<T, NlpError>;
