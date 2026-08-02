//! Error types for hyperparameter search.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuneError {
    EmptySpace,
    InvalidSpace(String),
    InvalidConfig(String),
    InvalidSplit(String),
    NoTrials,
}

impl TuneError {
    pub fn message(&self) -> String {
        match self {
            Self::EmptySpace => "search space is empty".into(),
            Self::InvalidSpace(msg) => msg.clone(),
            Self::InvalidConfig(msg) => msg.clone(),
            Self::InvalidSplit(msg) => msg.clone(),
            Self::NoTrials => "no trials to evaluate".into(),
        }
    }
}

pub type TuneResult<T> = Result<T, TuneError>;
