//! Signed cookie value helpers.

use crate::error::SignError;
use crate::serializer::{Serializer, SerializerKind, SerializerOptions};
use niao_json_core::Value as JsonValue;

/// Sign a cookie value (name + JSON payload) using URL-safe timed serializer.
pub fn sign_cookie_value(
    name: &str,
    value: &JsonValue,
    secret: &[u8],
    opts: &SerializerOptions,
) -> Result<String, SignError> {
    let _ = name;
    let mut config = opts.clone().into_config();
    if config.salt.is_empty() || config.salt == b"itsdangerous.Signer" {
        config.salt = b"itsdangerous".to_vec();
    }
    let mut ser = Serializer::timed(secret, config, SerializerKind::UrlSafe)?;
    if let Some(max) = opts.max_age {
        ser.set_default_max_age(Some(max));
    }
    ser.dumps_json(value)
}

/// Parse `Set-Cookie` header or raw `name=value` and unsign the value.
pub fn unsign_cookie_value(
    cookie: &str,
    secret: &[u8],
    opts: &SerializerOptions,
) -> Result<JsonValue, SignError> {
    let value_part = extract_cookie_value(cookie)?;
    let mut config = opts.clone().into_config();
    if config.salt.is_empty() || config.salt == b"itsdangerous.Signer" {
        config.salt = b"itsdangerous".to_vec();
    }
    let ser = Serializer::timed(secret, config, SerializerKind::UrlSafe)?;
    ser.loads_json(&value_part, opts.max_age)
}

/// Build a `Set-Cookie` header fragment (without signing the name).
pub fn format_set_cookie(
    name: &str,
    signed_value: &str,
    max_age: Option<u64>,
    path: &str,
    http_only: bool,
    secure: bool,
    same_site: Option<&str>,
) -> String {
    let mut out = format!("{name}={signed_value}; Path={path}");
    if let Some(age) = max_age {
        out.push_str(&format!("; Max-Age={age}"));
    }
    if http_only {
        out.push_str("; HttpOnly");
    }
    if secure {
        out.push_str("; Secure");
    }
    if let Some(ss) = same_site {
        out.push_str(&format!("; SameSite={ss}"));
    }
    out
}

fn extract_cookie_value(cookie: &str) -> Result<String, SignError> {
    let trimmed = cookie.trim();
    // Strip attributes after first ';'
    let pair = trimmed.split(';').next().unwrap_or(trimmed).trim();
    let Some((_, value)) = pair.split_once('=') else {
        return Err(SignError::BadFormat);
    };
    Ok(value.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_json_core::parse;

    #[test]
    fn cookie_roundtrip() {
        let obj = parse(r#"{"uid":99}"#).unwrap();
        let opts = SerializerOptions {
            max_age: Some(3600),
            ..Default::default()
        };
        let signed = sign_cookie_value("sid", &obj, b"secret", &opts).unwrap();
        let header = format_set_cookie("sid", &signed, Some(3600), "/", true, false, Some("Lax"));
        let out = unsign_cookie_value(&header, b"secret", &opts).unwrap();
        assert_eq!(out.get("uid").and_then(|v| v.as_i64()), Some(99));
    }
}
