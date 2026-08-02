//! Library-level errors (mapped to Niao `nopenapi_error` at the VM boundary).

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenApiError {
    pub message: String,
}

impl OpenApiError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for OpenApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for OpenApiError {}

pub type OpenApiResult<T> = Result<T, OpenApiError>;
