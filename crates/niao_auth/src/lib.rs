//! `niao_auth` — web auth kit for Niao (`nauth` stdlib).
//!
//! Sessions, login/logout, password reset, RBAC roles, CSRF tokens.
//! Built on `niao_pass` + `niao_sign` (~flask-login, django.contrib.auth subset).

mod auth;
mod csrf;
mod error;
mod password;
mod rbac;
mod session;
mod token;
mod user;

pub use auth::{Auth, AuthConfig, LoginResult, ResetResult};
pub use csrf::{issue as csrf_issue, validate as csrf_validate};
pub use error::{AuthError, AuthResult};
pub use password::{
    context_from_opts, hash as hash_password, hash_with, verify as verify_password,
    verify_and_update, VerifyUpdate,
};
pub use rbac::{
    allows, allows_all, allows_any, expand_roles, has_permission, has_role, RoleHierarchy,
};
pub use session::{
    clear_cookie, extract_cookie, issue_reset_token, load_session, session_cookie, sign_session,
    verify_reset_token, SessionData, DEFAULT_COOKIE_NAME, DEFAULT_RESET_MAX_AGE,
    DEFAULT_SESSION_LIFETIME,
};
pub use token::{compare, generate_token, DEFAULT_TOKEN_BYTES};
pub use user::{anonymous as anonymous_user, user};
