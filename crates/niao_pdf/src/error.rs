use std::fmt;

#[derive(Debug)]
pub enum PdfError {
    Io(std::io::Error),
    Lopdf(String),
    Extract(String),
    Build(String),
    InvalidHandle,
    InvalidPage(usize),
    InvalidInput(String),
}

impl PdfError {
    pub fn message(&self) -> String {
        match self {
            Self::Io(e) => e.to_string(),
            Self::Lopdf(s) | Self::Extract(s) | Self::Build(s) | Self::InvalidInput(s) => s.clone(),
            Self::InvalidHandle => "invalid PDF handle".into(),
            Self::InvalidPage(p) => format!("invalid page index {p}"),
        }
    }
}

impl fmt::Display for PdfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for PdfError {}

impl From<std::io::Error> for PdfError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<lopdf::Error> for PdfError {
    fn from(value: lopdf::Error) -> Self {
        Self::Lopdf(value.to_string())
    }
}

pub type PdfResult<T> = Result<T, PdfError>;
