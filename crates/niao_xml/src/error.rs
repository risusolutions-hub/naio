//! XML parse/emit/xpath errors.

/// Maximum XML input size (64 MiB guard).
pub const MAX_BYTES: usize = 64 * 1024 * 1024;

/// Maximum nodes per document (DoS guard).
pub const MAX_NODES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XmlError {
    Parse {
        line: u32,
        col: u32,
        message: String,
    },
    Emit(String),
    XPath(String),
    InvalidHandle(i64),
    InvalidNode(String),
    TooLarge(usize),
    TooManyNodes,
    Io(String),
}

impl XmlError {
    pub fn message(&self) -> String {
        match self {
            Self::Parse { line, col, message } => {
                format!("XML parse error at {line}:{col}: {message}")
            }
            Self::Emit(m) => m.clone(),
            Self::XPath(m) => format!("XPath error: {m}"),
            Self::InvalidHandle(id) => format!("invalid or closed nxml handle {id}"),
            Self::InvalidNode(m) => m.clone(),
            Self::TooLarge(n) => format!("XML size {n} exceeds limit {MAX_BYTES}"),
            Self::TooManyNodes => format!("document exceeds node limit {MAX_NODES}"),
            Self::Io(m) => m.clone(),
        }
    }

    pub fn parse(line: u32, col: u32, message: impl Into<String>) -> Self {
        Self::Parse {
            line,
            col,
            message: message.into(),
        }
    }
}
