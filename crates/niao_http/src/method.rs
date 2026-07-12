//! HTTP methods.

use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    Get,
    Head,
    Post,
    Put,
    Delete,
    Patch,
    Options,
    Connect,
    Trace,
}

impl Method {
    pub const GET: Method = Method::Get;
    pub const HEAD: Method = Method::Head;
    pub const POST: Method = Method::Post;
    pub const PUT: Method = Method::Put;
    pub const DELETE: Method = Method::Delete;
    pub const PATCH: Method = Method::Patch;
    pub const OPTIONS: Method = Method::Options;
    pub const CONNECT: Method = Method::Connect;
    pub const TRACE: Method = Method::Trace;

    #[inline]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Head => "HEAD",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
            Self::Patch => "PATCH",
            Self::Options => "OPTIONS",
            Self::Connect => "CONNECT",
            Self::Trace => "TRACE",
        }
    }

    #[inline]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "GET" => Some(Self::Get),
            "HEAD" => Some(Self::Head),
            "POST" => Some(Self::Post),
            "PUT" => Some(Self::Put),
            "DELETE" => Some(Self::Delete),
            "PATCH" => Some(Self::Patch),
            "OPTIONS" => Some(Self::Options),
            "CONNECT" => Some(Self::Connect),
            "TRACE" => Some(Self::Trace),
            _ => None,
        }
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Method {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or(())
    }
}

impl AsRef<str> for Method {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_str_roundtrip() {
        assert_eq!(Method::from_str("GET").unwrap(), Method::GET);
        assert!(Method::from_str("INVALID").is_err());
    }
}
