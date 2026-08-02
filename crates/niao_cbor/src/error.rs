use std::fmt;

/// CBOR encode/decode failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CborError {
    TooLarge(usize),
    DepthExceeded { max: usize },
    TrailingData { offset: usize },
    DuplicateKey(String),
    InvalidTag { tag: u64, reason: String },
    InvalidDatetime(String),
    InvalidDecimal(String),
    InvalidUuid(String),
    InvalidSimple(u8),
    Decode(String),
    Encode(String),
    Io(String),
}

impl CborError {
    pub fn message(&self) -> String {
        match self {
            Self::TooLarge(n) => format!("data size {n} exceeds limit {}", crate::MAX_BYTES),
            Self::DepthExceeded { max } => format!("nesting depth exceeds limit {max}"),
            Self::TrailingData { offset } => format!("trailing bytes at offset {offset}"),
            Self::DuplicateKey(k) => format!("duplicate map key {k:?}"),
            Self::InvalidTag { tag, reason } => format!("invalid CBOR tag {tag}: {reason}"),
            Self::InvalidDatetime(s) => format!("invalid datetime tag payload: {s}"),
            Self::InvalidDecimal(s) => format!("invalid decimal fraction tag: {s}"),
            Self::InvalidUuid(s) => format!("invalid UUID tag: {s}"),
            Self::InvalidSimple(n) => format!("invalid CBOR simple value {n}"),
            Self::Decode(m) => m.clone(),
            Self::Encode(m) => m.clone(),
            Self::Io(m) => m.clone(),
        }
    }
}

impl fmt::Display for CborError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for CborError {}

pub type CborResult<T> = Result<T, CborError>;
