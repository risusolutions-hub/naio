//! High-level `Auth` manager: login, sessions, reset, CSRF, RBAC.

use crate::csrf;
use crate::error::{AuthError, AuthResult};
use crate::password::{self, VerifyUpdate};
use crate::rbac::{self, RoleHierarchy};
use crate::session::{
    clear_cookie, extract_cookie, issue_reset_token, load_session, session_cookie, sign_session,
    verify_reset_token, SessionData, DEFAULT_COOKIE_NAME, DEFAULT_RESET_MAX_AGE,
    DEFAULT_SESSION_LIFETIME,
};
use niao_pass::CryptContext;

/// Configuration for an Auth manager.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub secret: Vec<u8>,
    pub cookie_name: String,
    pub session_lifetime: u64,
    pub reset_max_age: u64,
    pub cookie_path: String,
    pub cookie_http_only: bool,
    pub cookie_secure: bool,
    pub cookie_same_site: String,
    pub hierarchy: RoleHierarchy,
    pub pass_ctx: CryptContext,
}

impl AuthConfig {
    pub fn new(secret: impl AsRef<[u8]>) -> AuthResult<Self> {
        let secret = secret.as_ref().to_vec();
        if secret.is_empty() {
            return Err(AuthError::InvalidParameter(
                "secret must be non-empty".into(),
            ));
        }
        Ok(Self {
            secret,
            cookie_name: DEFAULT_COOKIE_NAME.into(),
            session_lifetime: DEFAULT_SESSION_LIFETIME,
            reset_max_age: DEFAULT_RESET_MAX_AGE,
            cookie_path: "/".into(),
            cookie_http_only: true,
            cookie_secure: false,
            cookie_same_site: "Lax".into(),
            hierarchy: RoleHierarchy::new(),
            pass_ctx: CryptContext::default(),
        })
    }
}

/// Web auth manager (~flask-login + django.contrib.auth subset).
#[derive(Debug, Clone)]
pub struct Auth {
    pub config: AuthConfig,
}

impl Auth {
    pub fn new(secret: impl AsRef<[u8]>) -> AuthResult<Self> {
        Ok(Self {
            config: AuthConfig::new(secret)?,
        })
    }

    pub fn with_config(config: AuthConfig) -> Self {
        Self { config }
    }

    pub fn hash_password(&self, password: &str) -> AuthResult<String> {
        password::hash_with(&self.config.pass_ctx, password)
    }

    pub fn verify_password(&self, password: &str, hash: &str) -> AuthResult<bool> {
        // Use context so deprecated/unsupported schemes are respected.
        Ok(self.config.pass_ctx.verify(password, hash)?)
    }

    pub fn verify_and_update(&self, password: &str, hash: &str) -> AuthResult<VerifyUpdate> {
        password::verify_and_update(&self.config.pass_ctx, password, hash)
    }

    /// Create a session for an already-authenticated user (flask `login_user`).
    pub fn login_user(
        &self,
        user_id: &str,
        roles: &[String],
        perms: &[String],
    ) -> AuthResult<SessionData> {
        if user_id.is_empty() {
            return Err(AuthError::InvalidParameter(
                "user_id must be non-empty".into(),
            ));
        }
        let mut s = SessionData::new(user_id);
        s.roles = roles.to_vec();
        s.permissions = perms.to_vec();
        Ok(s)
    }

    /// Verify password then create a session. Returns updated hash if rehash needed.
    pub fn login(
        &self,
        user_id: &str,
        password: &str,
        stored_hash: &str,
        roles: &[String],
        perms: &[String],
    ) -> AuthResult<LoginResult> {
        let vu = self.verify_and_update(password, stored_hash)?;
        if !vu.ok {
            return Err(AuthError::BadCredentials);
        }
        let session = self.login_user(user_id, roles, perms)?;
        Ok(LoginResult {
            session,
            hash: vu.hash,
            updated: vu.updated,
        })
    }

    pub fn sign_session(&self, session: &SessionData) -> AuthResult<String> {
        sign_session(&self.config.secret, session)
    }

    pub fn load_session(&self, token: &str) -> AuthResult<SessionData> {
        load_session(
            &self.config.secret,
            token,
            Some(self.config.session_lifetime),
        )
    }

    pub fn session_from_cookie(&self, cookie_header: &str) -> AuthResult<Option<SessionData>> {
        match extract_cookie(cookie_header, &self.config.cookie_name) {
            Some(tok) => Ok(Some(self.load_session(&tok)?)),
            None => Ok(None),
        }
    }

    pub fn cookie_header(&self, session: &SessionData) -> AuthResult<String> {
        let tok = self.sign_session(session)?;
        Ok(session_cookie(
            &self.config.cookie_name,
            &tok,
            Some(self.config.session_lifetime),
            &self.config.cookie_path,
            self.config.cookie_http_only,
            self.config.cookie_secure,
            Some(self.config.cookie_same_site.as_str()),
        ))
    }

    pub fn logout_cookie(&self) -> String {
        clear_cookie(&self.config.cookie_name, &self.config.cookie_path)
    }

    pub fn reset_token(&self, user_id: &str) -> AuthResult<String> {
        issue_reset_token(&self.config.secret, user_id)
    }

    pub fn verify_reset(&self, token: &str, max_age: Option<u64>) -> AuthResult<String> {
        verify_reset_token(
            &self.config.secret,
            token,
            Some(max_age.unwrap_or(self.config.reset_max_age)),
        )
    }

    pub fn complete_reset(
        &self,
        token: &str,
        new_password: &str,
        max_age: Option<u64>,
    ) -> AuthResult<ResetResult> {
        let user_id = self.verify_reset(token, max_age)?;
        let hash = self.hash_password(new_password)?;
        Ok(ResetResult { user_id, hash })
    }

    pub fn csrf_token(&self, session_id: &str) -> AuthResult<String> {
        csrf::require_session_id(session_id)?;
        csrf::issue(&self.config.secret, session_id)
    }

    pub fn validate_csrf(&self, session_id: &str, token: &str) -> bool {
        if session_id.is_empty() || token.is_empty() {
            return false;
        }
        csrf::validate(&self.config.secret, session_id, token)
    }

    pub fn allows(&self, user_roles: &[String], required: &str) -> bool {
        rbac::allows(&self.config.hierarchy, user_roles, required)
    }

    pub fn expand_roles(&self, roles: &[String]) -> Vec<String> {
        rbac::expand_roles(&self.config.hierarchy, roles)
    }

    pub fn has_permission(&self, perms: &[String], perm: &str) -> bool {
        rbac::has_permission(perms, perm)
    }
}

#[derive(Debug, Clone)]
pub struct LoginResult {
    pub session: SessionData,
    pub hash: Option<String>,
    pub updated: bool,
}

#[derive(Debug, Clone)]
pub struct ResetResult {
    pub user_id: String,
    pub hash: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::password::context_from_opts;

    fn fast_auth() -> Auth {
        let mut cfg = AuthConfig::new(b"test-secret-key-32-bytes-long!!!!").unwrap();
        cfg.pass_ctx = context_from_opts(Some("bcrypt"), Some(4), None, None).unwrap();
        let mut h = RoleHierarchy::new();
        h.insert("admin".into(), vec!["editor".into()]);
        cfg.hierarchy = h;
        Auth::with_config(cfg)
    }

    #[test]
    fn login_logout_flow() {
        let auth = fast_auth();
        let hash = auth.hash_password("pw").unwrap();
        let login = auth
            .login("u1", "pw", &hash, &["admin".into()], &[])
            .unwrap();
        assert_eq!(login.session.user_id, "u1");
        let cookie = auth.cookie_header(&login.session).unwrap();
        assert!(cookie.contains("session="));
        let loaded = auth.session_from_cookie(&cookie).unwrap().unwrap();
        assert_eq!(loaded.user_id, "u1");
        assert!(auth.allows(&loaded.roles, "editor"));
        let clear = auth.logout_cookie();
        assert!(clear.contains("Max-Age=0"));
    }

    #[test]
    fn bad_password() {
        let auth = fast_auth();
        let hash = auth.hash_password("pw").unwrap();
        assert!(matches!(
            auth.login("u1", "nope", &hash, &[], &[]),
            Err(AuthError::BadCredentials)
        ));
    }

    #[test]
    fn reset_and_csrf() {
        let auth = fast_auth();
        let tok = auth.reset_token("u1").unwrap();
        let r = auth.complete_reset(&tok, "newpw", None).unwrap();
        assert_eq!(r.user_id, "u1");
        assert!(auth.verify_password("newpw", &r.hash).unwrap());

        let s = auth.login_user("u1", &[], &[]).unwrap();
        let csrf = auth.csrf_token(&s.session_id).unwrap();
        assert!(auth.validate_csrf(&s.session_id, &csrf));
        assert!(!auth.validate_csrf(&s.session_id, "x.y"));
    }
}
