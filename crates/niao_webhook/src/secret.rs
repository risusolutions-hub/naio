//! Secret parsing (`whsec_` + standard base64) and constants.

use crate::error::{WebhookError, WebhookResult};
use niao_codec::base64::{decode_standard, encode_standard};

/// Standard Webhooks secret prefix.
pub const SECRET_PREFIX: &str = "whsec_";

/// Default timestamp tolerance: 5 minutes (per Standard Webhooks).
pub const DEFAULT_TOLERANCE_SECS: i64 = 5 * 60;

/// Canonical header names (lowercase).
pub const HDR_ID: &str = "webhook-id";
pub const HDR_TIMESTAMP: &str = "webhook-timestamp";
pub const HDR_SIGNATURE: &str = "webhook-signature";

/// How the secret string is interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SecretFormat {
    /// Strip optional `whsec_`, then standard-base64-decode (Standard Webhooks).
    #[default]
    Standard,
    /// Treat the string as raw key bytes (UTF-8), no base64.
    Raw,
}

/// Decode a webhook signing secret into raw HMAC key bytes.
pub fn parse_secret(secret: &str, format: SecretFormat) -> WebhookResult<Vec<u8>> {
    if secret.is_empty() {
        return Err(WebhookError::EmptySecret);
    }
    match format {
        SecretFormat::Raw => {
            let bytes = secret.as_bytes().to_vec();
            if bytes.is_empty() {
                return Err(WebhookError::EmptySecret);
            }
            Ok(bytes)
        }
        SecretFormat::Standard => {
            let b64 = if let Some(rest) = secret.strip_prefix(SECRET_PREFIX) {
                rest
            } else {
                secret
            };
            if b64.is_empty() {
                return Err(WebhookError::EmptySecret);
            }
            // Pad for unpadded base64 (matches Python reference).
            let padded = pad_b64(b64);
            decode_standard(&padded)
                .map_err(|e| WebhookError::InvalidSecret(format!("base64 decode failed: {e}")))
        }
    }
}

/// Encode raw key bytes as a `whsec_<base64>` secret string.
pub fn encode_secret(key: &[u8]) -> String {
    format!("{SECRET_PREFIX}{}", encode_standard(key))
}

fn pad_b64(s: &str) -> String {
    let rem = s.len() % 4;
    if rem == 0 {
        s.to_string()
    } else {
        let mut out = String::with_capacity(s.len() + (4 - rem));
        out.push_str(s);
        out.push_str(&"="[..(4 - rem)]);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_with_and_without_prefix() {
        let raw = b"hello-secret-key!!!!!";
        let b64 = encode_standard(raw);
        let with = format!("whsec_{b64}");
        assert_eq!(parse_secret(&with, SecretFormat::Standard).unwrap(), raw);
        assert_eq!(parse_secret(&b64, SecretFormat::Standard).unwrap(), raw);
    }

    #[test]
    fn empty_rejected() {
        assert!(matches!(
            parse_secret("", SecretFormat::Standard),
            Err(WebhookError::EmptySecret)
        ));
        assert!(matches!(
            parse_secret("whsec_", SecretFormat::Standard),
            Err(WebhookError::EmptySecret)
        ));
    }

    #[test]
    fn raw_format() {
        let k = parse_secret("abc", SecretFormat::Raw).unwrap();
        assert_eq!(k, b"abc");
    }
}
