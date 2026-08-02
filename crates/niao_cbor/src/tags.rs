//! Well-known CBOR semantic tags (RFC 8949 and common extensions).

/// Standard date/time string (RFC 3339).
pub const DATETIME_STRING: u64 = 0;
/// POSIX epoch seconds (float or int).
pub const DATETIME_EPOCH: u64 = 1;
/// Positive bignum (byte string magnitude).
pub const BIGNUM_POS: u64 = 2;
/// Negative bignum (-1 - magnitude).
pub const BIGNUM_NEG: u64 = 3;
/// Decimal fraction [exponent, mantissa].
pub const DECIMAL_FRACTION: u64 = 4;
/// Bigfloat [exponent, mantissa].
pub const BIGFLOAT: u64 = 5;
/// Expected conversion to base64url string.
pub const EXPECTED_BASE64URL: u64 = 21;
/// Expected conversion to base64 string.
pub const EXPECTED_BASE64: u64 = 22;
/// Expected conversion to base16 string.
pub const EXPECTED_BASE16: u64 = 23;
/// Encoded CBOR data item.
pub const ENCODED_CBOR: u64 = 24;
/// URI.
pub const URI: u64 = 32;
/// base64url-encoded string.
pub const BASE64URL: u64 = 33;
/// base64-encoded string.
pub const BASE64: u64 = 34;
/// Regular expression (text).
pub const REGEX: u64 = 35;
/// MIME message (text).
pub const MIME: u64 = 36;
/// UUID (byte string, 16 bytes).
pub const UUID: u64 = 37;
/// Self-describe CBOR (tag 55799).
pub const SELF_DESCRIBE: u64 = 55799;

/// Human-readable names for `ncbor.tags`.
pub const KNOWN: &[(&str, u64)] = &[
    ("DATETIME_STRING", DATETIME_STRING),
    ("DATETIME_EPOCH", DATETIME_EPOCH),
    ("BIGNUM_POS", BIGNUM_POS),
    ("BIGNUM_NEG", BIGNUM_NEG),
    ("DECIMAL_FRACTION", DECIMAL_FRACTION),
    ("BIGFLOAT", BIGFLOAT),
    ("EXPECTED_BASE64URL", EXPECTED_BASE64URL),
    ("EXPECTED_BASE64", EXPECTED_BASE64),
    ("EXPECTED_BASE16", EXPECTED_BASE16),
    ("ENCODED_CBOR", ENCODED_CBOR),
    ("URI", URI),
    ("BASE64URL", BASE64URL),
    ("BASE64", BASE64),
    ("REGEX", REGEX),
    ("MIME", MIME),
    ("UUID", UUID),
    ("SELF_DESCRIBE", SELF_DESCRIBE),
];
