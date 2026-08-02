use std::fmt;

/// Errors from protobuf compile, encode, decode, and schema operations.
#[derive(Debug)]
pub enum ProtoError {
    Compile(String),
    Parse(String),
    Encode(String),
    Decode(String),
    Schema(String),
    Type(String),
    Json(String),
    Io(String),
}

impl fmt::Display for ProtoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Compile(m) => write!(f, "compile error: {m}"),
            Self::Parse(m) => write!(f, "parse error: {m}"),
            Self::Encode(m) => write!(f, "encode error: {m}"),
            Self::Decode(m) => write!(f, "decode error: {m}"),
            Self::Schema(m) => write!(f, "schema error: {m}"),
            Self::Type(m) => write!(f, "type error: {m}"),
            Self::Json(m) => write!(f, "json error: {m}"),
            Self::Io(m) => write!(f, "io error: {m}"),
        }
    }
}

impl std::error::Error for ProtoError {}

pub type ProtoResult<T> = Result<T, ProtoError>;
