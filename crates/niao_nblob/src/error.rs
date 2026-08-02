//! Error type for `niao_nblob` (mapped to Niao E-codes at the VM boundary).

use std::fmt;

/// Library-level error; the runtime maps this to `nblob_error` / E4551.
#[derive(Debug, Clone)]
pub struct BlobError {
    pub message: String,
}

impl BlobError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }

    pub fn not_found(path: &str) -> Self {
        Self::new(format!("not found: {path}"))
    }

    pub fn invalid_uri(uri: &str) -> Self {
        Self::new(format!("invalid URI: {uri}"))
    }

    pub fn unsupported(scheme: &str) -> Self {
        Self::new(format!("unsupported scheme: {scheme}"))
    }

    pub fn io(msg: impl Into<String>) -> Self {
        Self::new(msg)
    }

    pub fn auth(msg: impl Into<String>) -> Self {
        Self::new(format!("auth: {}", msg.into()))
    }

    pub fn http(status: u16, body: &str) -> Self {
        let snippet: String = body.chars().take(512).collect();
        Self::new(format!("HTTP {status}: {snippet}"))
    }
}

impl fmt::Display for BlobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for BlobError {}

impl From<std::io::Error> for BlobError {
    fn from(e: std::io::Error) -> Self {
        Self::io(e.to_string())
    }
}

impl From<niao_http::Error> for BlobError {
    fn from(e: niao_http::Error) -> Self {
        Self::io(e.to_string())
    }
}

pub type BlobResult<T> = Result<T, BlobError>;
