use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TomlError {
    pub line: usize,
    pub col: usize,
    pub message: String,
}

impl TomlError {
    pub fn new(line: usize, col: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            col,
            message: message.into(),
        }
    }
}

impl fmt::Display for TomlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TOML parse error at {}:{}: {}", self.line, self.col, self.message)
    }
}

impl std::error::Error for TomlError {}

pub type TomlResult<T> = Result<T, TomlError>;
