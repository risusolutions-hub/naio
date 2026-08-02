use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum FinError {
    Empty,
    Length(String),
    Param(String),
    Domain(String),
    NonConvergence(String),
}

impl FinError {
    pub fn message(&self) -> String {
        match self {
            Self::Empty => "empty input".into(),
            Self::Length(m) | Self::Param(m) | Self::Domain(m) | Self::NonConvergence(m) => {
                m.clone()
            }
        }
    }
}

impl fmt::Display for FinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for FinError {}

pub type FinResult<T> = Result<T, FinError>;
