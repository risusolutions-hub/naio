//! URL-safe base64 helpers and integer packing (itsdangerous-compatible).

use niao_codec::base64::{decode_url_safe, encode_url_safe_no_pad};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodingError {
    InvalidBase64,
}

impl fmt::Display for EncodingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBase64 => write!(f, "invalid base64-encoded data"),
        }
    }
}

impl std::error::Error for EncodingError {}

#[inline]
pub fn b64_encode(data: &[u8]) -> String {
    encode_url_safe_no_pad(data)
}

#[inline]
pub fn b64_decode(input: &str) -> Result<Vec<u8>, EncodingError> {
    decode_url_safe(input).map_err(|_| EncodingError::InvalidBase64)
}

/// Pack a Unix timestamp as big-endian u64 with leading zeros stripped.
#[inline]
pub fn int_to_bytes(num: u64) -> Vec<u8> {
    let buf = num.to_be_bytes();
    let start = buf.iter().position(|&b| b != 0).unwrap_or(buf.len() - 1);
    buf[start..].to_vec()
}

/// Unpack a timestamp from bytes (zero-padded to 8 bytes).
#[inline]
pub fn bytes_to_int(bytestr: &[u8]) -> Result<u64, EncodingError> {
    if bytestr.len() > 8 {
        return Err(EncodingError::InvalidBase64);
    }
    let mut buf = [0u8; 8];
    buf[8 - bytestr.len()..].copy_from_slice(bytestr);
    Ok(u64::from_be_bytes(buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_roundtrip() {
        for ts in [0u64, 1, 1_700_000_000, u64::MAX] {
            let b = int_to_bytes(ts);
            assert_eq!(bytes_to_int(&b).unwrap(), ts);
        }
    }

    #[test]
    fn b64_roundtrip() {
        let data = b"hello+world/safe?";
        let enc = b64_encode(data);
        assert!(!enc.contains('='));
        assert_eq!(b64_decode(&enc).unwrap(), data);
    }
}
