//! Google Cloud authentication — service-account JWT (RS256) → OAuth2 access token.
//!
//! Uses `rsa` (already a `niao_runtime` dependency) for PKCS#1 v1.5 SHA-256 signing,
//! `niao_codec` for base64url, and `niao_http` for the token exchange. No new crates.

use niao_codec::base64;
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::DecodePrivateKey;
use rsa::sha2::Sha256;
use rsa::signature::{RandomizedSigner, SignatureEncoding};
use rsa::RsaPrivateKey;
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
const DEFAULT_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";

/// Normalise a PEM private key: expand literal `\n` sequences (common in SA JSON).
pub fn normalize_pem(pem: &str) -> String {
    if pem.contains("\\n") && !pem.contains('\n') {
        pem.replace("\\n", "\n")
    } else {
        pem.to_string()
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn b64url(data: &[u8]) -> String {
    base64::encode_url_safe_no_pad(data)
}

fn parse_private_key(pem: &str) -> Result<RsaPrivateKey, String> {
    let pem = normalize_pem(pem);
    RsaPrivateKey::from_pkcs8_pem(&pem)
        .or_else(|_| RsaPrivateKey::from_pkcs1_pem(&pem))
        .map_err(|e| format!("invalid RSA private key PEM: {e}"))
}

/// Build and RS256-sign a Google service-account JWT assertion.
///
/// Claims: `iss` = client_email, `scope`, `aud` = token_uri, `iat`/`exp` (1h).
pub fn sign_sa_jwt(
    client_email: &str,
    private_key_pem: &str,
    scope: &str,
    token_uri: &str,
    iat: u64,
) -> Result<String, String> {
    let header = r#"{"alg":"RS256","typ":"JWT"}"#;
    let scope = if scope.is_empty() { DEFAULT_SCOPE } else { scope };
    let aud = if token_uri.is_empty() {
        DEFAULT_TOKEN_URI
    } else {
        token_uri
    };
    let exp = iat + 3600;
    let payload = format!(
        "{{\"iss\":\"{}\",\"scope\":\"{}\",\"aud\":\"{}\",\"iat\":{},\"exp\":{}}}",
        json_escape(client_email),
        json_escape(scope),
        json_escape(aud),
        iat,
        exp
    );
    let signing_input = format!("{}.{}", b64url(header.as_bytes()), b64url(payload.as_bytes()));

    let key = parse_private_key(private_key_pem)?;
    let signing_key = SigningKey::<Sha256>::new(key);
    let mut rng = rand::thread_rng();
    let sig = signing_key.sign_with_rng(&mut rng, signing_input.as_bytes());
    Ok(format!("{}.{}", signing_input, b64url(sig.to_bytes().as_ref())))
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Exchange a signed JWT for an OAuth2 access token.
pub fn exchange_jwt(token_uri: &str, assertion: &str) -> Result<(String, u64), String> {
    let uri = if token_uri.is_empty() {
        DEFAULT_TOKEN_URI
    } else {
        token_uri
    };
    let body = format!(
        "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer&assertion={}",
        uri_encode_form(assertion)
    );
    let resp = niao_http::post(uri)
        .set("Content-Type", "application/x-www-form-urlencoded")
        .send_string(&body)
        .map_err(|e| format!("token exchange HTTP error: {e}"))?;
    if resp.status >= 400 {
        let msg = String::from_utf8_lossy(&resp.body);
        return Err(format!("token exchange failed (HTTP {}): {msg}", resp.status));
    }
    let text = String::from_utf8_lossy(&resp.body);
    let token = extract_json_string(&text, "access_token")
        .ok_or_else(|| "token exchange response missing access_token".to_string())?;
    let expires_in: u64 = extract_json_number(&text, "expires_in").unwrap_or(3600);
    Ok((token, now_secs() + expires_in.saturating_sub(60)))
}

/// Percent-encode for `application/x-www-form-urlencoded`.
pub fn uri_encode_form(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                const HEX: &[u8] = b"0123456789ABCDEF";
                out.push(HEX[(b >> 4) as usize] as char);
                out.push(HEX[(b & 0xf) as usize] as char);
            }
        }
    }
    out
}

/// URI-encode a path segment (encode `/`).
pub fn uri_encode_path(s: &str) -> String {
    uri_encode_form(s)
}

fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let start = json.find(&needle)?;
    let after = json[start + needle.len()..].trim_start();
    let after = after.strip_prefix(':')?.trim_start();
    let after = after.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = after.chars();
    while let Some(c) = chars.next() {
        if c == '"' {
            break;
        }
        if c == '\\' {
            match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => break,
            }
        } else {
            out.push(c);
        }
    }
    Some(out)
}

fn extract_json_number(json: &str, key: &str) -> Option<u64> {
    let needle = format!("\"{key}\"");
    let start = json.find(&needle)?;
    let after = json[start + needle.len()..].trim_start();
    let after = after.strip_prefix(':')?.trim_start();
    let end = after
        .find(|c: char| !(c.is_ascii_digit()))
        .unwrap_or(after.len());
    after[..end].parse().ok()
}

/// Obtain a Bearer access token from config fields.
pub fn obtain_access_token(
    access_token: &Option<String>,
    cached: &Option<(String, u64)>,
    client_email: &Option<String>,
    private_key: &Option<String>,
    token_uri: &str,
    scope: &str,
) -> Result<(String, Option<(String, u64)>), String> {
    if let Some(t) = access_token {
        if !t.is_empty() {
            return Ok((t.clone(), None));
        }
    }
    if let Some((tok, exp)) = cached {
        if now_secs() < *exp {
            return Ok((tok.clone(), None));
        }
    }
    let email = client_email
        .as_ref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            "ngcp: missing credentials — provide access_token or client_email+private_key"
                .to_string()
        })?;
    let pem = private_key
        .as_ref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "ngcp: missing private_key for service-account auth".to_string())?;
    let jwt = sign_sa_jwt(email, pem, scope, token_uri, now_secs())?;
    let (token, exp) = exchange_jwt(token_uri, &jwt)?;
    Ok((token.clone(), Some((token, exp))))
}

pub fn default_token_uri() -> &'static str {
    DEFAULT_TOKEN_URI
}

pub fn default_scope() -> &'static str {
    DEFAULT_SCOPE
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsa::traits::PublicKeyParts;
    use rsa::{RsaPrivateKey, RsaPublicKey};

    fn gen_pem() -> String {
        let mut rng = rand::thread_rng();
        let key = RsaPrivateKey::new(&mut rng, 2048).expect("keygen");
        use rsa::pkcs8::EncodePrivateKey;
        key.to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
            .unwrap()
            .to_string()
    }

    #[test]
    fn normalize_pem_expands_literal_newlines() {
        let s = "-----BEGIN PRIVATE KEY-----\\nABC\\n-----END PRIVATE KEY-----\\n";
        let n = normalize_pem(s);
        assert!(n.contains('\n'));
        assert!(!n.contains("\\n"));
    }

    #[test]
    fn sign_sa_jwt_produces_three_segments() {
        let pem = gen_pem();
        let jwt = sign_sa_jwt(
            "sa@example.iam.gserviceaccount.com",
            &pem,
            DEFAULT_SCOPE,
            DEFAULT_TOKEN_URI,
            1_600_000_000,
        )
        .unwrap();
        let parts: Vec<_> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3);
        assert!(!parts[0].is_empty());
        assert!(!parts[1].is_empty());
        assert!(!parts[2].is_empty());
    }

    #[test]
    fn sign_sa_jwt_verifies_with_public_key() {
        let mut rng = rand::thread_rng();
        let private = RsaPrivateKey::new(&mut rng, 2048).unwrap();
        let public = RsaPublicKey::from(&private);
        use rsa::pkcs8::EncodePrivateKey;
        let pem = private
            .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
            .unwrap()
            .to_string();
        let jwt = sign_sa_jwt("a@b.c", &pem, "s", "https://example.com/token", 100).unwrap();
        let parts: Vec<_> = jwt.split('.').collect();
        let signing_input = format!("{}.{}", parts[0], parts[1]);
        let sig_bytes = base64::decode_url_safe(parts[2]).unwrap();
        use rsa::pkcs1v15::{Signature, VerifyingKey};
        use rsa::signature::Verifier;
        let vk = VerifyingKey::<Sha256>::new(public.clone());
        let sig = Signature::try_from(sig_bytes.as_slice()).expect("sig");
        assert!(vk.verify(signing_input.as_bytes(), &sig).is_ok());
        assert!(public.n().bits() >= 2048);
    }

    #[test]
    fn uri_encode_form_encodes_specials() {
        assert_eq!(uri_encode_form("a b"), "a%20b");
        assert_eq!(uri_encode_form("a/b"), "a%2Fb");
        assert_eq!(uri_encode_form("safe-_.~"), "safe-_.~");
    }

    #[test]
    fn extract_json_string_basic() {
        let j = r#"{"access_token":"ya29.abc","expires_in":3600}"#;
        assert_eq!(extract_json_string(j, "access_token").as_deref(), Some("ya29.abc"));
        assert_eq!(extract_json_number(j, "expires_in"), Some(3600));
    }

    #[test]
    fn obtain_prefers_explicit_access_token() {
        let (tok, cache) = obtain_access_token(
            &Some("tok123".into()),
            &None,
            &None,
            &None,
            DEFAULT_TOKEN_URI,
            DEFAULT_SCOPE,
        )
        .unwrap();
        assert_eq!(tok, "tok123");
        assert!(cache.is_none());
    }

    #[test]
    fn obtain_missing_creds_errors() {
        let err = obtain_access_token(&None, &None, &None, &None, "", "").unwrap_err();
        assert!(err.contains("missing credentials"));
    }
}
