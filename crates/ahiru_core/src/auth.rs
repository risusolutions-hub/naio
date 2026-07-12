use crate::context::RequestContext;
use crate::value_de;
use niao_crypto::jwt::{self, Validation};
use niao_json_core::{to_string, Value};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    None,
    Jwt,
    Session,
    ApiKey,
}

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub mode: AuthMode,
    pub scope: String,
    pub jwt_secret: String,
    pub session_secret: String,
    pub api_keys: HashSet<String>,
    pub rbac_enabled: bool,
}

impl AuthConfig {
    pub fn from_file(f: &crate::config::AuthConfigFile) -> Self {
        let mode = match f.mode.to_lowercase().as_str() {
            "jwt" => AuthMode::Jwt,
            "session" => AuthMode::Session,
            "api_key" | "apikey" | "api-key" => AuthMode::ApiKey,
            _ => AuthMode::None,
        };
        Self {
            mode,
            scope: f.scope.clone(),
            jwt_secret: f.jwt_secret.clone().unwrap_or_else(|| "dev-secret-change-me".into()),
            session_secret: f
                .session_secret
                .clone()
                .unwrap_or_else(|| "session-dev-secret".into()),
            api_keys: f.api_keys.iter().cloned().collect(),
            rbac_enabled: f.rbac_enabled,
        }
    }

    pub fn authenticate(&self, ctx: &RequestContext) -> Result<Option<UserContext>, String> {
        match self.mode {
            AuthMode::None => Ok(None),
            AuthMode::Jwt => self.authenticate_jwt(ctx),
            AuthMode::Session => self.authenticate_session(ctx),
            AuthMode::ApiKey => self.authenticate_api_key(ctx),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    pub sub: String,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    pub exp: usize,
}

#[derive(Debug, Clone)]
pub struct UserContext {
    pub id: String,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
}

impl AuthConfig {
    fn authenticate_jwt(&self, ctx: &RequestContext) -> Result<Option<UserContext>, String> {
        let auth = ctx.header("authorization").ok_or("missing authorization header")?;
        let token = auth
            .strip_prefix("Bearer ")
            .or_else(|| auth.strip_prefix("bearer "))
            .ok_or("expected Bearer token")?;
        let payload = jwt::verify(token, self.jwt_secret.as_bytes(), &Validation::default())
            .map_err(|e| e.to_string())?;
        let claims: JwtClaims = value_de::from_value(&payload)?;
        Ok(Some(UserContext {
            id: claims.sub,
            roles: claims.roles,
            permissions: claims.permissions,
        }))
    }

    fn authenticate_session(&self, ctx: &RequestContext) -> Result<Option<UserContext>, String> {
        let cookie = ctx
            .header("cookie")
            .ok_or("missing session cookie")?;
        let session = cookie
            .split(';')
            .find_map(|p| {
                let p = p.trim();
                p.strip_prefix("ahiru_session=")
            })
            .ok_or("ahiru_session cookie not found")?;
        let user = verify_session_token(session, &self.session_secret)?;
        Ok(Some(user))
    }

    fn authenticate_api_key(&self, ctx: &RequestContext) -> Result<Option<UserContext>, String> {
        let key = ctx
            .header("x-api-key")
            .ok_or("missing X-API-Key header")?;
        if !self.api_keys.contains(key) {
            return Err("invalid API key".into());
        }
        Ok(Some(UserContext {
            id: format!("apikey:{}", &key[..key.len().min(8)]),
            roles: vec!["api".into()],
            permissions: vec!["*".into()],
        }))
    }

    pub fn issue_jwt(&self, user_id: &str, roles: &[String], permissions: &[String], exp_secs: usize) -> Result<String, String> {
        let exp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs() as usize
            + exp_secs;
        let claims = JwtClaims {
            sub: user_id.into(),
            roles: roles.to_vec(),
            permissions: permissions.to_vec(),
            exp,
        };
        let payload = claims_to_json(&claims)?;
        jwt::sign_hs256(jwt::default_header_hs256(), &payload, self.jwt_secret.as_bytes())
            .map_err(|e| e.to_string())
    }

    pub fn issue_session_cookie(&self, user_id: &str, roles: &[String], permissions: &[String]) -> Result<String, String> {
        let token = sign_session_token(user_id, roles, permissions, &self.session_secret)?;
        Ok(format!("ahiru_session={token}; HttpOnly; Path=/; SameSite=Lax"))
    }
}

fn claims_to_json(claims: &JwtClaims) -> Result<String, String> {
    let mut obj = niao_json_core::object::Object::new();
    obj.insert("sub".into(), Value::String(claims.sub.clone()));
    obj.insert("exp".into(), Value::Number(niao_json_core::Number::I64(claims.exp as i64)));
    if !claims.roles.is_empty() {
        obj.insert(
            "roles".into(),
            Value::Array(
                claims
                    .roles
                    .iter()
                    .map(|r| Value::String(r.clone()))
                    .collect(),
            ),
        );
    }
    if !claims.permissions.is_empty() {
        obj.insert(
            "permissions".into(),
            Value::Array(
                claims
                    .permissions
                    .iter()
                    .map(|p| Value::String(p.clone()))
                    .collect(),
            ),
        );
    }
    Ok(to_string(&Value::Object(obj)))
}

fn sign_session_token(
    user_id: &str,
    roles: &[String],
    permissions: &[String],
    secret: &str,
) -> Result<String, String> {
    let payload = format!(
        "{}|{}|{}",
        user_id,
        roles.join(","),
        permissions.join(",")
    );
    let sig = jwt::sha256_hex_secret_prefix(secret.as_bytes(), payload.as_bytes());
    Ok(niao_codec::base64::encode_standard(format!("{payload}.{sig}").as_bytes()))
}

fn verify_session_token(token: &str, secret: &str) -> Result<UserContext, String> {
    let decoded = niao_codec::base64::decode_standard(token).map_err(|e| e.to_string())?;
    let s = String::from_utf8(decoded).map_err(|e| e.to_string())?;
    let (payload, sig) = s.rsplit_once('.').ok_or("invalid session token")?;
    let expected = jwt::sha256_hex_secret_prefix(secret.as_bytes(), payload.as_bytes());
    if sig != expected {
        return Err("session signature mismatch".into());
    }
    let mut parts = payload.split('|');
    let id = parts.next().ok_or("invalid session payload")?.into();
    let roles: Vec<String> = parts
        .next()
        .unwrap_or("")
        .split(',')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    let permissions: Vec<String> = parts
        .next()
        .unwrap_or("")
        .split(',')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    Ok(UserContext {
        id,
        roles,
        permissions,
    })
}
