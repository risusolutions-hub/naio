//! Error types for nreq (mapped to Niao E445x at the VM boundary).

use std::fmt;

#[derive(Debug, Clone)]
pub enum ReqError {
    Url(String),
    Http(String),
    Timeout,
    TooManyRedirects,
    Io(String),
    Json(String),
    Status { status: u16, message: String },
    Proxy(String),
    Config(String),
}

impl fmt::Display for ReqError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Url(m) => write!(f, "{m}"),
            Self::Http(m) => write!(f, "{m}"),
            Self::Timeout => write!(f, "request timed out"),
            Self::TooManyRedirects => write!(f, "too many redirects"),
            Self::Io(m) => write!(f, "{m}"),
            Self::Json(m) => write!(f, "{m}"),
            Self::Status { status, message } => write!(f, "HTTP {status}: {message}"),
            Self::Proxy(m) => write!(f, "proxy: {m}"),
            Self::Config(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for ReqError {}

impl From<niao_http::Error> for ReqError {
    fn from(e: niao_http::Error) -> Self {
        match e {
            niao_http::Error::Url(m) => Self::Url(m),
            niao_http::Error::Timeout => Self::Timeout,
            niao_http::Error::TooManyRedirects => Self::TooManyRedirects,
            niao_http::Error::Io(m) => Self::Io(m),
            other => Self::Http(other.to_string()),
        }
    }
}

pub type ReqResult<T> = Result<T, ReqError>;
