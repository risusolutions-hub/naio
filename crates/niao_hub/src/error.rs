use thiserror::Error;

#[derive(Debug, Error)]
pub enum HubError {
    #[error("{0}")]
    Msg(String),
    #[error("hub: {0}")]
    Hf(#[from] hf_hub::api::sync::ApiError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("network: {0}")]
    Network(String),
    #[error("checksum mismatch: expected {expected}, got {actual}")]
    Checksum { expected: String, actual: String },
    #[error("invalid argument: {0}")]
    InvalidArg(String),
}

pub type HubResult<T> = Result<T, HubError>;
