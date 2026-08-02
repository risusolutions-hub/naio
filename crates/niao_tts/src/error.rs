use thiserror::Error;

#[derive(Debug, Error)]
pub enum TtsError {
    #[error("empty input")]
    Empty,
    #[error("invalid handle")]
    InvalidHandle,
    #[error("invalid parameter: {0}")]
    Param(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("model error: {0}")]
    Model(String),
    #[error("synthesis error: {0}")]
    Synth(String),
    #[error("audio error: {0}")]
    Audio(String),
    #[error("unknown property: {0}")]
    Property(String),
}

pub type TtsResult<T> = Result<T, TtsError>;

impl TtsError {
    pub fn message(&self) -> String {
        self.to_string()
    }
}
