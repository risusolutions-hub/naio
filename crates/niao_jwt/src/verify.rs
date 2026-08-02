use crate::algo::{is_fast_hmac, parse_alg};
use crate::error::JwtError;
use crate::keys::Key;
use crate::options::{to_validation, VerifyOptions};
use jsonwebtoken::{decode, Algorithm};
use niao_crypto::jwt::{verify as hs_verify, Validation as HsValidation};
use niao_json_core::{parse, Value};

pub fn verify(token: &str, key: &Key, opts: &VerifyOptions) -> Result<Value, JwtError> {
    let header_alg = header_alg(token)?;
    if is_fast_hmac(header_alg) && key.hmac_secret().is_some() {
        if opts
            .algorithms
            .iter()
            .any(|a| parse_alg(a).ok() == Some(header_alg))
        {
            return verify_fast_hmac(token, key, opts, header_alg);
        }
    }
    let validation = to_validation(opts)?;
    let data = decode::<serde_json::Value>(token, key.decoding()?, &validation)?;
    let json = serde_json::to_string(&data.claims).map_err(|e| JwtError::Json(e.to_string()))?;
    let out = parse(&json).map_err(|e| JwtError::Json(e.to_string()))?;
    if opts.validate_iat {
        validate_iat(&out, opts.leeway)?;
    }
    Ok(out)
}

fn verify_fast_hmac(
    token: &str,
    key: &Key,
    opts: &VerifyOptions,
    algo: Algorithm,
) -> Result<Value, JwtError> {
    let secret = key.hmac_secret().unwrap();
    let hs_val = HsValidation {
        validate_exp: opts.validate_exp,
        validate_nbf: opts.validate_nbf,
        leeway: opts.leeway,
    };
    let payload = hs_verify(token, secret, &hs_val).map_err(map_hs_err)?;
    if opts.validate_iat {
        validate_iat(&payload, opts.leeway)?;
    }
    if let Some(aud) = &opts.audience {
        check_audience(&payload, aud)?;
    }
    if let Some(iss) = &opts.issuer {
        check_issuer(&payload, iss)?;
    }
    if let Some(sub) = &opts.subject {
        check_subject(&payload, sub)?;
    }
    for claim in &opts.required_claims {
        if payload.get(claim).is_none() {
            return Err(JwtError::Message(format!(
                "missing required claim '{claim}'"
            )));
        }
    }
    let _ = algo;
    Ok(payload)
}

fn validate_iat(payload: &Value, leeway: u64) -> Result<(), JwtError> {
    let now = now_secs();
    if let Some(iat) = payload.get("iat").and_then(|v| v.as_i64()) {
        if now + leeway < iat as u64 {
            return Err(JwtError::Immature);
        }
    }
    Ok(())
}

fn check_audience(payload: &Value, expected: &str) -> Result<(), JwtError> {
    match payload.get("aud") {
        Some(Value::String(s)) if s == expected => Ok(()),
        Some(Value::Array(items)) if items.iter().any(|v| v.as_str() == Some(expected)) => Ok(()),
        _ => Err(JwtError::Audience),
    }
}

fn check_issuer(payload: &Value, expected: &str) -> Result<(), JwtError> {
    match payload.get("iss").and_then(|v| v.as_str()) {
        Some(s) if s == expected => Ok(()),
        _ => Err(JwtError::Issuer),
    }
}

fn check_subject(payload: &Value, expected: &str) -> Result<(), JwtError> {
    match payload.get("sub").and_then(|v| v.as_str()) {
        Some(s) if s == expected => Ok(()),
        _ => Err(JwtError::Subject),
    }
}

fn map_hs_err(e: niao_crypto::jwt::JwtError) -> JwtError {
    match e {
        niao_crypto::jwt::JwtError::Json(s) => JwtError::Json(s),
        niao_crypto::jwt::JwtError::Algorithm => JwtError::Algorithm,
        niao_crypto::jwt::JwtError::Format => JwtError::Format,
        niao_crypto::jwt::JwtError::Base64 => JwtError::Base64,
        niao_crypto::jwt::JwtError::Signature => JwtError::Signature,
        niao_crypto::jwt::JwtError::Expired => JwtError::Expired,
        niao_crypto::jwt::JwtError::NotBefore => JwtError::NotBefore,
        niao_crypto::jwt::JwtError::Message(s) => JwtError::Message(s),
    }
}

pub fn header_alg(token: &str) -> Result<Algorithm, JwtError> {
    let (header, _) = super::decode::decode_unverified(token)?;
    let alg = header
        .get("alg")
        .and_then(|v| v.as_str())
        .ok_or(JwtError::Algorithm)?;
    parse_alg(alg)
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
