//! JWT HS256 / HS512 sign and verify.

use crate::ct;
use crate::hmac::{hmac_sha256, hmac_sha512};
use crate::sha256::Sha256;
use niao_codec::base64::{decode_url_safe, encode_url_safe_no_pad};
use niao_json_core::{parse, Value};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    HS256,
    HS512,
}

impl Algorithm {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "HS256" => Some(Self::HS256),
            "HS512" => Some(Self::HS512),
            "none" | "None" | "NONE" => None,
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum JwtError {
    Format,
    Base64,
    Json(String),
    Algorithm,
    Signature,
    Expired,
    NotBefore,
    Message(String),
}

impl fmt::Display for JwtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Format => write!(f, "invalid JWT format"),
            Self::Base64 => write!(f, "invalid base64url segment"),
            Self::Json(e) => write!(f, "invalid JWT JSON: {e}"),
            Self::Algorithm => write!(f, "unsupported or forbidden JWT algorithm"),
            Self::Signature => write!(f, "invalid JWT signature"),
            Self::Expired => write!(f, "JWT expired"),
            Self::NotBefore => write!(f, "JWT not yet valid"),
            Self::Message(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for JwtError {}

pub struct Validation {
    pub validate_exp: bool,
    pub validate_nbf: bool,
    pub leeway: u64,
}

impl Default for Validation {
    fn default() -> Self {
        Self {
            validate_exp: true,
            validate_nbf: false,
            leeway: 0,
        }
    }
}

pub fn sign_hs256(
    header_json: &str,
    payload_json: &str,
    secret: &[u8],
) -> Result<String, JwtError> {
    sign(Algorithm::HS256, header_json, payload_json, secret)
}

pub fn sign_hs512(
    header_json: &str,
    payload_json: &str,
    secret: &[u8],
) -> Result<String, JwtError> {
    sign(Algorithm::HS512, header_json, payload_json, secret)
}

pub fn sign(
    algo: Algorithm,
    header_json: &str,
    payload_json: &str,
    secret: &[u8],
) -> Result<String, JwtError> {
    let header_b64 = encode_url_safe_no_pad(header_json.as_bytes());
    let payload_b64 = encode_url_safe_no_pad(payload_json.as_bytes());
    let signing_input = format!("{header_b64}.{payload_b64}");
    let sig = match algo {
        Algorithm::HS256 => hmac_sha256(secret, signing_input.as_bytes()).to_vec(),
        Algorithm::HS512 => hmac_sha512(secret, signing_input.as_bytes()).to_vec(),
    };
    Ok(format!(
        "{}.{}",
        signing_input,
        encode_url_safe_no_pad(&sig)
    ))
}

pub fn verify(token: &str, secret: &[u8], validation: &Validation) -> Result<Value, JwtError> {
    let (header_json, payload_json, _sig_bytes, _algo) = split_and_verify_sig(token, secret)?;
    validate_times(&payload_json, validation)?;
    let _ = header_json;
    parse(&payload_json).map_err(|e| JwtError::Json(e.to_string()))
}

pub fn decode_unverified(token: &str) -> Result<(Value, Value), JwtError> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(JwtError::Format);
    }
    let header_bytes = decode_url_safe(parts[0]).map_err(|_| JwtError::Base64)?;
    let payload_bytes = decode_url_safe(parts[1]).map_err(|_| JwtError::Base64)?;
    let header_json = std::str::from_utf8(&header_bytes).map_err(|_| JwtError::Format)?;
    let payload_json = std::str::from_utf8(&payload_bytes).map_err(|_| JwtError::Format)?;
    let header = parse(header_json).map_err(|e| JwtError::Json(e.to_string()))?;
    let payload = parse(payload_json).map_err(|e| JwtError::Json(e.to_string()))?;
    Ok((header, payload))
}

fn split_and_verify_sig(
    token: &str,
    secret: &[u8],
) -> Result<(String, String, Vec<u8>, Algorithm), JwtError> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(JwtError::Format);
    }
    let signing_input = format!("{}.{}", parts[0], parts[1]);
    let header_bytes = decode_url_safe(parts[0]).map_err(|_| JwtError::Base64)?;
    let payload_bytes = decode_url_safe(parts[1]).map_err(|_| JwtError::Base64)?;
    let sig_bytes = decode_url_safe(parts[2]).map_err(|_| JwtError::Base64)?;
    let header_json = String::from_utf8(header_bytes).map_err(|_| JwtError::Format)?;
    let payload_json = String::from_utf8(payload_bytes).map_err(|_| JwtError::Format)?;
    let header = parse(&header_json).map_err(|e| JwtError::Json(e.to_string()))?;
    let alg_str = header
        .get("alg")
        .and_then(|v| v.as_str())
        .ok_or(JwtError::Algorithm)?;
    let algo = Algorithm::from_str(alg_str).ok_or(JwtError::Algorithm)?;
    let expected = match algo {
        Algorithm::HS256 => hmac_sha256(secret, signing_input.as_bytes()).to_vec(),
        Algorithm::HS512 => hmac_sha512(secret, signing_input.as_bytes()).to_vec(),
    };
    if !ct::eq(&expected, &sig_bytes) {
        return Err(JwtError::Signature);
    }
    Ok((header_json, payload_json, sig_bytes, algo))
}

fn validate_times(payload_json: &str, validation: &Validation) -> Result<(), JwtError> {
    let payload = parse(payload_json).map_err(|e| JwtError::Json(e.to_string()))?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| JwtError::Message(e.to_string()))?
        .as_secs();
    if validation.validate_exp {
        if let Some(exp) = payload.get("exp").and_then(|v| v.as_i64()) {
            if now > exp as u64 + validation.leeway {
                return Err(JwtError::Expired);
            }
        }
    }
    if validation.validate_nbf {
        if let Some(nbf) = payload.get("nbf").and_then(|v| v.as_i64()) {
            if now + validation.leeway < nbf as u64 {
                return Err(JwtError::NotBefore);
            }
        }
    }
    Ok(())
}

/// Default JWT header JSON for HS256.
pub fn default_header_hs256() -> &'static str {
    r#"{"alg":"HS256","typ":"JWT"}"#
}

/// Hash payload for session-style tokens (secret prefix + payload SHA-256 hex).
pub fn sha256_hex_secret_prefix(secret: &[u8], payload: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(secret);
    h.update(payload);
    crate::hex::encode(&h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jwt_io_vector_hs256() {
        let token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.\
            eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiYWRtaW4iOnRydWUsImlhdCI6MTUxNjIzOTAyMn0.\
            reGQzG3OKdoIMWLDKOZ4TICJit3EW69cQE72E2CfzRE";
        let secret = b"your-256-bit-secret";
        let validation = Validation {
            validate_exp: false,
            ..Default::default()
        };
        let payload = verify(token, secret, &validation).unwrap();
        assert_eq!(
            payload.get("sub").and_then(|v| v.as_str()),
            Some("1234567890")
        );
    }

    #[test]
    fn reject_alg_none() {
        let token = "eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0.\
            eyJzdWIiOiIxMjM0NTY3ODkwIn0.";
        assert!(verify(token, b"x", &Validation::default()).is_err());
    }

    #[test]
    fn sign_verify_roundtrip() {
        let payload = r#"{"sub":"user1","exp":9999999999}"#;
        let token = sign_hs256(default_header_hs256(), payload, b"secret").unwrap();
        let validation = Validation {
            validate_exp: false,
            ..Default::default()
        };
        let out = verify(&token, b"secret", &validation).unwrap();
        assert_eq!(out.get("sub").and_then(|v| v.as_str()), Some("user1"));
    }
}
