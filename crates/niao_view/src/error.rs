//! Domain errors for nview (mapped to Niao E-codes at the VM boundary).

use std::fmt;

/// Result alias for core nview operations.
pub type ViewResult<T> = Result<T, ViewError>;

/// Error raised by the templating engine (parse, render, IO).
#[derive(Debug, Clone)]
pub struct ViewError {
    message: String,
    kind: ViewErrorKind,
}

/// Coarse classification used by bindings for E-code selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewErrorKind {
    /// Template syntax / compile failure.
    Parse,
    /// Render-time or semantic failure.
    Render,
    /// Filesystem / loader failure.
    Io,
    /// Invalid handle or bad argument at the core layer.
    Invalid,
}

impl ViewError {
    /// Construct a parse error.
    pub fn parse(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
            kind: ViewErrorKind::Parse,
        }
    }

    /// Construct a render error.
    pub fn render(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
            kind: ViewErrorKind::Render,
        }
    }

    /// Construct an IO error.
    pub fn io(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
            kind: ViewErrorKind::Io,
        }
    }

    /// Construct an invalid-argument error.
    pub fn invalid(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
            kind: ViewErrorKind::Invalid,
        }
    }

    /// Human-readable message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Error kind.
    pub fn kind(&self) -> ViewErrorKind {
        self.kind
    }
}

impl fmt::Display for ViewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ViewError {}

impl From<minijinja::Error> for ViewError {
    fn from(err: minijinja::Error) -> Self {
        let message = err.to_string();
        let kind = match err.kind() {
            minijinja::ErrorKind::SyntaxError | minijinja::ErrorKind::TemplateNotFound => {
                ViewErrorKind::Parse
            }
            _ => ViewErrorKind::Render,
        };
        Self { message, kind }
    }
}

impl From<std::io::Error> for ViewError {
    fn from(err: std::io::Error) -> Self {
        Self::io(err.to_string())
    }
}
