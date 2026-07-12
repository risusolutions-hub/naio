//! URL parse, build, percent-encoding, and resolution.
//!
//! Isolated module tree (separate from HTTP types/parser) for parallel agent work.

mod encode;
mod parse;
mod resolve;

#[cfg(test)]
mod wpt;

pub use encode::{form_urlencode, percent_decode, percent_encode};
pub use parse::parse_url;
pub use resolve::join;

/// Parsed absolute URL components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub path: String,
    pub query: String,
    pub fragment: String,
    pub user: String,
    pub password: String,
}

/// Structured view of a parsed URL (optional fields omit defaults / empties).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlComponents {
    pub scheme: String,
    pub username: String,
    pub password: String,
    pub host: String,
    pub port: Option<u16>,
    pub path: String,
    pub query: Option<String>,
    pub fragment: Option<String>,
}

impl Url {
    #[inline]
    pub fn default_port(scheme: &str) -> u16 {
        parse::default_port(scheme)
    }

    /// Parse an absolute URL (alias for [`parse_url`]).
    #[inline]
    pub fn parse(input: &str) -> Result<Self, String> {
        parse_url(input)
    }

    /// Decompose into a structured view (default ports omitted).
    pub fn components(&self) -> UrlComponents {
        parse::components(self)
    }

    /// Serialize origin (`scheme://host[:port]`).
    pub fn origin(&self) -> String {
        parse::origin(self)
    }

    /// Authority host[:port] without credentials.
    pub fn authority(&self) -> String {
        parse::authority(self)
    }

    /// Full serialization including credentials, path, query, fragment.
    pub fn to_string_full(&self) -> String {
        parse::to_string_full(self)
    }

    /// Resolve `input` relative to this URL.
    pub fn join(&self, input: &str) -> Result<Url, String> {
        resolve::resolve(self, input)
    }

    /// Iterate `(key, value)` pairs from the query string.
    pub fn query_pairs(&self) -> parse::QueryPairs<'_> {
        parse::query_pairs(self)
    }
}
