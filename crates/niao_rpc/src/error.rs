//! Error types for the JSON-RPC engine.

use std::fmt;

/// Standard JSON-RPC 2.0 error codes.
pub mod codes {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const INTERNAL_ERROR: i64 = -32603;
    /// Start of the implementation-defined server-error range.
    pub const SERVER_ERROR_START: i64 = -32099;
    /// End of the implementation-defined server-error range.
    pub const SERVER_ERROR_END: i64 = -32000;
}

/// A JSON-RPC application / protocol error payload.
#[derive(Clone, Debug, PartialEq)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<niao_json_core::Value>,
}

impl RpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(mut self, data: niao_json_core::Value) -> Self {
        self.data = Some(data);
        self
    }

    pub fn parse_error(message: impl Into<String>) -> Self {
        Self::new(codes::PARSE_ERROR, message)
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(codes::INVALID_REQUEST, message)
    }

    pub fn method_not_found(method: &str) -> Self {
        Self::new(
            codes::METHOD_NOT_FOUND,
            format!("Method not found: {method}"),
        )
    }

    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(codes::INVALID_PARAMS, message)
    }

    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::new(codes::INTERNAL_ERROR, message)
    }

    pub fn default_message(code: i64) -> &'static str {
        match code {
            codes::PARSE_ERROR => "Parse error",
            codes::INVALID_REQUEST => "Invalid Request",
            codes::METHOD_NOT_FOUND => "Method not found",
            codes::INVALID_PARAMS => "Invalid params",
            codes::INTERNAL_ERROR => "Internal error",
            c if (codes::SERVER_ERROR_START..=codes::SERVER_ERROR_END).contains(&c) => {
                "Server error"
            }
            _ => "Error",
        }
    }
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "rpc error {}: {}", self.code, self.message)
    }
}

impl std::error::Error for RpcError {}

/// Engine-level failures (I/O, framing, codec) distinct from JSON-RPC error objects.
#[derive(Clone, Debug, PartialEq)]
pub enum EngineError {
    Parse(String),
    Invalid(String),
    Io(String),
    Framing(String),
    Transport(String),
    Limit(String),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::Parse(m)
            | EngineError::Invalid(m)
            | EngineError::Io(m)
            | EngineError::Framing(m)
            | EngineError::Transport(m)
            | EngineError::Limit(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for EngineError {}

impl From<niao_json_core::ParseError> for EngineError {
    fn from(e: niao_json_core::ParseError) -> Self {
        EngineError::Parse(e.to_string())
    }
}
