use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JpathError {
    InvalidPointer(String),
    PointerNotFound(String),
    InvalidPatch(String),
    PatchFailed(String),
    InvalidJsonPath(String),
    InvalidJmesPath(String),
    TypeMismatch(String),
}

impl JpathError {
    pub fn message(&self) -> String {
        match self {
            Self::InvalidPointer(s) => format!("invalid JSON Pointer: {s}"),
            Self::PointerNotFound(s) => format!("pointer not found: {s}"),
            Self::InvalidPatch(s) => format!("invalid JSON Patch: {s}"),
            Self::PatchFailed(s) => format!("patch failed: {s}"),
            Self::InvalidJsonPath(s) => format!("invalid JSONPath: {s}"),
            Self::InvalidJmesPath(s) => format!("invalid JMESPath: {s}"),
            Self::TypeMismatch(s) => format!("type mismatch: {s}"),
        }
    }
}

impl fmt::Display for JpathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for JpathError {}

pub type JpathResult<T> = Result<T, JpathError>;
