//! JSON serializers — `Serializer`, `UrlSafeSerializer`, timed variants.

use crate::encoding::b64_decode;
use crate::error::{SignError, UnsafeLoad};
use crate::signer::{Digest, KeyDerivation, Signer, SignerConfig, TimestampSigner};
use niao_json_core::{parse, to_string, Value as JsonValue};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerializerKind {
    /// Raw JSON bytes as payload (standard Serializer).
    Json,
    /// Base64url-encoded JSON payload (URLSafeSerializer).
    UrlSafe,
}

#[derive(Debug, Clone)]
pub struct Serializer {
    signer: Signer,
    timed: bool,
    kind: SerializerKind,
    default_max_age: Option<u64>,
}

impl Serializer {
    pub fn new(
        secret: impl AsRef<[u8]>,
        mut config: SignerConfig,
        kind: SerializerKind,
    ) -> Result<Self, SignError> {
        config.salt = if config.salt.is_empty() {
            b"itsdangerous".to_vec()
        } else {
            config.salt
        };
        Ok(Self {
            signer: Signer::new(secret, config)?,
            timed: false,
            kind,
            default_max_age: None,
        })
    }

    pub fn timed(
        secret: impl AsRef<[u8]>,
        mut config: SignerConfig,
        kind: SerializerKind,
    ) -> Result<Self, SignError> {
        config.salt = if config.salt.is_empty() {
            b"itsdangerous".to_vec()
        } else {
            config.salt
        };
        Ok(Self {
            signer: Signer::new(secret, config)?,
            timed: true,
            kind,
            default_max_age: None,
        })
    }

    pub fn with_keys(
        keys: Vec<Vec<u8>>,
        mut config: SignerConfig,
        kind: SerializerKind,
        timed: bool,
    ) -> Result<Self, SignError> {
        config.salt = if config.salt.is_empty() {
            b"itsdangerous".to_vec()
        } else {
            config.salt
        };
        Ok(Self {
            signer: Signer::with_keys(keys, config)?,
            timed,
            kind,
            default_max_age: None,
        })
    }

    pub fn set_default_max_age(&mut self, secs: Option<u64>) {
        self.default_max_age = secs;
    }

    fn encode_payload(&self, value: &JsonValue) -> Result<Vec<u8>, SignError> {
        let json = to_string(value);
        match self.kind {
            SerializerKind::Json => Ok(json.into_bytes()),
            SerializerKind::UrlSafe => {
                Ok(crate::encoding::b64_encode(json.as_bytes()).into_bytes())
            }
        }
    }

    fn decode_payload(&self, payload: &[u8]) -> Result<JsonValue, SignError> {
        let json_bytes = match self.kind {
            SerializerKind::Json => payload.to_vec(),
            SerializerKind::UrlSafe => {
                let s = std::str::from_utf8(payload).map_err(|_| SignError::BadFormat)?;
                b64_decode(s).map_err(|_| SignError::BadFormat)?
            }
        };
        let json_str = std::str::from_utf8(&json_bytes)
            .map_err(|_| SignError::BadPayload("invalid UTF-8".into()))?;
        parse(json_str).map_err(|e| SignError::BadPayload(e.to_string()))
    }

    fn timestamp_signer(&self) -> TimestampSigner {
        TimestampSigner::with_keys(self.signer.secret_keys.clone(), self.signer.config.clone())
            .expect("valid signer config")
    }

    /// Serialize a JSON value and sign it.
    pub fn dumps_json(&self, value: &JsonValue) -> Result<String, SignError> {
        let payload = self.encode_payload(value)?;
        if self.timed {
            self.timestamp_signer().sign_bytes(&payload)
        } else {
            self.signer.sign_bytes(&payload)
        }
    }

    /// Verify signature and deserialize JSON.
    pub fn loads_json(&self, token: &str, max_age: Option<u64>) -> Result<JsonValue, SignError> {
        let max_age = max_age.or(self.default_max_age);
        let payload = if self.timed {
            self.timestamp_signer().unsign_bytes(token, max_age)?.0
        } else {
            self.signer.unsign_bytes(token)?
        };
        self.decode_payload(&payload)
    }

    /// Loads without verifying signature; returns validity + payload.
    pub fn loads_unsafe_json(&self, token: &str, max_age: Option<u64>) -> UnsafeLoad<JsonValue> {
        let max_age = max_age.or(self.default_max_age);
        if self.timed {
            let ts = self.timestamp_signer();
            match ts.unsign_bytes(token, max_age) {
                Ok((payload, timestamp)) => match self.decode_payload(&payload) {
                    Ok(value) => UnsafeLoad {
                        valid: true,
                        value: Some(value),
                        timestamp: Some(timestamp),
                        expired: false,
                        error: None,
                    },
                    Err(e) => UnsafeLoad {
                        valid: false,
                        value: None,
                        timestamp: Some(timestamp),
                        expired: false,
                        error: Some(e.to_string()),
                    },
                },
                Err(SignError::Expired { .. }) => {
                    Self::unsafe_expired_payload(self, token, max_age)
                }
                Err(e) => Self::unsafe_bad_sig(self, token, &e),
            }
        } else {
            match self.signer.unsign_bytes(token) {
                Ok(payload) => match self.decode_payload(&payload) {
                    Ok(value) => UnsafeLoad {
                        valid: true,
                        value: Some(value),
                        timestamp: None,
                        expired: false,
                        error: None,
                    },
                    Err(e) => UnsafeLoad {
                        valid: false,
                        value: None,
                        timestamp: None,
                        expired: false,
                        error: Some(e.to_string()),
                    },
                },
                Err(e) => Self::unsafe_bad_sig(self, token, &e),
            }
        }
    }

    fn unsafe_expired_payload(&self, token: &str, max_age: Option<u64>) -> UnsafeLoad<JsonValue> {
        let ts = self.timestamp_signer();
        match ts.unsign_bytes(token, None) {
            Ok((payload, timestamp)) => {
                let expired = max_age
                    .map(|m| {
                        let age = TimestampSigner::now_secs() as i64 - timestamp as i64;
                        age > m as i64
                    })
                    .unwrap_or(false);
                match self.decode_payload(&payload) {
                    Ok(value) => UnsafeLoad {
                        valid: false,
                        value: Some(value),
                        timestamp: Some(timestamp),
                        expired,
                        error: Some("signature expired".into()),
                    },
                    Err(e) => UnsafeLoad {
                        valid: false,
                        value: None,
                        timestamp: Some(timestamp),
                        expired,
                        error: Some(e.to_string()),
                    },
                }
            }
            Err(e) => Self::unsafe_bad_sig(self, token, &e),
        }
    }

    fn unsafe_bad_sig(ser: &Self, token: &str, err: &SignError) -> UnsafeLoad<JsonValue> {
        // Try to extract payload from malformed token for debugging.
        let payload = extract_payload_guess(token, ser.timed, ser.signer.config().sep);
        let value = payload.and_then(|p| ser.decode_payload(&p).ok());
        UnsafeLoad {
            valid: false,
            value,
            timestamp: None,
            expired: false,
            error: Some(err.to_string()),
        }
    }
}

fn extract_payload_guess(token: &str, timed: bool, sep: u8) -> Option<Vec<u8>> {
    let bytes = token.as_bytes();
    let mut end = bytes.len();
    // Strip signature
    if let Some(p) = bytes[..end].iter().rposition(|&b| b == sep) {
        end = p;
    } else {
        return None;
    }
    if timed {
        if let Some(p) = bytes[..end].iter().rposition(|&b| b == sep) {
            end = p;
        }
    }
    Some(bytes[..end].to_vec())
}

/// Build config from optional parameters.
#[derive(Debug, Clone, Default)]
pub struct SerializerOptions {
    pub salt: Option<Vec<u8>>,
    pub sep: Option<u8>,
    pub digest: Option<Digest>,
    pub key_derivation: Option<KeyDerivation>,
    pub max_age: Option<u64>,
    pub max_payload: Option<usize>,
}

impl SerializerOptions {
    pub fn into_config(self) -> SignerConfig {
        let mut cfg = SignerConfig::default();
        if let Some(salt) = self.salt {
            cfg.salt = salt;
        }
        if let Some(sep) = self.sep {
            cfg.sep = sep;
        }
        if let Some(digest) = self.digest {
            cfg.digest = digest;
        }
        if let Some(kd) = self.key_derivation {
            cfg.key_derivation = kd;
        }
        if let Some(max) = self.max_payload {
            cfg.max_payload = max;
        }
        cfg
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_json_core::Value;

    #[test]
    fn json_serializer_roundtrip() {
        let ser = Serializer::new("secret", SignerConfig::default(), SerializerKind::Json).unwrap();
        let obj = parse(r#"{"id":42,"name":"test"}"#).unwrap();
        let tok = ser.dumps_json(&obj).unwrap();
        let out = ser.loads_json(&tok, None).unwrap();
        assert_eq!(out.get("id").and_then(|v| v.as_i64()), Some(42));
    }

    #[test]
    fn url_safe_roundtrip() {
        let ser =
            Serializer::timed("secret", SignerConfig::default(), SerializerKind::UrlSafe).unwrap();
        let obj = parse(r#"{"email":"a@b.com"}"#).unwrap();
        let tok = ser.dumps_json(&obj).unwrap();
        assert!(!tok.contains('+'));
        let out = ser.loads_json(&tok, Some(3600)).unwrap();
        assert_eq!(out.get("email").and_then(|v| v.as_str()), Some("a@b.com"));
    }

    #[test]
    fn loads_unsafe_invalid_sig() {
        let ser = Serializer::new("secret", SignerConfig::default(), SerializerKind::Json).unwrap();
        let obj = parse(r#"{"x":1}"#).unwrap();
        let mut tok = ser.dumps_json(&obj).unwrap();
        tok.pop();
        let r = ser.loads_unsafe_json(&tok, None);
        assert!(!r.valid);
    }
}
