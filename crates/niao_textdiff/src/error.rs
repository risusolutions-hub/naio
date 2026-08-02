use std::fmt;

#[derive(Debug, Clone)]
pub struct TextDiffError {
    message: String,
}

impl TextDiffError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for TextDiffError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for TextDiffError {}
