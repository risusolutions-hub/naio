use crate::error::{OAuthError, OAuthResult};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientAuthMethod {
    /// `client_id` + optional `client_secret` in POST body (default).
    Body,
    /// HTTP Basic auth header.
    Basic,
    /// `client_id` only in body (public clients).
    None,
}

impl ClientAuthMethod {
    pub fn from_str(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "basic" => Self::Basic,
            "none" | "public" => Self::None,
            _ => Self::Body,
        }
    }
}

/// OAuth2 / OIDC client configuration.
#[derive(Debug, Clone)]
pub struct OAuthClient {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub redirect_uri: Option<String>,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub revocation_endpoint: Option<String>,
    pub introspection_endpoint: Option<String>,
    pub userinfo_endpoint: Option<String>,
    pub jwks_uri: Option<String>,
    pub issuer: Option<String>,
    pub scopes: Vec<String>,
    pub client_auth_method: ClientAuthMethod,
    pub timeout_ms: u64,
    pub extra_auth_params: HashMap<String, String>,
    pub extra_token_params: HashMap<String, String>,
}

impl OAuthClient {
    pub fn builder(
        client_id: impl Into<String>,
        token_endpoint: impl Into<String>,
    ) -> OAuthClientBuilder {
        OAuthClientBuilder::new(client_id, token_endpoint)
    }

    pub fn scope_string(&self) -> String {
        self.scopes.join(" ")
    }

    pub fn validate_for_auth_code(&self) -> OAuthResult<()> {
        if self.authorization_endpoint.is_empty() {
            return Err(OAuthError::Config(
                "authorization_endpoint is required for auth code flow".into(),
            ));
        }
        if self.redirect_uri.as_ref().is_none_or(|u| u.is_empty()) {
            return Err(OAuthError::Config(
                "redirect_uri is required for auth code flow".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct OAuthClientBuilder {
    client_id: String,
    client_secret: Option<String>,
    redirect_uri: Option<String>,
    authorization_endpoint: String,
    token_endpoint: String,
    revocation_endpoint: Option<String>,
    introspection_endpoint: Option<String>,
    userinfo_endpoint: Option<String>,
    jwks_uri: Option<String>,
    issuer: Option<String>,
    scopes: Vec<String>,
    client_auth_method: ClientAuthMethod,
    timeout_ms: u64,
    extra_auth_params: HashMap<String, String>,
    extra_token_params: HashMap<String, String>,
}

impl OAuthClientBuilder {
    pub fn new(client_id: impl Into<String>, token_endpoint: impl Into<String>) -> Self {
        Self {
            client_id: client_id.into(),
            client_secret: None,
            redirect_uri: None,
            authorization_endpoint: String::new(),
            token_endpoint: token_endpoint.into(),
            revocation_endpoint: None,
            introspection_endpoint: None,
            userinfo_endpoint: None,
            jwks_uri: None,
            issuer: None,
            scopes: Vec::new(),
            client_auth_method: ClientAuthMethod::Body,
            timeout_ms: 30_000,
            extra_auth_params: HashMap::new(),
            extra_token_params: HashMap::new(),
        }
    }

    pub fn client_secret(mut self, secret: impl Into<String>) -> Self {
        self.client_secret = Some(secret.into());
        self
    }

    pub fn redirect_uri(mut self, uri: impl Into<String>) -> Self {
        self.redirect_uri = Some(uri.into());
        self
    }

    pub fn authorization_endpoint(mut self, url: impl Into<String>) -> Self {
        self.authorization_endpoint = url.into();
        self
    }

    pub fn revocation_endpoint(mut self, url: impl Into<String>) -> Self {
        self.revocation_endpoint = Some(url.into());
        self
    }

    pub fn introspection_endpoint(mut self, url: impl Into<String>) -> Self {
        self.introspection_endpoint = Some(url.into());
        self
    }

    pub fn userinfo_endpoint(mut self, url: impl Into<String>) -> Self {
        self.userinfo_endpoint = Some(url.into());
        self
    }

    pub fn jwks_uri(mut self, url: impl Into<String>) -> Self {
        self.jwks_uri = Some(url.into());
        self
    }

    pub fn issuer(mut self, iss: impl Into<String>) -> Self {
        self.issuer = Some(iss.into());
        self
    }

    pub fn scopes(mut self, scopes: Vec<String>) -> Self {
        self.scopes = scopes;
        self
    }

    pub fn client_auth_method(mut self, method: ClientAuthMethod) -> Self {
        self.client_auth_method = method;
        self
    }

    pub fn timeout_ms(mut self, ms: u64) -> Self {
        self.timeout_ms = ms;
        self
    }

    pub fn extra_auth_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_auth_params.insert(key.into(), value.into());
        self
    }

    pub fn extra_token_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_token_params.insert(key.into(), value.into());
        self
    }

    pub fn build(self) -> OAuthResult<OAuthClient> {
        if self.client_id.is_empty() {
            return Err(OAuthError::Config("client_id is required".into()));
        }
        if self.token_endpoint.is_empty() {
            return Err(OAuthError::Config("token_endpoint is required".into()));
        }
        Ok(OAuthClient {
            client_id: self.client_id,
            client_secret: self.client_secret,
            redirect_uri: self.redirect_uri,
            authorization_endpoint: self.authorization_endpoint,
            token_endpoint: self.token_endpoint,
            revocation_endpoint: self.revocation_endpoint,
            introspection_endpoint: self.introspection_endpoint,
            userinfo_endpoint: self.userinfo_endpoint,
            jwks_uri: self.jwks_uri,
            issuer: self.issuer,
            scopes: self.scopes,
            client_auth_method: self.client_auth_method,
            timeout_ms: self.timeout_ms,
            extra_auth_params: self.extra_auth_params,
            extra_token_params: self.extra_token_params,
        })
    }
}
