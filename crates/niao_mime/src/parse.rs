//! MIME type parsing, validation, and wildcard matching.

use crate::categories::kind_of_mime;
use crate::error::{MimeError, MimeResult};
use crate::types::FileKind;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMime {
    pub type_: String,
    pub subtype: String,
    pub suffix: Option<String>,
    pub parameters: HashMap<String, String>,
    pub canonical: String,
}

/// Parse a MIME type string into components.
pub fn parse_mime(raw: &str) -> MimeResult<ParsedMime> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(MimeError::InvalidMime(raw.into()));
    }
    let (main, params) = match s.split_once(';') {
        Some((m, p)) => (m.trim(), Some(p.trim())),
        None => (s, None),
    };
    let mut parts = main.split('/');
    let type_ = parts
        .next()
        .filter(|t| !t.is_empty() && t.bytes().all(valid_token_byte))
        .ok_or_else(|| MimeError::InvalidMime(raw.into()))?
        .to_ascii_lowercase();
    let subtype_full = parts
        .next()
        .filter(|t| !t.is_empty() && t.bytes().all(valid_token_byte))
        .ok_or_else(|| MimeError::InvalidMime(raw.into()))?
        .to_ascii_lowercase();
    if parts.next().is_some() {
        return Err(MimeError::InvalidMime(raw.into()));
    }
    let (subtype, suffix) = if let Some((sub, suf)) = subtype_full.rsplit_once('+') {
        if sub.is_empty() || suf.is_empty() {
            return Err(MimeError::InvalidMime(raw.into()));
        }
        (sub.to_string(), Some(suf.to_string()))
    } else {
        (subtype_full, None)
    };
    let mut parameters = HashMap::new();
    if let Some(p) = params {
        for pair in p.split(';') {
            let pair = pair.trim();
            if pair.is_empty() {
                continue;
            }
            let (k, v) = pair
                .split_once('=')
                .ok_or_else(|| MimeError::InvalidMime(raw.into()))?;
            let key = k.trim().to_ascii_lowercase();
            if key.is_empty() {
                return Err(MimeError::InvalidMime(raw.into()));
            }
            let val = unquote(v.trim());
            parameters.insert(key, val);
        }
    }
    let canonical = if let Some(suf) = &suffix {
        format!("{type_}/{subtype}+{suf}")
    } else {
        format!("{type_}/{subtype}")
    };
    Ok(ParsedMime {
        type_,
        subtype,
        suffix,
        parameters,
        canonical,
    })
}

fn unquote(s: &str) -> String {
    if s.len() >= 2 {
        if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

fn valid_token_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'+' || b == b'-' || b == b'.'
}

pub fn is_valid_mime(raw: &str) -> bool {
    parse_mime(raw).is_ok()
}

pub fn normalize_mime(raw: &str) -> MimeResult<String> {
    Ok(parse_mime(raw)?.canonical)
}

/// Wildcard match: `image/*`, `*/json`, exact match.
pub fn mime_matches(value: &str, pattern: &str) -> MimeResult<bool> {
    let v = parse_mime(value)?;
    let p = pattern.trim().to_ascii_lowercase();
    if p == "*/*" {
        return Ok(true);
    }
    if let Some(rest) = p.strip_suffix("/*") {
        return Ok(v.type_ == rest);
    }
    if let Some(rest) = p.strip_prefix("*/") {
        return Ok(v.subtype == rest || v.suffix.as_deref() == Some(rest));
    }
    Ok(v.canonical == p)
}

pub fn kind_from_mime(mime: &str) -> FileKind {
    kind_of_mime(mime)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_suffix() {
        let p = parse_mime("application/ld+json; charset=utf-8").unwrap();
        assert_eq!(p.subtype, "ld");
        assert_eq!(p.suffix.as_deref(), Some("json"));
        assert_eq!(
            p.parameters.get("charset").map(String::as_str),
            Some("utf-8")
        );
    }

    #[test]
    fn wildcard() {
        assert!(mime_matches("image/png", "image/*").unwrap());
        assert!(mime_matches("application/ld+json", "*/json").unwrap());
    }
}
