use std::fmt;

/// Parse failure for natural-language date strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WhenError {
    Empty,
    NoDate,
    InvalidDate(String),
    InvalidTime(String),
    Ambiguous(String),
    Unsupported(String),
}

impl WhenError {
    pub fn message(&self) -> String {
        match self {
            Self::Empty => "empty input".into(),
            Self::NoDate => "could not parse date or time".into(),
            Self::InvalidDate(m) => m.clone(),
            Self::InvalidTime(m) => m.clone(),
            Self::Ambiguous(m) => m.clone(),
            Self::Unsupported(m) => m.clone(),
        }
    }
}

impl fmt::Display for WhenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for WhenError {}
