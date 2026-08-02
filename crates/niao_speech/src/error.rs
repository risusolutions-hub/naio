use thiserror::Error;

#[derive(Debug, Error)]
pub enum SpeechError {
    #[error("empty input")]
    Empty,
    #[error("invalid handle")]
    InvalidHandle,
    #[error("invalid parameter: {0}")]
    Param(String),
    #[error("audio error: {0}")]
    Audio(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("model error: {0}")]
    Model(String),
    #[error("microphone error: {0}")]
    Mic(String),
    #[error("whisper error: {0}")]
    Whisper(String),
}

pub type SpeechResult<T> = Result<T, SpeechError>;

impl SpeechError {
    pub fn message(&self) -> String {
        self.to_string()
    }
}
