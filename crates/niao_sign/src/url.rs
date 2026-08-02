//! Signed URL query parameter helpers.

use crate::error::SignError;
use crate::serializer::{Serializer, SerializerKind, SerializerOptions};
use niao_json_core::Value as JsonValue;

const DEFAULT_PARAM: &str = "token";

/// Append a signed token query parameter to a URL.
pub fn sign_url(
    base_url: &str,
    params: &JsonValue,
    secret: &[u8],
    opts: &SerializerOptions,
    param_name: &str,
) -> Result<String, SignError> {
    let mut config = opts.clone().into_config();
    if config.salt.is_empty() || config.salt == b"itsdangerous.Signer" {
        config.salt = b"itsdangerous".to_vec();
    }
    let mut ser = Serializer::timed(secret, config, SerializerKind::UrlSafe)?;
    if let Some(max) = opts.max_age {
        ser.set_default_max_age(Some(max));
    }
    let token = ser.dumps_json(params)?;
    let sep = if base_url.contains('?') { '&' } else { '?' };
    Ok(format!("{base_url}{sep}{param_name}={token}"))
}

/// Extract and verify a signed token from a URL.
pub fn unsign_url(
    url: &str,
    secret: &[u8],
    opts: &SerializerOptions,
    param_name: &str,
) -> Result<JsonValue, SignError> {
    let token = extract_query_param(url, param_name)?;
    let mut config = opts.clone().into_config();
    if config.salt.is_empty() || config.salt == b"itsdangerous.Signer" {
        config.salt = b"itsdangerous".to_vec();
    }
    let ser = Serializer::timed(secret, config, SerializerKind::UrlSafe)?;
    ser.loads_json(&token, opts.max_age)
}

/// Default query parameter name for signed URLs.
pub fn default_param() -> &'static str {
    DEFAULT_PARAM
}

fn extract_query_param(url: &str, name: &str) -> Result<String, SignError> {
    let Some(q_start) = url.find('?') else {
        return Err(SignError::BadFormat);
    };
    let query = &url[q_start + 1..];
    let query = query.split('#').next().unwrap_or(query);
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        if let Some((k, v)) = pair.split_once('=') {
            if k == name {
                return Ok(v.to_string());
            }
        } else if pair == name {
            return Ok(String::new());
        }
    }
    Err(SignError::BadFormat)
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_json_core::parse;

    #[test]
    fn url_roundtrip() {
        let obj = parse(r#"{"user":"alice"}"#).unwrap();
        let opts = SerializerOptions {
            max_age: Some(7200),
            ..Default::default()
        };
        let url = sign_url("https://app.example/reset", &obj, b"key", &opts, "token").unwrap();
        assert!(url.contains("token="));
        let out = unsign_url(&url, b"key", &opts, "token").unwrap();
        assert_eq!(out.get("user").and_then(|v| v.as_str()), Some("alice"));
    }
}
