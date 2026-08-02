use crate::client::{ClientAuthMethod, OAuthClient};
use crate::error::{OAuthError, OAuthResult};
use crate::json_util::{object_get_str, value_as_u64, value_to_object};
use crate::pkce::validate_verifier;
use niao_http::{post, RequestBuilder, Response};
use niao_json_core::{parse, Object, Value};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: Option<u64>,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
    pub id_token: Option<String>,
    pub obtained_at: u64,
    pub raw: Object,
}

#[derive(Debug, Clone, Default)]
pub struct ExchangeOptions {
    pub code_verifier: Option<String>,
    pub redirect_uri: Option<String>,
    pub scope: Option<String>,
    pub extra: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default)]
pub struct ClientCredentialsOptions {
    pub scope: Option<String>,
    pub audience: Option<String>,
    pub extra: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default)]
pub struct RefreshOptions {
    pub scope: Option<String>,
    pub extra: Vec<(String, String)>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn exchange_code(
    client: &OAuthClient,
    code: &str,
    opts: &ExchangeOptions,
) -> OAuthResult<TokenResponse> {
    if code.is_empty() {
        return Err(OAuthError::Token("authorization code is empty".into()));
    }
    if let Some(v) = &opts.code_verifier {
        validate_verifier(v)?;
    }
    let mut params = base_token_params(client);
    params.push(("grant_type".into(), "authorization_code".into()));
    params.push(("code".into(), code.into()));
    let redirect = opts
        .redirect_uri
        .clone()
        .or_else(|| client.redirect_uri.clone())
        .ok_or_else(|| OAuthError::Config("redirect_uri required".into()))?;
    params.push(("redirect_uri".into(), redirect));
    if let Some(v) = &opts.code_verifier {
        params.push(("code_verifier".into(), v.clone()));
    }
    if let Some(s) = &opts.scope {
        params.push(("scope".into(), s.clone()));
    }
    for (k, v) in &opts.extra {
        params.push((k.clone(), v.clone()));
    }
    token_request(client, &params)
}

pub fn client_credentials(
    client: &OAuthClient,
    opts: &ClientCredentialsOptions,
) -> OAuthResult<TokenResponse> {
    let mut params = base_token_params(client);
    params.push(("grant_type".into(), "client_credentials".into()));
    let scope = opts.scope.clone().or_else(|| {
        let s = client.scope_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    });
    if let Some(s) = scope {
        params.push(("scope".into(), s));
    }
    if let Some(a) = &opts.audience {
        params.push(("audience".into(), a.clone()));
    }
    for (k, v) in &opts.extra {
        params.push((k.clone(), v.clone()));
    }
    token_request(client, &params)
}

pub fn refresh_token(
    client: &OAuthClient,
    refresh: &str,
    opts: &RefreshOptions,
) -> OAuthResult<TokenResponse> {
    if refresh.is_empty() {
        return Err(OAuthError::Token("refresh_token is empty".into()));
    }
    let mut params = base_token_params(client);
    params.push(("grant_type".into(), "refresh_token".into()));
    params.push(("refresh_token".into(), refresh.into()));
    if let Some(s) = &opts.scope {
        params.push(("scope".into(), s.clone()));
    }
    for (k, v) in &opts.extra {
        params.push((k.clone(), v.clone()));
    }
    token_request(client, &params)
}

pub fn revoke_token(
    client: &OAuthClient,
    token: &str,
    token_type_hint: Option<&str>,
) -> OAuthResult<()> {
    let endpoint = client
        .revocation_endpoint
        .as_ref()
        .ok_or_else(|| OAuthError::Revocation("revocation_endpoint not configured".into()))?;
    let mut params = base_token_params(client);
    params.push(("token".into(), token.into()));
    if let Some(h) = token_type_hint {
        params.push(("token_type_hint".into(), h.into()));
    }
    let body = encode_form(&params);
    let resp = http_send(build_request(client, post(endpoint)).send_string(&body))?;
    if resp.status >= 200 && resp.status < 300 {
        Ok(())
    } else {
        Err(OAuthError::Revocation(format!(
            "revocation failed with status {}",
            resp.status
        )))
    }
}

pub fn introspect_token(client: &OAuthClient, token: &str) -> OAuthResult<Object> {
    let endpoint = client
        .introspection_endpoint
        .as_ref()
        .ok_or_else(|| OAuthError::Introspection("introspection_endpoint not configured".into()))?;
    let mut params = base_token_params(client);
    params.push(("token".into(), token.into()));
    let body = encode_form(&params);
    let resp = http_send(build_request(client, post(endpoint)).send_string(&body))?;
    if resp.status < 200 || resp.status >= 300 {
        return Err(OAuthError::Introspection(format!(
            "introspection failed with status {}",
            resp.status
        )));
    }
    let text = String::from_utf8_lossy(&resp.body);
    parse_token_json(&text).map(|t| t.raw)
}

pub fn fetch_userinfo(client: &OAuthClient, access_token: &str) -> OAuthResult<Object> {
    let endpoint = client
        .userinfo_endpoint
        .as_ref()
        .ok_or_else(|| OAuthError::Userinfo("userinfo_endpoint not configured".into()))?;
    let resp = http_send(
        build_request(client, niao_http::get(endpoint))
            .set("Authorization", format!("Bearer {access_token}"))
            .send(),
    )?;
    if resp.status < 200 || resp.status >= 300 {
        return Err(OAuthError::Userinfo(format!(
            "userinfo failed with status {}",
            resp.status
        )));
    }
    let text = String::from_utf8_lossy(&resp.body);
    value_to_object(parse(&text).map_err(|e| OAuthError::Parse(e.to_string()))?)
}

pub fn parse_token_json(text: &str) -> OAuthResult<TokenResponse> {
    let map = value_to_object(parse(text).map_err(|e| OAuthError::Parse(e.to_string()))?)?;
    if let Some(err) = object_get_str(&map, "error") {
        let desc = object_get_str(&map, "error_description").unwrap_or_default();
        return Err(OAuthError::Token(format!("{err}: {desc}")));
    }
    let access_token = object_get_str(&map, "access_token")
        .ok_or_else(|| OAuthError::Token("missing access_token".into()))?;
    let token_type = object_get_str(&map, "token_type").unwrap_or_else(|| "Bearer".into());
    let expires_in = map.get("expires_in").and_then(value_as_u64);
    let refresh_token = object_get_str(&map, "refresh_token");
    let scope = object_get_str(&map, "scope");
    let id_token = object_get_str(&map, "id_token");
    Ok(TokenResponse {
        access_token,
        token_type,
        expires_in,
        refresh_token,
        scope,
        id_token,
        obtained_at: now_secs(),
        raw: map,
    })
}

pub fn access_token(token: &TokenResponse) -> &str {
    &token.access_token
}

pub fn token_type(token: &TokenResponse) -> &str {
    &token.token_type
}

pub fn is_bearer(token: &TokenResponse) -> bool {
    token.token_type.eq_ignore_ascii_case("bearer")
}

pub fn token_expires_in(token: &TokenResponse) -> Option<u64> {
    token.expires_in
}

pub fn token_expired(token: &TokenResponse, leeway_secs: u64) -> bool {
    let Some(expires_in) = token.expires_in else {
        return false;
    };
    let elapsed = now_secs().saturating_sub(token.obtained_at);
    elapsed + leeway_secs >= expires_in
}

fn base_token_params(client: &OAuthClient) -> Vec<(String, String)> {
    let mut params = Vec::new();
    match client.client_auth_method {
        ClientAuthMethod::Body | ClientAuthMethod::None => {
            params.push(("client_id".into(), client.client_id.clone()));
            if let Some(secret) = &client.client_secret {
                params.push(("client_secret".into(), secret.clone()));
            }
        }
        ClientAuthMethod::Basic => {}
    }
    for (k, v) in &client.extra_token_params {
        params.push((k.clone(), v.clone()));
    }
    params
}

fn token_request(client: &OAuthClient, params: &[(String, String)]) -> OAuthResult<TokenResponse> {
    let body = encode_form(params);
    let resp = http_send(build_request(client, post(&client.token_endpoint)).send_string(&body))?;
    if resp.status < 200 || resp.status >= 300 {
        let text = String::from_utf8_lossy(&resp.body);
        if let Ok(parsed) = parse_token_json(&text) {
            return Ok(parsed);
        }
        return Err(OAuthError::Token(format!(
            "token endpoint returned status {}: {}",
            resp.status,
            text.chars().take(256).collect::<String>()
        )));
    }
    let text = String::from_utf8_lossy(&resp.body);
    parse_token_json(&text)
}

fn build_request(client: &OAuthClient, mut req: RequestBuilder) -> RequestBuilder {
    req = req
        .set("Content-Type", "application/x-www-form-urlencoded")
        .set("Accept", "application/json")
        .timeout(std::time::Duration::from_millis(client.timeout_ms));
    if client.client_auth_method == ClientAuthMethod::Basic {
        if let Some(secret) = &client.client_secret {
            let auth = crate::url::basic_auth_header(&client.client_id, secret);
            req = req.set("Authorization", auth);
        }
    }
    req
}

fn encode_form(params: &[(String, String)]) -> String {
    if params.is_empty() {
        return String::new();
    }
    let mut body = Vec::new();
    for (i, (k, v)) in params.iter().enumerate() {
        if i > 0 {
            body.push(b'&');
        }
        body.extend_from_slice(k.as_bytes());
        body.push(b'=');
        body.extend_from_slice(v.as_bytes());
    }
    niao_http::form_urlencode(&body)
}

fn http_send(r: Result<Response, niao_http::Error>) -> OAuthResult<Response> {
    r.map_err(|e| OAuthError::Http(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_token_response() {
        let json =
            r#"{"access_token":"at","token_type":"Bearer","expires_in":3600,"refresh_token":"rt"}"#;
        let t = parse_token_json(json).unwrap();
        assert_eq!(t.access_token, "at");
        assert_eq!(t.expires_in, Some(3600));
    }

    #[test]
    fn token_expired_logic() {
        let mut t = parse_token_json(r#"{"access_token":"x","expires_in":60}"#).unwrap();
        t.obtained_at = now_secs().saturating_sub(100);
        assert!(token_expired(&t, 0));
    }
}
