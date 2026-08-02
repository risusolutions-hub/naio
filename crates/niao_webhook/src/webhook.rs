//! Standard Webhooks HMAC-SHA256 sign / verify (symmetric `v1`).

use crate::error::{WebhookError, WebhookResult};
use crate::secret::{
    parse_secret, SecretFormat, DEFAULT_TOLERANCE_SECS, HDR_ID, HDR_SIGNATURE, HDR_TIMESTAMP,
};
use crate::timestamp::{now_secs, verify_timestamp_header};
use niao_codec::base64::{decode_standard, encode_standard};
use niao_crypto::{constant_time_eq, hmac_sha256};
use niao_json_core::{parse as parse_json, Value as JsonValue};
use niao_rand::fill_os_random;
use std::collections::HashMap;

/// Verified webhook message.
#[derive(Debug, Clone)]
pub struct Verified {
    pub id: String,
    pub timestamp: i64,
    pub payload: String,
    pub json: Option<JsonValue>,
}

/// Options for verify / valid.
#[derive(Debug, Clone)]
pub struct VerifyOptions {
    pub tolerance: i64,
    /// Override wall-clock for tests (unix seconds). `None` = now.
    pub now: Option<i64>,
    /// If true, parse payload as JSON (default). Empty payload → `json: None`.
    pub parse_json: bool,
}

impl Default for VerifyOptions {
    fn default() -> Self {
        Self {
            tolerance: DEFAULT_TOLERANCE_SECS,
            now: None,
            parse_json: true,
        }
    }
}

/// Options for constructing a [`Webhook`].
#[derive(Debug, Clone, Default)]
pub struct WebhookOptions {
    pub format: SecretFormat,
}

/// Standard Webhooks signer/verifier (one or more HMAC keys for rotation).
#[derive(Debug, Clone)]
pub struct Webhook {
    keys: Vec<Vec<u8>>,
}

impl Webhook {
    /// Create from a single secret string (`whsec_…` or bare base64, or raw).
    pub fn new(secret: &str, opts: WebhookOptions) -> WebhookResult<Self> {
        let key = parse_secret(secret, opts.format)?;
        Ok(Self { keys: vec![key] })
    }

    /// Create from already-decoded key bytes.
    pub fn from_key(key: Vec<u8>) -> WebhookResult<Self> {
        if key.is_empty() {
            return Err(WebhookError::EmptySecret);
        }
        Ok(Self { keys: vec![key] })
    }

    /// Multiple secrets for consumer-side key rotation (try each).
    pub fn with_secrets(secrets: &[&str], format: SecretFormat) -> WebhookResult<Self> {
        if secrets.is_empty() {
            return Err(WebhookError::EmptySecret);
        }
        let mut keys = Vec::with_capacity(secrets.len());
        for s in secrets {
            keys.push(parse_secret(s, format)?);
        }
        Ok(Self { keys })
    }

    /// Sign `msg_id.timestamp.payload` → `v1,<base64>`.
    pub fn sign(&self, msg_id: &str, timestamp: i64, payload: &str) -> WebhookResult<String> {
        if msg_id.is_empty() {
            return Err(WebhookError::BadArgument("msg_id must not be empty".into()));
        }
        let key = self.keys.last().ok_or(WebhookError::EmptySecret)?;
        let sig = sign_with_key(key, msg_id, timestamp, payload);
        Ok(format!("v1,{sig}"))
    }

    /// Verify headers + payload. On success returns [`Verified`].
    pub fn verify(
        &self,
        payload: &str,
        headers: &HashMap<String, String>,
        opts: &VerifyOptions,
    ) -> WebhookResult<Verified> {
        let (id, ts, _) = self.verify_inner(payload, headers, opts)?;
        let json = if opts.parse_json {
            if payload.is_empty() {
                None
            } else {
                Some(parse_json(payload).map_err(|e| WebhookError::InvalidJson(e.to_string()))?)
            }
        } else {
            None
        };
        Ok(Verified {
            id,
            timestamp: ts,
            payload: payload.to_string(),
            json,
        })
    }

    /// Verify without JSON parsing; returns the raw payload string.
    pub fn verify_raw(
        &self,
        payload: &str,
        headers: &HashMap<String, String>,
        opts: &VerifyOptions,
    ) -> WebhookResult<Verified> {
        let mut o = opts.clone();
        o.parse_json = false;
        self.verify(payload, headers, &o)
    }

    /// Boolean check (no JSON parse).
    pub fn valid(
        &self,
        payload: &str,
        headers: &HashMap<String, String>,
        opts: &VerifyOptions,
    ) -> bool {
        let mut o = opts.clone();
        o.parse_json = false;
        self.verify_inner(payload, headers, &o).is_ok()
    }

    fn verify_inner(
        &self,
        payload: &str,
        headers: &HashMap<String, String>,
        opts: &VerifyOptions,
    ) -> WebhookResult<(String, i64, String)> {
        let hdrs = lower_headers(headers);
        let msg_id = hdrs
            .get(HDR_ID)
            .filter(|s| !s.is_empty())
            .ok_or(WebhookError::MissingHeaders)?;
        let msg_sig = hdrs
            .get(HDR_SIGNATURE)
            .filter(|s| !s.is_empty())
            .ok_or(WebhookError::MissingHeaders)?;
        let msg_ts = hdrs
            .get(HDR_TIMESTAMP)
            .filter(|s| !s.is_empty())
            .ok_or(WebhookError::MissingHeaders)?;

        let now = opts.now.unwrap_or_else(now_secs);
        let timestamp = verify_timestamp_header(msg_ts, now, opts.tolerance)?;

        // Compare against each key (rotation) × each v1 signature in the header.
        let passed = msg_sig.split_whitespace();
        let mut any_v1 = false;
        for versioned in passed {
            let (version, signature) = match versioned.split_once(',') {
                Some(p) => p,
                None => continue,
            };
            if version != "v1" {
                continue;
            }
            any_v1 = true;
            let expected_bytes = match decode_standard(signature) {
                Ok(b) => b,
                Err(_) => continue,
            };
            for key in &self.keys {
                let computed = hmac_sha256(key, &to_sign_bytes(msg_id, timestamp, payload));
                if constant_time_eq(&computed, &expected_bytes) {
                    return Ok((msg_id.clone(), timestamp, msg_sig.clone()));
                }
            }
        }
        if !any_v1 {
            return Err(WebhookError::InvalidSignatureHeader);
        }
        Err(WebhookError::NoMatchingSignature)
    }
}

#[inline]
fn to_sign_bytes(msg_id: &str, timestamp: i64, payload: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(msg_id.len() + 1 + 20 + 1 + payload.len());
    out.extend_from_slice(msg_id.as_bytes());
    out.push(b'.');
    out.extend_from_slice(timestamp.to_string().as_bytes());
    out.push(b'.');
    out.extend_from_slice(payload.as_bytes());
    out
}

fn sign_with_key(key: &[u8], msg_id: &str, timestamp: i64, payload: &str) -> String {
    let mac = hmac_sha256(key, &to_sign_bytes(msg_id, timestamp, payload));
    encode_standard(&mac)
}

fn lower_headers(headers: &HashMap<String, String>) -> HashMap<String, String> {
    let mut out = HashMap::with_capacity(headers.len());
    for (k, v) in headers {
        out.insert(k.to_ascii_lowercase(), v.clone());
    }
    out
}

/// Build the three Standard Webhooks headers.
pub fn make_headers(msg_id: &str, timestamp: i64, signature: &str) -> HashMap<String, String> {
    let mut m = HashMap::with_capacity(3);
    m.insert(HDR_ID.to_string(), msg_id.to_string());
    m.insert(HDR_TIMESTAMP.to_string(), timestamp.to_string());
    m.insert(HDR_SIGNATURE.to_string(), signature.to_string());
    m
}

/// Generate a Svix-style message id: `msg_` + 24 URL-safe chars.
pub fn new_msg_id() -> String {
    // 18 random bytes → 24 base64url chars (no pad).
    let mut buf = [0u8; 18];
    fill_os_random(&mut buf);
    let enc = niao_codec::base64::encode_url_safe_no_pad(&buf);
    format!("msg_{enc}")
}

/// Sign a request for sending: allocates id + timestamp + signature + headers.
pub fn sign_request(
    wh: &Webhook,
    payload: &str,
    msg_id: Option<&str>,
    timestamp: Option<i64>,
) -> WebhookResult<SignRequest> {
    let id = match msg_id {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => new_msg_id(),
    };
    let ts = timestamp.unwrap_or_else(now_secs);
    let signature = wh.sign(&id, ts, payload)?;
    let headers = make_headers(&id, ts, &signature);
    Ok(SignRequest {
        id,
        timestamp: ts,
        signature,
        headers,
        payload: payload.to_string(),
    })
}

/// Result of [`sign_request`].
#[derive(Debug, Clone)]
pub struct SignRequest {
    pub id: String,
    pub timestamp: i64,
    pub signature: String,
    pub headers: HashMap<String, String>,
    pub payload: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Official Standard Webhooks / Svix vector from JS SDK tests.
    const SECRET_B64: &str = "MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw";
    const MSG_ID: &str = "msg_p5jXN8AQM9LWM0D4loKWxJek";
    const TS: i64 = 1_614_265_330;
    const PAYLOAD: &str = r#"{"test": 2432232314}"#;
    const EXPECTED_SIG: &str = "v1,g0hM9SsE+OTPJTGt/tmIKtSyZlE3uFJELVlNIOLJ1OE=";

    #[test]
    fn official_sign_vector() {
        let wh = Webhook::new(&format!("whsec_{SECRET_B64}"), WebhookOptions::default()).unwrap();
        let got = wh.sign(MSG_ID, TS, PAYLOAD).unwrap();
        assert_eq!(got, EXPECTED_SIG);
    }

    #[test]
    fn verify_roundtrip() {
        let wh = Webhook::new(SECRET_B64, WebhookOptions::default()).unwrap();
        let now = now_secs();
        let sig = wh.sign(MSG_ID, now, PAYLOAD).unwrap();
        let headers = make_headers(MSG_ID, now, &sig);
        let opts = VerifyOptions {
            now: Some(now),
            ..Default::default()
        };
        let v = wh.verify(PAYLOAD, &headers, &opts).unwrap();
        assert_eq!(v.id, MSG_ID);
        assert!(v.json.is_some());
    }

    #[test]
    fn multi_sig_header() {
        let wh = Webhook::new(SECRET_B64, WebhookOptions::default()).unwrap();
        let now = now_secs();
        let good = wh.sign(MSG_ID, now, PAYLOAD).unwrap();
        let combined = format!("v1,Ceo5qEr07ixe2NLpvHk3FH9bwy/WavXrAFQ/9tdO6mc= v2,x {good}");
        let mut headers = make_headers(MSG_ID, now, &combined);
        let opts = VerifyOptions {
            now: Some(now),
            ..Default::default()
        };
        assert!(wh.verify(PAYLOAD, &headers, &opts).is_ok());
        headers.insert(HDR_SIGNATURE.to_string(), "v1,bad".into());
        assert!(wh.verify(PAYLOAD, &headers, &opts).is_err());
    }

    #[test]
    fn tolerance_rejects_old() {
        let wh = Webhook::new(SECRET_B64, WebhookOptions::default()).unwrap();
        let old = now_secs() - 400;
        let sig = wh.sign(MSG_ID, old, PAYLOAD).unwrap();
        let headers = make_headers(MSG_ID, old, &sig);
        let opts = VerifyOptions::default();
        assert!(matches!(
            wh.verify(PAYLOAD, &headers, &opts),
            Err(WebhookError::TimestampTooOld)
        ));
    }

    #[test]
    fn empty_json_payload() {
        let wh = Webhook::new(SECRET_B64, WebhookOptions::default()).unwrap();
        let now = now_secs();
        let payload = "";
        let sig = wh.sign(MSG_ID, now, payload).unwrap();
        let headers = make_headers(MSG_ID, now, &sig);
        let opts = VerifyOptions {
            now: Some(now),
            ..Default::default()
        };
        let v = wh.verify(payload, &headers, &opts).unwrap();
        assert!(v.json.is_none());
    }
}
