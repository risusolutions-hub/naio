use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsError {
    Incomplete,
    Io(String),
    Handshake(String),
    Protocol(String),
    Tls(String),
    Utf8,
}

impl fmt::Display for WsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Incomplete => write!(f, "incomplete frame"),
            Self::Io(e) => write!(f, "{e}"),
            Self::Handshake(e) => write!(f, "handshake: {e}"),
            Self::Protocol(e) => write!(f, "protocol: {e}"),
            Self::Tls(e) => write!(f, "tls: {e}"),
            Self::Utf8 => write!(f, "invalid utf8"),
        }
    }
}

impl std::error::Error for WsError {}
