//! Double-submit CSRF tokens bound to a session id via HMAC-SHA256.

use crate::error::{AuthError, AuthResult};
use crate::token::{compare, generate_token};
use niao_codec::base64::encode_url_safe_no_pad;
use niao_crypto::hmac::hmac_sha256;

const CSRF_SALT: &[u8] = b"nauth.csrf";

/// Issue a CSRF token: `nonce.mac` where mac = HMAC-SHA256(secret, salt|sid|nonce).
pub fn issue(secret: &[u8], session_id: &str) -> AuthResult<String> {
    let nonce = generate_token(16)?;
    let mac = mac_hex(secret, session_id, &nonce);
    Ok(format!("{nonce}.{mac}"))
}

/// Validate a CSRF token for the given session id (constant-time).
pub fn validate(secret: &[u8], session_id: &str, token: &str) -> bool {
    let Some((nonce, mac)) = token.split_once('.') else {
        return false;
    };
    if nonce.is_empty() || mac.is_empty() {
        return false;
    }
    let expected = mac_hex(secret, session_id, nonce);
    compare(mac, &expected)
}

fn mac_hex(secret: &[u8], session_id: &str, nonce: &str) -> String {
    let mut msg = Vec::with_capacity(CSRF_SALT.len() + session_id.len() + nonce.len() + 2);
    msg.extend_from_slice(CSRF_SALT);
    msg.push(b'|');
    msg.extend_from_slice(session_id.as_bytes());
    msg.push(b'|');
    msg.extend_from_slice(nonce.as_bytes());
    let dig = hmac_sha256(secret, &msg);
    encode_url_safe_no_pad(&dig)
}

/// Reject empty session ids for CSRF binding.
pub fn require_session_id(sid: &str) -> AuthResult<()> {
    if sid.is_empty() {
        Err(AuthError::InvalidParameter(
            "CSRF requires a non-empty session id".into(),
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let secret = b"csrf-secret-key-32bytes-minimum!!";
        let tok = issue(secret, "sid-1").unwrap();
        assert!(validate(secret, "sid-1", &tok));
        assert!(!validate(secret, "sid-2", &tok));
        assert!(!validate(secret, "sid-1", "bad.token"));
    }

    #[test]
    fn tamper_fails() {
        let secret = b"csrf-secret-key-32bytes-minimum!!";
        let tok = issue(secret, "sid").unwrap();
        let mut bad = tok.clone();
        bad.push('x');
        assert!(!validate(secret, "sid", &bad));
    }
}
