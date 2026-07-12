//! HTTP request URI (path-only or absolute form).

use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct InvalidUri;

impl fmt::Display for InvalidUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid URI")
    }
}

impl std::error::Error for InvalidUri {}

/// Request target URI (`/path`, `*`, or absolute `http://...`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uri {
    bytes: String,
    scheme_end: Option<usize>,
    authority_end: Option<usize>,
}

impl Uri {
    #[inline]
    pub fn from_bytes(src: &[u8]) -> Result<Self, InvalidUri> {
        if src.is_empty() {
            return Err(InvalidUri);
        }
        if src.contains(&b' ') {
            return Err(InvalidUri);
        }
        let s = std::str::from_utf8(src).map_err(|_| InvalidUri)?;
        Self::from_str(s)
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        &self.bytes
    }

    #[inline]
    pub fn scheme(&self) -> Option<&str> {
        let end = self.scheme_end?;
        Some(&self.bytes[..end])
    }

    #[inline]
    pub fn authority(&self) -> Option<&str> {
        let scheme_end = self.scheme_end?;
        let auth_end = self.authority_end?;
        Some(&self.bytes[scheme_end + 3..auth_end])
    }

    #[inline]
    pub fn path(&self) -> &str {
        match (self.scheme_end, self.authority_end) {
            (Some(_), Some(auth_end)) => {
                let rest = &self.bytes[auth_end..];
                rest.split_once('?')
                    .map(|(p, _)| p)
                    .unwrap_or(rest)
            }
            _ => self
                .bytes
                .split_once('?')
                .map(|(p, _)| p)
                .unwrap_or(&self.bytes),
        }
    }

    #[inline]
    pub fn path_and_query(&self) -> &str {
        match (self.scheme_end, self.authority_end) {
            (Some(_), Some(auth_end)) => &self.bytes[auth_end..],
            _ => &self.bytes,
        }
    }

    #[inline]
    pub fn query(&self) -> Option<&str> {
        let pq = self.path_and_query();
        pq.split_once('?').map(|(_, q)| q)
    }
}

impl FromStr for Uri {
    type Err = InvalidUri;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() || s.contains(' ') {
            return Err(InvalidUri);
        }
        let (scheme_end, authority_end) = if let Some(pos) = s.find("://") {
            let scheme = &s[..pos];
            if scheme.is_empty() || !scheme.bytes().all(is_scheme_char) {
                return Err(InvalidUri);
            }
            let rest = &s[pos + 3..];
            let auth_end = pos + 3 + authority_len(rest);
            (Some(pos), Some(auth_end))
        } else {
            (None, None)
        };
        Ok(Self {
            bytes: s.to_string(),
            scheme_end,
            authority_end,
        })
    }
}

impl fmt::Display for Uri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.bytes)
    }
}

#[inline]
fn is_scheme_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.')
}

fn authority_len(rest: &str) -> usize {
    if rest.starts_with('[') {
        if let Some(end) = rest.find(']') {
            let mut len = end + 1;
            if rest.as_bytes().get(len) == Some(&b':') {
                if let Some(slash) = rest[len..].find('/') {
                    len += slash;
                } else {
                    len = rest.len();
                }
            } else if let Some(slash) = rest[len..].find('/') {
                len += slash;
            } else {
                len = rest.len();
            }
            return len;
        }
        return rest.len();
    }
    rest.find('/').unwrap_or(rest.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_only() {
        let u = Uri::from_str("/hello/world?q=1").unwrap();
        assert_eq!(u.path(), "/hello/world");
        assert_eq!(u.query(), Some("q=1"));
        assert!(u.scheme().is_none());
    }

    #[test]
    fn absolute() {
        let u = Uri::from_str("https://example.com/path").unwrap();
        assert_eq!(u.scheme(), Some("https"));
        assert_eq!(u.authority(), Some("example.com"));
        assert_eq!(u.path(), "/path");
    }
}
