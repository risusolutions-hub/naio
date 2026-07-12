//! Hexadecimal encode/decode.

use std::fmt;

const HEX_LOWER: &[u8; 16] = b"0123456789abcdef";
const HEX_UPPER: &[u8; 16] = b"0123456789ABCDEF";

#[derive(Debug, PartialEq, Eq)]
pub enum HexError {
    OddLength,
    InvalidChar(u8),
}

impl fmt::Display for HexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OddLength => write!(f, "hex string has odd length"),
            Self::InvalidChar(c) => write!(f, "invalid hex character: {c}"),
        }
    }
}

impl std::error::Error for HexError {}

#[inline]
fn nibble_value(b: u8) -> Result<u8, HexError> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        other => Err(HexError::InvalidChar(other)),
    }
}

/// Encode bytes as lowercase hex.
pub fn encode(data: &[u8]) -> String {
    encode_with(data, false)
}

/// Encode bytes as hex with optional uppercase letters.
pub fn encode_with(data: &[u8], uppercase: bool) -> String {
    let table = if uppercase { HEX_UPPER } else { HEX_LOWER };
    let mut out = Vec::with_capacity(data.len() * 2);
    for &b in data {
        out.push(table[(b >> 4) as usize]);
        out.push(table[(b & 0x0f) as usize]);
    }
    unsafe { String::from_utf8_unchecked(out) }
}

/// Decode a hex string into bytes.
pub fn decode(input: &str) -> Result<Vec<u8>, HexError> {
    let s = input.as_bytes();
    if s.is_empty() {
        return Ok(Vec::new());
    }
    if s.len() % 2 != 0 {
        return Err(HexError::OddLength);
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let mut i = 0;
    while i < s.len() {
        let hi = nibble_value(s[i])?;
        let lo = nibble_value(s[i + 1])?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let raw = b"Hello";
        assert_eq!(encode(raw), "48656c6c6f");
        assert_eq!(decode("48656c6c6f").unwrap(), raw.as_slice());
    }

    #[test]
    fn uppercase_decode() {
        assert_eq!(decode("48656C6C6F").unwrap(), b"Hello".as_slice());
    }

    #[test]
    fn odd_length() {
        assert_eq!(decode("abc"), Err(HexError::OddLength));
    }
}
