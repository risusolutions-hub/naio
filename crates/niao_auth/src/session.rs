//! Signed session payloads via `niao_sign` URL-safe timed serializer.

use crate::error::{AuthError, AuthResult};
use crate::token::generate_token;
use niao_json_core::{Object, Value};
use niao_sign::{format_set_cookie, Serializer, SerializerKind, SignerConfig};

/// Default session lifetime (24h).
pub const DEFAULT_SESSION_LIFETIME: u64 = 86_400;
/// Default cookie name.
pub const DEFAULT_COOKIE_NAME: &str = "session";
/// Salt for session signer.
const SESSION_SALT: &[u8] = b"nauth.session";

#[derive(Debug, Clone)]
pub struct SessionData {
    pub session_id: String,
    pub user_id: String,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
    pub data: Object,
}

impl SessionData {
    pub fn new(user_id: impl Into<String>) -> Self {
        Self {
            session_id: generate_token(16).unwrap_or_else(|_| "sid".into()),
            user_id: user_id.into(),
            roles: Vec::new(),
            permissions: Vec::new(),
            data: Object::new(),
        }
    }

    pub fn to_json(&self) -> Value {
        let mut obj = Object::new();
        obj.insert("sid".into(), Value::string(&self.session_id));
        obj.insert("uid".into(), Value::string(&self.user_id));
        obj.insert(
            "roles".into(),
            Value::array(
                self.roles
                    .iter()
                    .map(|r| Value::string(r.as_str()))
                    .collect(),
            ),
        );
        obj.insert(
            "perms".into(),
            Value::array(
                self.permissions
                    .iter()
                    .map(|p| Value::string(p.as_str()))
                    .collect(),
            ),
        );
        obj.insert("data".into(), Value::object(self.data.clone()));
        Value::object(obj)
    }

    pub fn from_json(v: &Value) -> AuthResult<Self> {
        let obj = v
            .as_object()
            .ok_or_else(|| AuthError::BadSession("payload must be object".into()))?;
        let user_id = obj
            .get("uid")
            .and_then(|x| x.as_str())
            .ok_or_else(|| AuthError::BadSession("missing uid".into()))?
            .to_string();
        let session_id = obj
            .get("sid")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let roles = string_list(obj.get("roles"));
        let permissions = string_list(obj.get("perms"));
        let data = match obj.get("data") {
            Some(Value::Object(o)) => o.clone(),
            _ => Object::new(),
        };
        Ok(Self {
            session_id,
            user_id,
            roles,
            permissions,
            data,
        })
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.data.get(key)
    }

    pub fn set(&mut self, key: impl Into<String>, value: Value) {
        self.data.insert(key.into(), value);
    }
}

fn string_list(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|i| i.as_str().map(|s| s.to_string()))
            .collect(),
        _ => Vec::new(),
    }
}

fn session_config() -> SignerConfig {
    let mut c = SignerConfig::default();
    c.salt = SESSION_SALT.to_vec();
    c
}

/// Sign a session into a URL-safe timed token.
pub fn sign_session(secret: &[u8], session: &SessionData) -> AuthResult<String> {
    let ser = Serializer::timed(secret, session_config(), SerializerKind::UrlSafe)?;
    Ok(ser.dumps_json(&session.to_json())?)
}

/// Unsign and load a session token.
pub fn load_session(secret: &[u8], token: &str, max_age: Option<u64>) -> AuthResult<SessionData> {
    let ser = Serializer::timed(secret, session_config(), SerializerKind::UrlSafe)?;
    let json = ser.loads_json(token, max_age)?;
    SessionData::from_json(&json)
}

/// Build a Set-Cookie header for a signed session token.
pub fn session_cookie(
    name: &str,
    token: &str,
    max_age: Option<u64>,
    path: &str,
    http_only: bool,
    secure: bool,
    same_site: Option<&str>,
) -> String {
    format_set_cookie(name, token, max_age, path, http_only, secure, same_site)
}

/// Clear-cookie header (Max-Age=0).
pub fn clear_cookie(name: &str, path: &str) -> String {
    format!("{name}=; Path={path}; Max-Age=0; HttpOnly; SameSite=Lax")
}

/// Extract `name=value` from a Cookie header (first match).
pub fn extract_cookie(cookie_header: &str, name: &str) -> Option<String> {
    for part in cookie_header.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix(name) {
            if let Some(val) = rest.strip_prefix('=') {
                return Some(val.to_string());
            }
        }
    }
    None
}

/// Password-reset token salt.
const RESET_SALT: &[u8] = b"nauth.reset";

/// Default reset token max age (1 hour).
pub const DEFAULT_RESET_MAX_AGE: u64 = 3_600;

fn reset_config() -> SignerConfig {
    let mut c = SignerConfig::default();
    c.salt = RESET_SALT.to_vec();
    c
}

/// Issue a timed password-reset token for `user_id`.
pub fn issue_reset_token(secret: &[u8], user_id: &str) -> AuthResult<String> {
    if user_id.is_empty() {
        return Err(AuthError::InvalidParameter(
            "user_id must be non-empty".into(),
        ));
    }
    let mut obj = Object::new();
    obj.insert("uid".into(), Value::string(user_id));
    obj.insert("purpose".into(), Value::string("reset"));
    let ser = Serializer::timed(secret, reset_config(), SerializerKind::UrlSafe)?;
    Ok(ser.dumps_json(&Value::object(obj))?)
}

/// Verify a reset token; returns user_id.
pub fn verify_reset_token(secret: &[u8], token: &str, max_age: Option<u64>) -> AuthResult<String> {
    let ser = Serializer::timed(secret, reset_config(), SerializerKind::UrlSafe)?;
    let json = ser.loads_json(token, max_age.or(Some(DEFAULT_RESET_MAX_AGE)))?;
    let purpose = json.get("purpose").and_then(|v| v.as_str()).unwrap_or("");
    if purpose != "reset" {
        return Err(AuthError::BadSession("not a reset token".into()));
    }
    json.get("uid")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| AuthError::BadSession("missing uid in reset token".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_roundtrip() {
        let secret = b"session-secret-key-32bytes-long!!";
        let mut s = SessionData::new("alice");
        s.roles = vec!["admin".into()];
        s.set("flash", Value::string("hi"));
        let tok = sign_session(secret, &s).unwrap();
        let loaded = load_session(secret, &tok, Some(3600)).unwrap();
        assert_eq!(loaded.user_id, "alice");
        assert_eq!(loaded.roles, vec!["admin".to_string()]);
        assert_eq!(loaded.get("flash").and_then(|v| v.as_str()), Some("hi"));
    }

    #[test]
    fn reset_roundtrip() {
        let secret = b"session-secret-key-32bytes-long!!";
        let tok = issue_reset_token(secret, "bob").unwrap();
        assert_eq!(verify_reset_token(secret, &tok, Some(3600)).unwrap(), "bob");
    }

    #[test]
    fn extract_cookie_value() {
        let h = "foo=1; session=abc.def; bar=2";
        assert_eq!(extract_cookie(h, "session").as_deref(), Some("abc.def"));
        assert!(extract_cookie(h, "missing").is_none());
    }

    #[test]
    fn bad_signature_fails() {
        let secret = b"session-secret-key-32bytes-long!!";
        let s = SessionData::new("x");
        let tok = sign_session(secret, &s).unwrap();
        assert!(load_session(b"other-secret-key-32bytes-long!!!!!", &tok, Some(3600)).is_err());
    }
}
