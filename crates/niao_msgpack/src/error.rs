use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MsgpackError {
    Encode(String),
    Decode(String),
    Type(String),
    TooLarge(usize),
    Incomplete,
    InvalidExt(i8),
    StrictMapKey(String),
}

impl MsgpackError {
    pub fn message(&self) -> String {
        match self {
            Self::Encode(m) => m.clone(),
            Self::Decode(m) => m.clone(),
            Self::Type(m) => m.clone(),
            Self::TooLarge(n) => format!("data size {n} exceeds limit {}", crate::MAX_BYTES),
            Self::Incomplete => "incomplete MessagePack data".into(),
            Self::InvalidExt(c) => format!("invalid extension type code {c}"),
            Self::StrictMapKey(k) => format!("non-string map key in strict mode: {k}"),
        }
    }
}

impl fmt::Display for MsgpackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for MsgpackError {}

impl From<rmp::encode::ValueWriteError> for MsgpackError {
    fn from(e: rmp::encode::ValueWriteError) -> Self {
        Self::Encode(e.to_string())
    }
}

impl From<rmpv::decode::Error> for MsgpackError {
    fn from(e: rmpv::decode::Error) -> Self {
        match e {
            rmpv::decode::Error::InvalidDataRead(e)
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                Self::Incomplete
            }
            other => Self::Decode(other.to_string()),
        }
    }
}
