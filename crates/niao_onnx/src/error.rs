use thiserror::Error;

#[derive(Debug, Error)]
pub enum OnnxError {
    #[error("empty input")]
    Empty,
    #[error("invalid path: {0}")]
    Path(String),
    #[error("invalid handle")]
    InvalidHandle,
    #[error("missing input: {0}")]
    MissingInput(String),
    #[error("unknown input: {0}")]
    UnknownInput(String),
    #[error("unknown output: {0}")]
    UnknownOutput(String),
    #[error("shape mismatch for {name}: expected {expected}, got {got}")]
    ShapeMismatch {
        name: String,
        expected: String,
        got: String,
    },
    #[error("dtype mismatch for {name}: expected {expected}, got {got}")]
    DtypeMismatch {
        name: String,
        expected: String,
        got: String,
    },
    #[error("tensor size mismatch for {name}: expected {expected} elements, got {got}")]
    SizeMismatch {
        name: String,
        expected: usize,
        got: usize,
    },
    #[error("invalid parameter: {0}")]
    Param(String),
    #[error("inference engine: {0}")]
    Engine(String),
}

pub type OnnxResult<T> = Result<T, OnnxError>;

impl OnnxError {
    pub fn message(&self) -> String {
        self.to_string()
    }
}
