use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DspError {
    Empty,
    Length(String),
    Param(String),
    Filter(String),
    Domain(String),
}

impl DspError {
    pub fn message(&self) -> String {
        match self {
            Self::Empty => "empty input".into(),
            Self::Length(m) | Self::Param(m) | Self::Filter(m) | Self::Domain(m) => m.clone(),
        }
    }
}

impl fmt::Display for DspError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for DspError {}

pub type DspResult<T> = Result<T, DspError>;
