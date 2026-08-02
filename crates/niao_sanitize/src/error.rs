use std::fmt;

/// Sanitization or policy error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SanitizeError {
    message: String,
}

impl SanitizeError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for SanitizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SanitizeError {}
