//! HTTP header field values (opaque bytes, visible ASCII for `to_str`).

use crate::types::header_name::HeaderName;
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct InvalidHeaderValue;

impl fmt::Display for InvalidHeaderValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid HTTP header value")
    }
}

impl std::error::Error for InvalidHeaderValue {}

#[derive(Debug, Clone)]
pub struct ToStrError;

impl fmt::Display for ToStrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("header value contains non-visible ASCII")
    }
}

impl std::error::Error for ToStrError {}

/// HTTP header value; may contain opaque bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct HeaderValue {
    inner: Vec<u8>,
}

impl HeaderValue {
    #[inline]
    pub fn from_static(src: &'static str) -> Self {
        Self::from_bytes(src.as_bytes()).expect("invalid static header value")
    }

    #[inline]
    pub fn from_bytes(src: &[u8]) -> Result<Self, InvalidHeaderValue> {
        if !is_valid_value(src) {
            return Err(InvalidHeaderValue);
        }
        Ok(Self {
            inner: src.to_vec(),
        })
    }

    #[inline]
    pub fn from_name(name: HeaderName) -> Self {
        Self::from_bytes(name.as_str().as_bytes()).expect("header name is valid value")
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        &self.inner
    }

    #[inline]
    pub fn to_str(&self) -> Result<&str, ToStrError> {
        if self.inner.iter().all(|&b| is_visible_ascii(b)) {
            // Safety: checked visible ASCII subset of UTF-8.
            Ok(unsafe { std::str::from_utf8_unchecked(&self.inner) })
        } else {
            Err(ToStrError)
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl fmt::Debug for HeaderValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.to_str() {
            Ok(s) => f.debug_tuple("HeaderValue").field(&s).finish(),
            Err(_) => f
                .debug_tuple("HeaderValue")
                .field(&format_args!("<binary {} bytes>", self.inner.len()))
                .finish(),
        }
    }
}

impl fmt::Display for HeaderValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.to_str() {
            Ok(s) => f.write_str(s),
            Err(_) => write!(f, "<binary {} bytes>", self.inner.len()),
        }
    }
}

impl FromStr for HeaderValue {
    type Err = InvalidHeaderValue;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_bytes(s.as_bytes())
    }
}

impl From<HeaderName> for HeaderValue {
    #[inline]
    fn from(name: HeaderName) -> Self {
        Self::from_name(name)
    }
}

#[inline]
fn is_valid_value(bytes: &[u8]) -> bool {
    bytes.iter().all(|&b| b == b'\t' || (b >= 0x20 && b != 0x7f))
}

#[inline]
fn is_visible_ascii(b: u8) -> bool {
    b >= 0x20 && b != 0x7f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_str() {
        let v = HeaderValue::from_str("text/html; charset=utf-8").unwrap();
        assert_eq!(v.to_str().unwrap(), "text/html; charset=utf-8");
    }

    #[test]
    fn rejects_newline() {
        assert!(HeaderValue::from_str("bad\nvalue").is_err());
    }
}
