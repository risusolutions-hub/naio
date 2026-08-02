//! Search client errors (domain failures — never panics).

use std::fmt;

#[derive(Debug, Clone)]
pub enum SearchError {
    Url(String),
    Http(String),
    Status { status: u16, body: String },
    Json(String),
    Config(String),
    Protocol(String),
}

impl fmt::Display for SearchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Url(e) => write!(f, "url: {e}"),
            Self::Http(e) => write!(f, "http: {e}"),
            Self::Status { status, body } => {
                let snippet: String = body.chars().take(240).collect();
                write!(f, "status {status}: {snippet}")
            }
            Self::Json(e) => write!(f, "json: {e}"),
            Self::Config(e) => write!(f, "config: {e}"),
            Self::Protocol(e) => write!(f, "protocol: {e}"),
        }
    }
}

impl std::error::Error for SearchError {}

pub type SearchResult<T> = Result<T, SearchError>;
