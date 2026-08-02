use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HtmlError {
    Parse(String),
    Selector(String),
    InvalidHandle(String),
    InvalidNode(String),
}

pub type HtmlResult<T> = Result<T, HtmlError>;

impl HtmlError {
    pub fn message(&self) -> String {
        match self {
            Self::Parse(s) | Self::Selector(s) | Self::InvalidHandle(s) | Self::InvalidNode(s) => {
                s.clone()
            }
        }
    }
}

impl fmt::Display for HtmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for HtmlError {}
