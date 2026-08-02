use niao_bignum::BigInt;

/// Owned CBOR value preserving map insertion order.
#[derive(Debug, Clone, PartialEq)]
pub enum CborValue {
    Null,
    Undefined,
    Bool(bool),
    Int(i128),
    BigInt(BigInt),
    Float(f64),
    Bytes(Vec<u8>),
    String(String),
    Array(Vec<CborValue>),
    Map(Vec<(CborValue, CborValue)>),
    Tag(u64, Box<CborValue>),
    /// CBOR simple value (0–23 inline, 24–255 one-byte).
    Simple(u8),
}

impl CborValue {
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    pub fn tag_number(&self) -> Option<u64> {
        match self {
            Self::Tag(n, _) => Some(*n),
            _ => None,
        }
    }

    pub fn untagged(self) -> CborValue {
        match self {
            Self::Tag(_, inner) => *inner,
            other => other,
        }
    }
}

/// Sentinel keys used when bridging to Niao objects.
pub const NIAO_TAG_KEY: &str = "__tag";
pub const NIAO_VALUE_KEY: &str = "value";
pub const NIAO_SIMPLE_KEY: &str = "__simple";
pub const NIAO_UNDEFINED_KEY: &str = "__cbor_undefined";

pub fn bigint_from_tagged_bytes(tag: u64, bytes: &[u8]) -> Option<BigInt> {
    if bytes.is_empty() {
        return None;
    }
    let mag = BigInt::parse_bytes(bytes, 256)?;
    match tag {
        crate::tags::BIGNUM_POS => Some(mag),
        crate::tags::BIGNUM_NEG => Some(-BigInt::from(1) - mag),
        _ => None,
    }
}
