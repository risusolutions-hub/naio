//! Google Key URI Format — `otpauth://` provisioning and parsing.

use crate::digest::{Digest, DEFAULT_DIGEST};
use crate::error::OtpError;
use crate::hotp::Hotp;
use crate::hotp::DEFAULT_DIGITS;
use crate::totp::{Totp, DEFAULT_INTERVAL};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OtpKind {
    Totp,
    Hotp,
}

#[derive(Debug, Clone)]
pub enum ParsedOtp {
    Totp(Totp),
    Hotp(Hotp),
}

impl ParsedOtp {
    pub fn kind(&self) -> OtpKind {
        match self {
            Self::Totp(_) => OtpKind::Totp,
            Self::Hotp(_) => OtpKind::Hotp,
        }
    }
}

pub fn build_totp_uri(
    name: &str,
    issuer: Option<&str>,
    secret_b32: &str,
    digits: u32,
    digest: Digest,
    interval: u64,
) -> String {
    let label = format_label(issuer, name);
    let mut uri = format!("otpauth://totp/{label}?secret={secret_b32}");
    if let Some(iss) = issuer {
        uri.push_str("&issuer=");
        uri.push_str(&url_encode(iss));
    }
    if digits != DEFAULT_DIGITS {
        uri.push_str(&format!("&digits={digits}"));
    }
    if digest != DEFAULT_DIGEST {
        uri.push_str("&algorithm=");
        uri.push_str(digest.name());
    }
    if interval != DEFAULT_INTERVAL {
        uri.push_str(&format!("&period={interval}"));
    }
    uri
}

pub fn build_hotp_uri(
    name: &str,
    issuer: Option<&str>,
    secret_b32: &str,
    digits: u32,
    digest: Digest,
    counter: Option<u64>,
) -> String {
    let label = format_label(issuer, name);
    let mut uri = format!("otpauth://hotp/{label}?secret={secret_b32}");
    if let Some(iss) = issuer {
        uri.push_str("&issuer=");
        uri.push_str(&url_encode(iss));
    }
    if digits != DEFAULT_DIGITS {
        uri.push_str(&format!("&digits={digits}"));
    }
    if digest != DEFAULT_DIGEST {
        uri.push_str("&algorithm=");
        uri.push_str(digest.name());
    }
    if let Some(c) = counter {
        uri.push_str(&format!("&counter={c}"));
    }
    uri
}

fn format_label(issuer: Option<&str>, name: &str) -> String {
    match issuer {
        Some(iss) if !name.starts_with(&format!("{iss}:")) => {
            format!("{}:{}", url_encode_path(iss), url_encode_path(name))
        }
        _ => url_encode_path(name),
    }
}

pub fn parse_uri(uri: &str) -> Result<ParsedOtp, OtpError> {
    let uri = uri.trim();
    if !uri.starts_with("otpauth://") {
        return Err(OtpError::InvalidUri("must start with otpauth://".into()));
    }
    let rest = &uri[10..];
    let (scheme, path_and_query) = rest
        .split_once('/')
        .ok_or_else(|| OtpError::InvalidUri("missing path".into()))?;
    let kind = match scheme.to_ascii_lowercase().as_str() {
        "totp" => OtpKind::Totp,
        "hotp" => OtpKind::Hotp,
        other => return Err(OtpError::InvalidUri(format!("unknown type {other}"))),
    };

    let (path, query) = match path_and_query.split_once('?') {
        Some((p, q)) => (p, q),
        None => (path_and_query, ""),
    };

    let params = parse_query(query);
    let secret = params
        .get("secret")
        .ok_or_else(|| OtpError::InvalidUri("missing secret".into()))?
        .clone();

    let digits = params
        .get("digits")
        .map(|s| s.parse::<u32>())
        .transpose()
        .map_err(|_| OtpError::InvalidUri("invalid digits".into()))?
        .unwrap_or(DEFAULT_DIGITS);

    let digest = match params.get("algorithm") {
        Some(a) => Digest::parse(a)?,
        None => DEFAULT_DIGEST,
    };

    let issuer = params.get("issuer").cloned();
    let name = url_decode_path(path);
    let display_name = if let Some(ref iss) = issuer {
        if name.starts_with(&format!("{iss}:")) {
            name.split_once(':')
                .map(|(_, n)| n.to_string())
                .unwrap_or(name)
        } else {
            name.clone()
        }
    } else {
        name.clone()
    };

    match kind {
        OtpKind::Totp => {
            let interval = params
                .get("period")
                .map(|s| s.parse::<u64>())
                .transpose()
                .map_err(|_| OtpError::InvalidUri("invalid period".into()))?
                .unwrap_or(DEFAULT_INTERVAL);
            let mut t = Totp::new(&secret, digits, interval, digest)?;
            t = t.with_labels(Some(display_name), issuer);
            Ok(ParsedOtp::Totp(t))
        }
        OtpKind::Hotp => {
            let mut h = Hotp::new(&secret, digits, digest)?;
            h = h.with_labels(Some(display_name), issuer);
            Ok(ParsedOtp::Hotp(h))
        }
    }
}

fn parse_query(query: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        if let Some((k, v)) = pair.split_once('=') {
            map.insert(url_decode(k).to_ascii_lowercase(), url_decode(v));
        }
    }
    map
}

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn url_encode_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b':' => out.push_str("%3A"),
            b'@' => out.push_str("%40"),
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) =
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16)
            {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn url_decode_path(s: &str) -> String {
    url_decode(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_google_totp_uri() {
        let uri =
            "otpauth://totp/Example:user%40example.com?secret=JBSWY3DPEHPK3PXP&issuer=Example";
        let parsed = parse_uri(uri).unwrap();
        match parsed {
            ParsedOtp::Totp(t) => {
                assert_eq!(t.at(59), "996554");
                assert_eq!(t.digits(), 6);
            }
            _ => panic!("expected totp"),
        }
    }

    #[test]
    fn roundtrip_uri() {
        let uri = build_totp_uri(
            "user@example.com",
            Some("MyApp"),
            "JBSWY3DPEHPK3PXP",
            6,
            DEFAULT_DIGEST,
            30,
        );
        let parsed = parse_uri(&uri).unwrap();
        assert!(matches!(parsed, ParsedOtp::Totp(_)));
    }
}
