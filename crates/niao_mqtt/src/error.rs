//! MQTT client and codec errors.

use std::fmt;

/// Result alias for MQTT operations.
pub type MqttResult<T> = Result<T, MqttError>;

/// Errors raised by the MQTT codec or client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MqttError {
    Io(String),
    Tls(String),
    Protocol(String),
    Connack(u8, String),
    NotConnected,
    InvalidTopic(String),
    InvalidArgument(String),
}

impl fmt::Display for MqttError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(m) => write!(f, "io: {m}"),
            Self::Tls(m) => write!(f, "tls: {m}"),
            Self::Protocol(m) => write!(f, "protocol: {m}"),
            Self::Connack(code, m) => write!(f, "connack {code}: {m}"),
            Self::NotConnected => write!(f, "not connected"),
            Self::InvalidTopic(m) => write!(f, "invalid topic: {m}"),
            Self::InvalidArgument(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for MqttError {}

impl From<std::io::Error> for MqttError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}
