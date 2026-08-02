//! Error type for `niao_ssh`. Mapped to Niao `nssh_error` at the VM boundary.

/// Result alias for SSH operations.
pub type SshResult<T> = Result<T, SshError>;

/// Catchable SSH / auth / IO failure.
#[derive(Debug, thiserror::Error)]
pub enum SshError {
    #[error("{0}")]
    Message(String),
    #[error("invalid or closed handle {0}")]
    InvalidHandle(i64),
    #[error("authentication failed")]
    AuthFailed,
    #[error("connection refused or unreachable: {0}")]
    Connect(String),
    #[error("I/O error: {0}")]
    Io(String),
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("timeout")]
    Timeout,
    #[error("agent unavailable: {0}")]
    Agent(String),
}

impl SshError {
    pub fn msg(s: impl Into<String>) -> Self {
        SshError::Message(s.into())
    }

    pub fn from_russh(e: russh::Error) -> Self {
        SshError::Protocol(e.to_string())
    }
}

impl From<std::io::Error> for SshError {
    fn from(e: std::io::Error) -> Self {
        SshError::Io(e.to_string())
    }
}

impl From<russh::Error> for SshError {
    fn from(e: russh::Error) -> Self {
        SshError::from_russh(e)
    }
}

impl From<russh::keys::Error> for SshError {
    fn from(e: russh::keys::Error) -> Self {
        SshError::msg(e.to_string())
    }
}

impl From<russh_sftp::client::error::Error> for SshError {
    fn from(e: russh_sftp::client::error::Error) -> Self {
        SshError::Protocol(e.to_string())
    }
}
