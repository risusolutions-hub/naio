//! HTTP header field names (case-insensitive, stored lowercase).

use std::borrow::Borrow;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct InvalidHeaderName;

impl fmt::Display for InvalidHeaderName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid HTTP header name")
    }
}

impl std::error::Error for InvalidHeaderName {}

/// Validated HTTP header name; compared and hashed case-insensitively.
#[derive(Clone, Eq)]
pub struct HeaderName {
    inner: String,
}

impl HeaderName {
    #[inline]
    pub fn from_static(src: &'static str) -> Self {
        assert!(
            is_valid_name(src.as_bytes()) && !src.bytes().any(|b| b.is_ascii_uppercase()),
            "HeaderName::from_static requires valid lowercase name"
        );
        Self {
            inner: src.to_string(),
        }
    }

    #[inline]
    pub fn from_bytes(src: &[u8]) -> Result<Self, InvalidHeaderName> {
        if !is_valid_name(src) {
            return Err(InvalidHeaderName);
        }
        let inner = std::str::from_utf8(src)
            .map_err(|_| InvalidHeaderName)?
            .to_ascii_lowercase();
        Ok(Self { inner })
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        &self.inner
    }
}

impl PartialEq for HeaderName {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Hash for HeaderName {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inner.hash(state);
    }
}

impl fmt::Debug for HeaderName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("HeaderName").field(&self.inner).finish()
    }
}

impl fmt::Display for HeaderName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.inner)
    }
}

impl FromStr for HeaderName {
    type Err = InvalidHeaderName;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_bytes(s.as_bytes())
    }
}

impl AsRef<str> for HeaderName {
    #[inline]
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for HeaderName {
    #[inline]
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

#[inline]
fn is_valid_name(bytes: &[u8]) -> bool {
    !bytes.is_empty()
        && bytes
            .iter()
            .all(|&b| matches!(b, b'!'..=b'~') && b != b':' && b != b' ')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_to_lowercase() {
        let n = HeaderName::from_str("Content-Type").unwrap();
        assert_eq!(n.as_str(), "content-type");
    }

    #[test]
    fn rejects_empty() {
        assert!(HeaderName::from_str("").is_err());
    }

    #[test]
    fn rejects_colon() {
        assert!(HeaderName::from_bytes(b"bad:name").is_err());
    }
}
