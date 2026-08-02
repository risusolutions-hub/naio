use crate::error::JwtError;
use crate::keys::Key;
use crate::options::{FetchOptions, VerifyOptions};
use crate::verify;
use jsonwebtoken::jwk::JwkSet;
use niao_http::get;
use niao_json_core::{parse, to_string, Value};
use std::time::Duration;

#[derive(Clone)]
pub struct Jwks {
    pub keys: Vec<Key>,
    pub raw: Value,
}

pub fn parse_jwks(json: &str) -> Result<Jwks, JwtError> {
    let set: JwkSet = serde_json::from_str(json).map_err(|e| JwtError::Jwks(e.to_string()))?;
    let mut keys = Vec::with_capacity(set.keys.len());
    for jwk in &set.keys {
        keys.push(Key::from_jwk(jwk)?);
    }
    let raw = parse(json).map_err(|e| JwtError::Json(e.to_string()))?;
    Ok(Jwks { keys, raw })
}

pub fn fetch_jwks(url: &str, opts: &FetchOptions) -> Result<Jwks, JwtError> {
    let mut req = get(url);
    if let Some(ua) = &opts.user_agent {
        req = req.header("User-Agent", ua);
    }
    if opts.timeout_ms > 0 {
        req = req.timeout(Duration::from_millis(opts.timeout_ms));
    }
    let resp = req.send().map_err(|e| JwtError::Fetch(e.to_string()))?;
    if resp.status < 200 || resp.status >= 300 {
        return Err(JwtError::Fetch(format!("HTTP {}", resp.status)));
    }
    if resp.body.len() > opts.max_bytes.max(1) {
        return Err(JwtError::Fetch(format!(
            "JWKS body {} bytes exceeds limit {}",
            resp.body.len(),
            opts.max_bytes
        )));
    }
    let text = String::from_utf8(resp.body).map_err(|e| JwtError::Fetch(e.to_string()))?;
    parse_jwks(&text)
}

pub fn verify_jwks(token: &str, jwks: &Jwks, opts: &VerifyOptions) -> Result<Value, JwtError> {
    let header = super::decode::header(token)?;
    let kid = header.get("kid").and_then(|v| v.as_str());
    if let Some(kid) = kid {
        for key in &jwks.keys {
            if key_matches_kid(&jwks.raw, key, kid) {
                return verify::verify(token, key, opts);
            }
        }
        return Err(JwtError::Jwks(format!("no JWK with kid '{kid}'")));
    }
    let mut last_err = JwtError::Signature;
    for key in &jwks.keys {
        match verify::verify(token, key, opts) {
            Ok(v) => return Ok(v),
            Err(e) => last_err = e,
        }
    }
    Err(last_err)
}

fn key_matches_kid(raw: &Value, _key: &Key, kid: &str) -> bool {
    if let Some(Value::Array(items)) = raw.get("keys") {
        return items
            .iter()
            .any(|j| j.get("kid").and_then(|v| v.as_str()) == Some(kid));
    }
    raw.get("kid").and_then(|v| v.as_str()) == Some(kid)
}

pub fn jwks_to_value(jwks: &Jwks) -> Value {
    jwks.raw.clone()
}

pub fn jwks_from_value(v: &Value) -> Result<Jwks, JwtError> {
    let json = to_string(v);
    parse_jwks(&json)
}

pub fn verify_all(
    tokens: &[String],
    key: &Key,
    opts: &VerifyOptions,
    threads: usize,
) -> Vec<Result<Value, JwtError>> {
    use niao_parallel::map;
    map(tokens, threads, |t| verify::verify(t, key, opts))
}
