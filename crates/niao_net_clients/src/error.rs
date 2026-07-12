#[derive(Debug)]
pub enum NetClientError {
    Message(String),
    Io(std::io::Error),
    Protocol(String),
    UnexpectedReply { expected: u16, got: u16 },
    TlsUnsupported,
}

impl std::fmt::Display for NetClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetClientError::Message(s) => write!(f, "{s}"),
            NetClientError::Io(e) => write!(f, "{e}"),
            NetClientError::Protocol(s) => write!(f, "protocol error: {s}"),
            NetClientError::UnexpectedReply { expected, got } => {
                write!(f, "expected FTP reply {expected}, got {got}")
            }
            NetClientError::TlsUnsupported => write!(f, "FTPS/TLS is not enabled in this build"),
        }
    }
}

impl std::error::Error for NetClientError {}

impl From<std::io::Error> for NetClientError {
    fn from(e: std::io::Error) -> Self {
        NetClientError::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, NetClientError>;
