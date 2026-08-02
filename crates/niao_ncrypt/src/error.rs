use std::fmt;

#[derive(Debug)]
pub enum NcryptError {
    InvalidArgument(String),
    InvalidKey(String),
    InvalidLength { expected: usize, got: usize },
    EncryptFailed(String),
    DecryptFailed(String),
    SignFailed(String),
    VerifyFailed(String),
    ParseFailed(String),
    Unsupported(String),
}

impl NcryptError {
    pub fn message(&self) -> String {
        match self {
            Self::InvalidArgument(m) => m.clone(),
            Self::InvalidKey(m) => m.clone(),
            Self::InvalidLength { expected, got } => {
                format!("expected {expected} bytes, got {got}")
            }
            Self::EncryptFailed(m) => m.clone(),
            Self::DecryptFailed(m) => m.clone(),
            Self::SignFailed(m) => m.clone(),
            Self::VerifyFailed(m) => m.clone(),
            Self::ParseFailed(m) => m.clone(),
            Self::Unsupported(m) => m.clone(),
        }
    }
}

impl fmt::Display for NcryptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for NcryptError {}

pub type NcryptResult<T> = Result<T, NcryptError>;

pub const MAX_BYTES: usize = 256 * 1024 * 1024;
pub const MAX_TOKEN_BYTES: usize = 1024 * 1024;
pub const NONCE_LEN: usize = 12;
pub const TAG_LEN: usize = 16;

pub fn check_len(len: usize) -> NcryptResult<()> {
    if len > MAX_BYTES {
        return Err(NcryptError::InvalidArgument(format!(
            "buffer exceeds MAX_BYTES ({MAX_BYTES})"
        )));
    }
    Ok(())
}

pub fn check_token_len(n: usize) -> NcryptResult<()> {
    if n == 0 || n > MAX_TOKEN_BYTES {
        return Err(NcryptError::InvalidArgument(format!(
            "token length must be 1..={MAX_TOKEN_BYTES}"
        )));
    }
    Ok(())
}
