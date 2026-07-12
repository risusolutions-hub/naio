//! HTTP status codes.

use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy)]
pub struct InvalidStatusCode;

impl fmt::Display for InvalidStatusCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid HTTP status code")
    }
}

impl std::error::Error for InvalidStatusCode {}

/// HTTP response status code (100–599).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StatusCode(u16);

impl StatusCode {
    pub const CONTINUE: StatusCode = StatusCode(100);
    pub const OK: StatusCode = StatusCode(200);
    pub const CREATED: StatusCode = StatusCode(201);
    pub const NO_CONTENT: StatusCode = StatusCode(204);
    pub const MOVED_PERMANENTLY: StatusCode = StatusCode(301);
    pub const FOUND: StatusCode = StatusCode(302);
    pub const NOT_MODIFIED: StatusCode = StatusCode(304);
    pub const BAD_REQUEST: StatusCode = StatusCode(400);
    pub const UNAUTHORIZED: StatusCode = StatusCode(401);
    pub const FORBIDDEN: StatusCode = StatusCode(403);
    pub const NOT_FOUND: StatusCode = StatusCode(404);
    pub const METHOD_NOT_ALLOWED: StatusCode = StatusCode(405);
    pub const INTERNAL_SERVER_ERROR: StatusCode = StatusCode(500);
    pub const BAD_GATEWAY: StatusCode = StatusCode(502);
    pub const SERVICE_UNAVAILABLE: StatusCode = StatusCode(503);

    pub const fn new(code: u16) -> Self {
        Self(code)
    }

    #[inline]
    pub const fn from_u16(src: u16) -> Result<Self, InvalidStatusCode> {
        if src >= 100 && src <= 599 {
            Ok(Self(src))
        } else {
            Err(InvalidStatusCode)
        }
    }

    #[inline]
    pub const fn as_u16(&self) -> u16 {
        self.0
    }

    #[inline]
    pub fn is_informational(&self) -> bool {
        (100..200).contains(&self.0)
    }

    #[inline]
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.0)
    }

    #[inline]
    pub fn is_redirection(&self) -> bool {
        (300..400).contains(&self.0)
    }

    #[inline]
    pub fn is_client_error(&self) -> bool {
        (400..500).contains(&self.0)
    }

    #[inline]
    pub fn is_server_error(&self) -> bool {
        (500..600).contains(&self.0)
    }

    #[inline]
    pub fn canonical_reason(&self) -> Option<&'static str> {
        Some(match self.0 {
            100 => "Continue",
            200 => "OK",
            201 => "Created",
            204 => "No Content",
            301 => "Moved Permanently",
            302 => "Found",
            304 => "Not Modified",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            500 => "Internal Server Error",
            502 => "Bad Gateway",
            503 => "Service Unavailable",
            _ => return None,
        })
    }

    /// Legacy helper used by the HTTP/1 server writer.
    #[inline]
    pub fn reason(self) -> &'static str {
        self.canonical_reason().unwrap_or("Unknown")
    }
}

impl fmt::Display for StatusCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u16> for StatusCode {
    #[inline]
    fn from(v: u16) -> Self {
        Self::new(v)
    }
}

impl FromStr for StatusCode {
    type Err = InvalidStatusCode;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let code: u16 = s.parse().map_err(|_| InvalidStatusCode)?;
        Self::from_u16(code)
    }
}

impl From<StatusCode> for u16 {
    #[inline]
    fn from(code: StatusCode) -> Self {
        code.as_u16()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifiers() {
        assert!(StatusCode::OK.is_success());
        assert!(StatusCode::NOT_FOUND.is_client_error());
        assert!(StatusCode::INTERNAL_SERVER_ERROR.is_server_error());
    }

    #[test]
    fn rejects_invalid() {
        assert!(StatusCode::from_u16(99).is_err());
        assert!(StatusCode::from_u16(600).is_err());
    }
}
