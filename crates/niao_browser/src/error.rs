//! Error type for `niao_browser`. Mapped to Niao `nbrowser_error` at the VM boundary.

/// Result alias for browser operations.
pub type BrowserResult<T> = Result<T, BrowserError>;

/// Catchable browser / CDP / IO failure.
#[derive(Debug, thiserror::Error)]
pub enum BrowserError {
    #[error("{0}")]
    Message(String),
    #[error("invalid or closed handle {0}")]
    InvalidHandle(i64),
    #[error("browser not found: {0}")]
    ExecutableNotFound(String),
    #[error("connection refused or unreachable: {0}")]
    Connect(String),
    #[error("timeout: {0}")]
    Timeout(String),
    #[error("selector not found: {0}")]
    SelectorNotFound(String),
    #[error("navigation failed: {0}")]
    Navigation(String),
    #[error("CDP / protocol error: {0}")]
    Protocol(String),
    #[error("I/O error: {0}")]
    Io(String),
}

impl BrowserError {
    pub fn msg(s: impl Into<String>) -> Self {
        BrowserError::Message(s.into())
    }
}

impl From<std::io::Error> for BrowserError {
    fn from(e: std::io::Error) -> Self {
        BrowserError::Io(e.to_string())
    }
}
