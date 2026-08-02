//! CBOR encode/decode for Niao (`ncbor`).
//!
//! RFC 8949 CBOR with semantic tags, canonical encoding (COSE-friendly),
//! indefinite-length support, and tag hooks (~cbor2 subset).
//!
//! Backed by [`minicbor`] for zero-copy decode and compact encoding.

mod decode;
mod encode;
mod error;
mod tags;
mod value;

pub use decode::{decode, decode_all, is_valid};
pub use encode::{encode, encode_into};
pub use error::{CborError, CborResult};
pub use tags::{
    BASE64, BASE64URL, BIGFLOAT, BIGNUM_NEG, BIGNUM_POS, DATETIME_EPOCH, DATETIME_STRING,
    DECIMAL_FRACTION, ENCODED_CBOR, EXPECTED_BASE16, EXPECTED_BASE64, EXPECTED_BASE64URL, KNOWN,
    MIME, REGEX, SELF_DESCRIBE, URI, UUID,
};
pub use value::{
    bigint_from_tagged_bytes, CborValue, NIAO_SIMPLE_KEY, NIAO_TAG_KEY, NIAO_UNDEFINED_KEY,
    NIAO_VALUE_KEY,
};

/// Maximum input/output size (64 MiB guard).
pub const MAX_BYTES: usize = 64 * 1024 * 1024;

/// Default nesting depth limit.
pub const DEFAULT_MAX_DEPTH: usize = 512;

/// Default max array/map item count.
pub const DEFAULT_MAX_ITEMS: usize = 1_000_000;

/// Decode options (~cbor2 CBORDecoder).
#[derive(Debug, Clone)]
pub struct DecodeOptions {
    pub max_bytes: usize,
    pub max_depth: usize,
    pub max_items: usize,
    /// Apply semantic tag hooks (datetime, bignum, UUID, decimal).
    pub tag_hook: bool,
    pub allow_indefinite: bool,
    pub reject_trailing: bool,
    pub reject_duplicate_keys: bool,
}

impl Default for DecodeOptions {
    fn default() -> Self {
        Self {
            max_bytes: MAX_BYTES,
            max_depth: DEFAULT_MAX_DEPTH,
            max_items: DEFAULT_MAX_ITEMS,
            tag_hook: true,
            allow_indefinite: true,
            reject_trailing: false,
            reject_duplicate_keys: false,
        }
    }
}

/// Encode options (~cbor2 CBOREncoder).
#[derive(Debug, Clone)]
pub struct EncodeOptions {
    pub max_bytes: usize,
    pub max_depth: usize,
    /// RFC 8949 canonical CBOR (sorted map keys, minimal numeric encodings).
    pub canonical: bool,
    pub sort_keys: bool,
    /// Emit datetime strings as tag 0 (vs raw text).
    pub auto_datetime_tag: bool,
    /// Use tag 1 epoch for floats that look like timestamps.
    pub datetime_timestamp: bool,
    /// Allow indefinite-length strings/arrays/maps/bytes.
    pub indefinite_length: bool,
    /// Encode non-integer floats as tag 5 bigfloat.
    pub fractional_floats: bool,
    /// Prefix output with self-describe tag 55799.
    pub self_describe: bool,
}

impl Default for EncodeOptions {
    fn default() -> Self {
        Self {
            max_bytes: MAX_BYTES,
            max_depth: DEFAULT_MAX_DEPTH,
            canonical: false,
            sort_keys: false,
            auto_datetime_tag: false,
            datetime_timestamp: false,
            indefinite_length: false,
            fractional_floats: false,
            self_describe: false,
        }
    }
}

/// Wrap a value with a CBOR semantic tag for encoding.
pub fn tagged(tag: u64, value: CborValue) -> CborValue {
    CborValue::Tag(tag, Box::new(value))
}

/// Shorthand for `encode` with canonical options (COSE / deterministic).
pub fn encode_canonical(value: &CborValue) -> CborResult<Vec<u8>> {
    let opts = EncodeOptions {
        canonical: true,
        sort_keys: true,
        ..Default::default()
    };
    encode(value, &opts)
}

/// Encode with optional self-describe wrapper tag.
pub fn encode_with_opts(value: &CborValue, opts: &EncodeOptions) -> CborResult<Vec<u8>> {
    if opts.self_describe {
        let wrapped = CborValue::Tag(tags::SELF_DESCRIBE, Box::new(value.clone()));
        encode(&wrapped, opts)
    } else {
        encode(value, opts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_roundtrip() {
        let v = CborValue::Null;
        let bytes = encode(&v, &EncodeOptions::default()).unwrap();
        assert_eq!(decode(&bytes, &DecodeOptions::default()).unwrap(), v);
    }

    #[test]
    fn nested_structure() {
        let v = CborValue::Map(vec![
            (
                CborValue::String("items".into()),
                CborValue::Array(vec![CborValue::Int(1), CborValue::Bytes(vec![0xDE, 0xAD])]),
            ),
            (CborValue::String("ok".into()), CborValue::Bool(true)),
        ]);
        let bytes = encode(&v, &EncodeOptions::default()).unwrap();
        assert_eq!(decode(&bytes, &DecodeOptions::default()).unwrap(), v);
    }

    #[test]
    fn decode_all_sequence() {
        let a = encode(&CborValue::Int(1), &EncodeOptions::default()).unwrap();
        let b = encode(&CborValue::Int(2), &EncodeOptions::default()).unwrap();
        let mut buf = a;
        buf.extend(b);
        let items = decode_all(&buf, &DecodeOptions::default()).unwrap();
        assert_eq!(items.len(), 2);
    }
}
