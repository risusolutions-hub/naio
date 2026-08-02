//! HMAC signers — `Signer` and `TimestampSigner` (itsdangerous-compatible).

use crate::encoding::{b64_decode, b64_encode, bytes_to_int, int_to_bytes};
use crate::error::SignError;
use niao_crypto::ct::eq as ct_eq;
use niao_crypto::hmac::{hmac_sha256, hmac_sha512};
use niao_crypto::sha1::Sha1;
use std::time::{SystemTime, UNIX_EPOCH};

const BASE64_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_=";

/// Default max signed payload size (1 MiB).
pub const DEFAULT_MAX_PAYLOAD: usize = 1_048_576;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Digest {
    Sha1,
    Sha256,
    Sha512,
}

impl Digest {
    pub fn from_name(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "sha1" | "sha-1" => Some(Self::Sha1),
            "sha256" | "sha-256" => Some(Self::Sha256),
            "sha512" | "sha-512" => Some(Self::Sha512),
            _ => None,
        }
    }

    fn hmac(&self, key: &[u8], data: &[u8]) -> Vec<u8> {
        match self {
            Self::Sha1 => hmac_sha1(key, data).to_vec(),
            Self::Sha256 => hmac_sha256(key, data).to_vec(),
            Self::Sha512 => hmac_sha512(key, data).to_vec(),
        }
    }

    fn hash_digest(&self, data: &[u8]) -> Vec<u8> {
        match self {
            Self::Sha1 => {
                let mut h = Sha1::new();
                h.update(data);
                h.finalize().to_vec()
            }
            Self::Sha256 => niao_crypto::sha256(data).to_vec(),
            Self::Sha512 => niao_crypto::sha512(data).to_vec(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyDerivation {
    DjangoConcat,
    Concat,
    Hmac,
    None,
}

impl KeyDerivation {
    pub fn from_name(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "django-concat" | "django_concat" | "django" => Some(Self::DjangoConcat),
            "concat" => Some(Self::Concat),
            "hmac" => Some(Self::Hmac),
            "none" => Some(Self::None),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SignerConfig {
    pub salt: Vec<u8>,
    pub sep: u8,
    pub digest: Digest,
    pub key_derivation: KeyDerivation,
    pub max_payload: usize,
}

impl Default for SignerConfig {
    fn default() -> Self {
        Self {
            salt: b"itsdangerous.Signer".to_vec(),
            sep: b'.',
            digest: Digest::Sha1,
            key_derivation: KeyDerivation::DjangoConcat,
            max_payload: DEFAULT_MAX_PAYLOAD,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Signer {
    pub(crate) secret_keys: Vec<Vec<u8>>,
    pub(crate) config: SignerConfig,
    /// Cached derived keys per secret (newest last).
    derived_keys: Vec<Vec<u8>>,
}

impl Signer {
    pub fn new(secret_key: impl AsRef<[u8]>, config: SignerConfig) -> Result<Self, SignError> {
        if BASE64_ALPHABET.contains(&config.sep) {
            return Err(SignError::InvalidSeparator);
        }
        let secret_keys = vec![secret_key.as_ref().to_vec()];
        let derived_keys = secret_keys.iter().map(|k| derive_key(k, &config)).collect();
        Ok(Self {
            secret_keys,
            config,
            derived_keys,
        })
    }

    pub fn with_keys(secret_keys: Vec<Vec<u8>>, config: SignerConfig) -> Result<Self, SignError> {
        if secret_keys.is_empty() {
            return Err(SignError::BadFormat);
        }
        if BASE64_ALPHABET.contains(&config.sep) {
            return Err(SignError::InvalidSeparator);
        }
        let derived_keys = secret_keys.iter().map(|k| derive_key(k, &config)).collect();
        Ok(Self {
            secret_keys,
            config,
            derived_keys,
        })
    }

    pub fn config(&self) -> &SignerConfig {
        &self.config
    }

    fn signing_key(&self) -> &[u8] {
        self.derived_keys.last().expect("at least one key")
    }

    fn get_signature(&self, value: &[u8]) -> String {
        let sig = self.config.digest.hmac(self.signing_key(), value);
        b64_encode(&sig)
    }

    fn verify_signature(&self, value: &[u8], sig_b64: &str) -> bool {
        let Ok(sig) = b64_decode(sig_b64) else {
            return false;
        };
        for derived in self.derived_keys.iter().rev() {
            let expected = self.config.digest.hmac(derived, value);
            if ct_eq(&expected, &sig) {
                return true;
            }
        }
        false
    }

    /// Sign a byte payload: `value + sep + base64url(hmac(value))`.
    pub fn sign_bytes(&self, value: &[u8]) -> Result<String, SignError> {
        if value.len() > self.config.max_payload {
            return Err(SignError::PayloadTooLarge);
        }
        let sig = self.get_signature(value);
        let mut out = Vec::with_capacity(value.len() + 1 + sig.len());
        out.extend_from_slice(value);
        out.push(self.config.sep);
        out.extend_from_slice(sig.as_bytes());
        Ok(String::from_utf8(out).expect("ascii signature"))
    }

    /// Sign a UTF-8 string.
    pub fn sign(&self, value: &str) -> Result<String, SignError> {
        self.sign_bytes(value.as_bytes())
    }

    /// Unsign and return the original bytes.
    pub fn unsign_bytes(&self, signed: &str) -> Result<Vec<u8>, SignError> {
        let sep = self.config.sep;
        let bytes = signed.as_bytes();
        let Some(pos) = bytes.iter().rposition(|&b| b == sep) else {
            return Err(SignError::BadFormat);
        };
        let value = &bytes[..pos];
        let sig = std::str::from_utf8(&bytes[pos + 1..]).map_err(|_| SignError::BadFormat)?;
        if !self.verify_signature(value, sig) {
            return Err(SignError::BadSignature);
        }
        if value.len() > self.config.max_payload {
            return Err(SignError::PayloadTooLarge);
        }
        Ok(value.to_vec())
    }

    /// Unsign and return a UTF-8 string.
    pub fn unsign(&self, signed: &str) -> Result<String, SignError> {
        let bytes = self.unsign_bytes(signed)?;
        String::from_utf8(bytes).map_err(|_| SignError::BadPayload("invalid UTF-8".into()))
    }

    /// Check signature validity without returning payload.
    pub fn validate(&self, signed: &str) -> bool {
        self.unsign_bytes(signed).is_ok()
    }
}

#[derive(Debug, Clone)]
pub struct TimestampSigner {
    inner: Signer,
}

impl TimestampSigner {
    pub fn new(secret_key: impl AsRef<[u8]>, config: SignerConfig) -> Result<Self, SignError> {
        Ok(Self {
            inner: Signer::new(secret_key, config)?,
        })
    }

    pub fn with_keys(secret_keys: Vec<Vec<u8>>, config: SignerConfig) -> Result<Self, SignError> {
        Ok(Self {
            inner: Signer::with_keys(secret_keys, config)?,
        })
    }

    pub fn config(&self) -> &SignerConfig {
        self.inner.config()
    }

    pub fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// Sign with embedded timestamp: `value.sep.b64(ts).sep.sig`.
    pub fn sign_bytes_at(&self, value: &[u8], timestamp: u64) -> Result<String, SignError> {
        if value.len() > self.inner.config.max_payload {
            return Err(SignError::PayloadTooLarge);
        }
        let sep = self.inner.config.sep;
        let ts_b64 = b64_encode(&int_to_bytes(timestamp));
        let mut to_sign = Vec::with_capacity(value.len() + 1 + ts_b64.len());
        to_sign.extend_from_slice(value);
        to_sign.push(sep);
        to_sign.extend_from_slice(ts_b64.as_bytes());
        let sig = self.inner.get_signature(&to_sign);
        let mut out = to_sign;
        out.push(sep);
        out.extend_from_slice(sig.as_bytes());
        Ok(String::from_utf8(out).expect("ascii"))
    }

    pub fn sign_bytes(&self, value: &[u8]) -> Result<String, SignError> {
        self.sign_bytes_at(value, Self::now_secs())
    }

    pub fn sign(&self, value: &str) -> Result<String, SignError> {
        self.sign_bytes(value.as_bytes())
    }

    pub fn sign_at(&self, value: &str, timestamp: u64) -> Result<String, SignError> {
        self.sign_bytes_at(value.as_bytes(), timestamp)
    }

    /// Unsign with optional max_age (seconds). Returns (payload, timestamp).
    pub fn unsign_bytes(
        &self,
        signed: &str,
        max_age: Option<u64>,
    ) -> Result<(Vec<u8>, u64), SignError> {
        let sep = self.inner.config.sep;
        let bytes = signed.as_bytes();
        let Some(sig_pos) = bytes.iter().rposition(|&b| b == sep) else {
            return Err(SignError::BadFormat);
        };
        let without_sig = &bytes[..sig_pos];
        let sig = std::str::from_utf8(&bytes[sig_pos + 1..]).map_err(|_| SignError::BadFormat)?;

        let sig_ok = self.inner.verify_signature(without_sig, sig);

        let Some(ts_sep_pos) = without_sig.iter().rposition(|&b| b == sep) else {
            if !sig_ok {
                return Err(SignError::BadSignature);
            }
            return Err(SignError::TimestampMissing);
        };

        let value = &without_sig[..ts_sep_pos];
        let ts_b64 = std::str::from_utf8(&without_sig[ts_sep_pos + 1..])
            .map_err(|_| SignError::MalformedTimestamp)?;
        let ts_bytes = b64_decode(ts_b64).map_err(|_| SignError::MalformedTimestamp)?;
        let timestamp = bytes_to_int(&ts_bytes).map_err(|_| SignError::MalformedTimestamp)?;

        if !sig_ok {
            return Err(SignError::BadSignature);
        }

        if let Some(max) = max_age {
            let now = Self::now_secs();
            let age = now as i64 - timestamp as i64;
            if age > max as i64 || age < 0 {
                return Err(SignError::Expired {
                    age_secs: age,
                    max_age: max,
                });
            }
        }

        if value.len() > self.inner.config.max_payload {
            return Err(SignError::PayloadTooLarge);
        }

        Ok((value.to_vec(), timestamp))
    }

    pub fn unsign(&self, signed: &str, max_age: Option<u64>) -> Result<(String, u64), SignError> {
        let (bytes, ts) = self.unsign_bytes(signed, max_age)?;
        let s =
            String::from_utf8(bytes).map_err(|_| SignError::BadPayload("invalid UTF-8".into()))?;
        Ok((s, ts))
    }

    pub fn validate(&self, signed: &str, max_age: Option<u64>) -> bool {
        self.unsign_bytes(signed, max_age).is_ok()
    }
}

fn derive_key(secret: &[u8], config: &SignerConfig) -> Vec<u8> {
    match config.key_derivation {
        KeyDerivation::Concat => config
            .digest
            .hash_digest(&concat_bytes(&config.salt, secret)),
        KeyDerivation::DjangoConcat => {
            let mut buf = Vec::with_capacity(config.salt.len() + 6 + secret.len());
            buf.extend_from_slice(&config.salt);
            buf.extend_from_slice(b"signer");
            buf.extend_from_slice(secret);
            config.digest.hash_digest(&buf)
        }
        KeyDerivation::Hmac => config.digest.hmac(secret, &config.salt),
        KeyDerivation::None => secret.to_vec(),
    }
}

fn concat_bytes(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(a.len() + b.len());
    v.extend_from_slice(a);
    v.extend_from_slice(b);
    v
}

/// HMAC-SHA1 (itsdangerous default digest).
fn hmac_sha1(key: &[u8], data: &[u8]) -> [u8; 20] {
    const BLOCK: usize = 64;
    let k = normalize_key_sha1(key);
    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let mut inner = Sha1::new();
    inner.update(&ipad);
    inner.update(data);
    let inner_digest = inner.finalize();
    let mut outer = Sha1::new();
    outer.update(&opad);
    outer.update(&inner_digest);
    outer.finalize()
}

fn normalize_key_sha1(key: &[u8]) -> [u8; 64] {
    let mut out = [0u8; 64];
    if key.len() > 64 {
        let mut h = Sha1::new();
        h.update(key);
        let digest = h.finalize();
        out[..20].copy_from_slice(&digest);
    } else {
        out[..key.len()].copy_from_slice(key);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_signer(secret: &str) -> Signer {
        Signer::new(secret.as_bytes(), SignerConfig::default()).unwrap()
    }

    #[test]
    fn sign_unsign_roundtrip() {
        let s = default_signer("secret-key");
        let signed = s.sign("my string").unwrap();
        assert!(signed.contains('.'));
        assert_eq!(s.unsign(&signed).unwrap(), "my string");
    }

    #[test]
    fn bad_signature_rejected() {
        let s = default_signer("secret-key");
        let signed = s.sign("data").unwrap();
        let mut tampered = signed.clone();
        tampered.pop();
        tampered.push('X');
        assert_eq!(s.unsign(&tampered), Err(SignError::BadSignature));
    }

    #[test]
    fn timestamp_sign_unsign() {
        let ts = TimestampSigner::new("secret", SignerConfig::default()).unwrap();
        let signed = ts.sign_at("payload", 1_700_000_000).unwrap();
        let (val, got_ts) = ts.unsign(&signed, None).unwrap();
        assert_eq!(val, "payload");
        assert_eq!(got_ts, 1_700_000_000);
    }

    #[test]
    fn timestamp_expired() {
        let ts = TimestampSigner::new("secret", SignerConfig::default()).unwrap();
        let old = 1_000_000_000u64;
        let signed = ts.sign_at("x", old).unwrap();
        let err = ts.unsign(&signed, Some(60)).unwrap_err();
        assert!(matches!(err, SignError::Expired { .. }));
    }

    #[test]
    fn key_rotation() {
        let old_key = b"old-secret".to_vec();
        let new_key = b"new-secret".to_vec();
        let config = SignerConfig::default();
        let signer =
            Signer::with_keys(vec![old_key.clone(), new_key.clone()], config.clone()).unwrap();
        let token = signer.sign("rotate").unwrap();
        let verifier = Signer::with_keys(vec![old_key, new_key], config).unwrap();
        assert_eq!(verifier.unsign(&token).unwrap(), "rotate");
    }
}
