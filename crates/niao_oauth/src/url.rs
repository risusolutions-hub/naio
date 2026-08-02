use crate::client::OAuthClient;
use crate::error::{OAuthError, OAuthResult};
use crate::pkce::PkceChallengeMethod;
use niao_http::{form_urlencode, parse_url, Url};
use std::collections::HashMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuthUrlOptions {
    pub state: Option<String>,
    pub nonce: Option<String>,
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<PkceChallengeMethod>,
    pub scopes: Option<Vec<String>>,
    pub response_mode: Option<String>,
    pub prompt: Option<String>,
    pub login_hint: Option<String>,
    pub audience: Option<String>,
    pub extra: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationResponse {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
    pub error_uri: Option<String>,
}

/// Build OAuth2 authorization URL for the auth-code (+PKCE) flow.
///
/// >>> use niao_oauth::{OAuthClient, auth_url, AuthUrlOptions};
/// >>> let c = OAuthClient::builder("cid", "https://idp/token")
/// ...     .authorization_endpoint("https://idp/authorize")
/// ...     .redirect_uri("https://app/cb")
/// ...     .build().unwrap();
/// >>> let url = auth_url(&c, &AuthUrlOptions { state: Some("st".into()), ..Default::default() }).unwrap();
/// >>> url.contains("client_id=cid") && url.contains("state=st")
/// true
pub fn auth_url(client: &OAuthClient, opts: &AuthUrlOptions) -> OAuthResult<String> {
    client.validate_for_auth_code()?;
    let mut params: Vec<(String, String)> = Vec::new();
    params.push(("client_id".into(), client.client_id.clone()));
    params.push(("response_type".into(), "code".into()));
    params.push(("redirect_uri".into(), client.redirect_uri.clone().unwrap()));

    let scopes = opts
        .scopes
        .as_ref()
        .map(|s| s.join(" "))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| client.scope_string());
    if !scopes.is_empty() {
        params.push(("scope".into(), scopes));
    }
    if let Some(st) = &opts.state {
        params.push(("state".into(), st.clone()));
    }
    if let Some(n) = &opts.nonce {
        params.push(("nonce".into(), n.clone()));
    }
    if let Some(ch) = &opts.code_challenge {
        params.push(("code_challenge".into(), ch.clone()));
        let method = opts
            .code_challenge_method
            .unwrap_or(PkceChallengeMethod::S256)
            .as_str();
        params.push(("code_challenge_method".into(), method.into()));
    }
    if let Some(rm) = &opts.response_mode {
        params.push(("response_mode".into(), rm.clone()));
    }
    if let Some(p) = &opts.prompt {
        params.push(("prompt".into(), p.clone()));
    }
    if let Some(h) = &opts.login_hint {
        params.push(("login_hint".into(), h.clone()));
    }
    if let Some(a) = &opts.audience {
        params.push(("audience".into(), a.clone()));
    }
    for (k, v) in &client.extra_auth_params {
        params.push((k.clone(), v.clone()));
    }
    for (k, v) in &opts.extra {
        params.push((k.clone(), v.clone()));
    }

    append_query(&client.authorization_endpoint, &params)
}

/// Parse redirect callback URL query for `code`, `state`, or OAuth error.
///
/// >>> use niao_oauth::parse_callback_url;
/// >>> let r = parse_callback_url("https://app/cb?code=abc&state=xyz").unwrap();
/// >>> r.code.as_deref() == Some("abc") && r.state.as_deref() == Some("xyz")
/// true
pub fn parse_callback_url(url: &str) -> OAuthResult<AuthorizationResponse> {
    let parsed = parse_url(url).map_err(|e| OAuthError::Parse(e.to_string()))?;
    parse_query_map(&parsed)
}

pub fn parse_authorization_response(query: &str) -> OAuthResult<AuthorizationResponse> {
    let q = query.strip_prefix('?').unwrap_or(query);
    let mut map = HashMap::new();
    for pair in q.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = pair
            .split_once('=')
            .map(|(a, b)| (a, b))
            .unwrap_or((pair, ""));
        map.insert(percent_decode_component(k), percent_decode_component(v));
    }
    Ok(map_to_auth_response(&map))
}

fn parse_query_map(url: &Url) -> OAuthResult<AuthorizationResponse> {
    parse_authorization_response(&url.query)
}

fn map_to_auth_response(map: &HashMap<String, String>) -> AuthorizationResponse {
    AuthorizationResponse {
        code: map.get("code").cloned(),
        state: map.get("state").cloned(),
        error: map.get("error").cloned(),
        error_description: map.get("error_description").cloned(),
        error_uri: map.get("error_uri").cloned(),
    }
}

/// Validate returned `state` against expected value (constant-time compare when possible).
///
/// >>> use niao_oauth::validate_state;
/// >>> validate_state("abc", "abc").unwrap()
/// >>> validate_state("abc", "xyz").is_err()
/// true
pub fn validate_state(expected: &str, received: &str) -> OAuthResult<()> {
    if niao_crypto::constant_time_eq(expected.as_bytes(), received.as_bytes()) {
        Ok(())
    } else {
        Err(OAuthError::State("state mismatch".into()))
    }
}

/// Build HTTP Basic Authorization header value for client auth.
///
/// >>> use niao_oauth::basic_auth_header;
/// >>> basic_auth_header("id", "secret").starts_with("Basic ")
/// true
pub fn basic_auth_header(client_id: &str, client_secret: &str) -> String {
    let raw = format!("{client_id}:{client_secret}");
    format!(
        "Basic {}",
        niao_codec::base64::encode_standard(raw.as_bytes())
    )
}

fn append_query(base: &str, params: &[(String, String)]) -> OAuthResult<String> {
    if params.is_empty() {
        return Ok(base.to_string());
    }
    let body: Vec<u8> = params
        .iter()
        .flat_map(|(k, v)| {
            let mut s = k.as_bytes().to_vec();
            s.push(b'=');
            s.extend_from_slice(v.as_bytes());
            s.push(b'&');
            s
        })
        .collect();
    let encoded = form_urlencode(&body[..body.len().saturating_sub(1)]);
    let sep = if base.contains('?') { '&' } else { '?' };
    Ok(format!("{base}{sep}{encoded}"))
}

fn percent_decode_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h1), Some(h2)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push(char::from((h1 << 4) | h2));
                i += 3;
                continue;
            }
        } else if bytes[i] == b'+' {
            out.push(' ');
            i += 1;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_error_response() {
        let r =
            parse_callback_url("https://x/cb?error=access_denied&error_description=nope").unwrap();
        assert_eq!(r.error.as_deref(), Some("access_denied"));
        assert!(r.code.is_none());
    }
}
