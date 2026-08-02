//! Error type for the niao_grpc core (Niao bindings map these to E-codes).

use std::fmt;

/// Recoverable gRPC / transport failure.
#[derive(Debug, Clone)]
pub struct GrpcError {
    pub message: String,
}

impl GrpcError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for GrpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for GrpcError {}

impl From<std::io::Error> for GrpcError {
    fn from(e: std::io::Error) -> Self {
        Self::new(e.to_string())
    }
}

impl From<h2::Error> for GrpcError {
    fn from(e: h2::Error) -> Self {
        Self::new(e.to_string())
    }
}

impl From<http::Error> for GrpcError {
    fn from(e: http::Error) -> Self {
        Self::new(e.to_string())
    }
}

pub type GrpcResult<T> = Result<T, GrpcError>;
