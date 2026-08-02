//! Error types for `niao_graphql`.

use std::fmt;

/// Result alias for GraphQL core operations.
pub type GqlResult<T> = Result<T, GqlError>;

/// GraphQL parse / validate / execute error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GqlError {
    pub message: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

impl GqlError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            line: None,
            column: None,
        }
    }

    pub fn at(message: impl Into<String>, line: usize, column: usize) -> Self {
        Self {
            message: message.into(),
            line: Some(line),
            column: Some(column),
        }
    }

    pub fn parse(message: impl Into<String>, line: usize, column: usize) -> Self {
        Self::at(format!("parse error: {}", message.into()), line, column)
    }
}

impl fmt::Display for GqlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.line, self.column) {
            (Some(l), Some(c)) => write!(f, "{} (line {}, column {})", self.message, l, c),
            _ => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for GqlError {}
