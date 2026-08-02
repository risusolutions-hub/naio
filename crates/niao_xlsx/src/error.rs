//! Error types for `niao_xlsx`.

use std::fmt;

#[derive(Debug)]
pub enum XlsxError {
    Io(String),
    Format(String),
    Shape(String),
    Sheet(String),
    Cell(String),
    Style(String),
    Handle(String),
    Limit(String),
}

pub type XlsxResult<T> = Result<T, XlsxError>;

impl XlsxError {
    pub fn message(&self) -> String {
        match self {
            Self::Io(m) => m.clone(),
            Self::Format(m) => m.clone(),
            Self::Shape(m) => m.clone(),
            Self::Sheet(m) => m.clone(),
            Self::Cell(m) => m.clone(),
            Self::Style(m) => m.clone(),
            Self::Handle(m) => m.clone(),
            Self::Limit(m) => m.clone(),
        }
    }
}

impl fmt::Display for XlsxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for XlsxError {}

impl From<std::io::Error> for XlsxError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}
