use crate::client::OAuthClientBuilder;
use crate::error::{OAuthError, OAuthResult};
use crate::json_util::{
    object_as_map, object_get_str, object_require_str, object_str_array, value_to_object,
};
use niao_http::get;
use niao_json_core::{parse, Object, Value};

#[derive(Debug, Clone)]
pub struct OidcConfig {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub userinfo_endpoint: Option<String>,
    pub jwks_uri: Option<String>,
    pub revocation_endpoint: Option<String>,
    pub introspection_endpoint: Option<String>,
    pub scopes_supported: Vec<String>,
    pub response_types_supported: Vec<String>,
    pub grant_types_supported: Vec<String>,
    pub code_challenge_methods_supported: Vec<String>,
    pub raw: Object,
}

pub fn discover(issuer: &str) -> OAuthResult<OidcConfig> {
    let base = issuer.trim_end_matches('/');
    let url = format!("{base}/.well-known/openid-configuration");
    let resp = get(&url)
        .set("Accept", "application/json")
        .send()
        .map_err(|e| OAuthError::Discovery(e.to_string()))?;
    if resp.status < 200 || resp.status >= 300 {
        return Err(OAuthError::Discovery(format!(
            "discovery returned status {}",
            resp.status
        )));
    }
    let text = String::from_utf8_lossy(&resp.body);
    parse_discovery_json(&text)
}

pub fn parse_discovery_json(text: &str) -> OAuthResult<OidcConfig> {
    let map = value_to_object(parse(text).map_err(|e| OAuthError::Discovery(e.to_string()))?)?;
    let issuer = object_require_str(&map, "issuer", OAuthError::Discovery)?;
    let authorization_endpoint =
        object_require_str(&map, "authorization_endpoint", OAuthError::Discovery)?;
    let token_endpoint = object_require_str(&map, "token_endpoint", OAuthError::Discovery)?;
    Ok(OidcConfig {
        issuer: issuer.clone(),
        authorization_endpoint,
        token_endpoint,
        userinfo_endpoint: object_get_str(&map, "userinfo_endpoint"),
        jwks_uri: object_get_str(&map, "jwks_uri"),
        revocation_endpoint: object_get_str(&map, "revocation_endpoint"),
        introspection_endpoint: object_get_str(&map, "introspection_endpoint"),
        scopes_supported: object_str_array(&map, "scopes_supported"),
        response_types_supported: object_str_array(&map, "response_types_supported"),
        grant_types_supported: object_str_array(&map, "grant_types_supported"),
        code_challenge_methods_supported: object_str_array(
            &map,
            "code_challenge_methods_supported",
        ),
        raw: map,
    })
}

impl OidcConfig {
    pub fn client_builder(&self, client_id: impl Into<String>) -> OAuthResult<OAuthClientBuilder> {
        let mut b = OAuthClientBuilder::new(client_id, &self.token_endpoint)
            .authorization_endpoint(&self.authorization_endpoint)
            .issuer(&self.issuer);
        if let Some(u) = &self.userinfo_endpoint {
            b = b.userinfo_endpoint(u.clone());
        }
        if let Some(j) = &self.jwks_uri {
            b = b.jwks_uri(j.clone());
        }
        if let Some(r) = &self.revocation_endpoint {
            b = b.revocation_endpoint(r.clone());
        }
        if let Some(i) = &self.introspection_endpoint {
            b = b.introspection_endpoint(i.clone());
        }
        Ok(b)
    }

    pub fn raw_map(&self) -> std::collections::HashMap<String, Value> {
        object_as_map(&self.raw)
    }
}
