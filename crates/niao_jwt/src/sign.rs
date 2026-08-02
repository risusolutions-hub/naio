use crate::algo::{is_fast_hmac, parse_alg};
use crate::error::JwtError;
use crate::keys::Key;
use crate::options::SignOptions;
use jsonwebtoken::{encode, Algorithm, Header};
use niao_crypto::jwt::{sign as hs_sign, sign_hs256, sign_hs512, Algorithm as HsAlgo};
use niao_json_core::{to_string, Value};

pub fn sign(claims: &Value, key: &Key, opts: &SignOptions) -> Result<String, JwtError> {
    let algo = parse_alg(&opts.alg)?;
    if is_fast_hmac(algo) {
        return sign_fast_hmac(claims, key, opts, algo);
    }
    let mut header = Header::new(algo);
    if let Some(kid) = &opts.kid {
        header.kid = Some(kid.clone());
    }
    if let Some(typ) = &opts.typ {
        header.typ = Some(typ.clone());
    }
    let claims_json: serde_json::Value =
        serde_json::from_str(&to_string(claims)).map_err(|e| JwtError::Json(e.to_string()))?;
    encode(&header, &claims_json, key.encoding()?).map_err(JwtError::from)
}

fn sign_fast_hmac(
    claims: &Value,
    key: &Key,
    opts: &SignOptions,
    algo: Algorithm,
) -> Result<String, JwtError> {
    let secret = key
        .hmac_secret()
        .ok_or_else(|| JwtError::Key("HMAC sign requires a shared secret".into()))?;
    let mut header_obj = serde_json::Map::new();
    header_obj.insert("alg".into(), serde_json::Value::String(opts.alg.clone()));
    if let Some(typ) = &opts.typ {
        header_obj.insert("typ".into(), serde_json::Value::String(typ.clone()));
    }
    if let Some(kid) = &opts.kid {
        header_obj.insert("kid".into(), serde_json::Value::String(kid.clone()));
    }
    for (k, v) in &opts.extra_header {
        header_obj.insert(k.clone(), serde_json::Value::String(v.clone()));
    }
    let header_json =
        serde_json::to_string(&header_obj).map_err(|e| JwtError::Json(e.to_string()))?;
    let payload_json = to_string(claims);
    let hs = match algo {
        Algorithm::HS256 => HsAlgo::HS256,
        Algorithm::HS512 => HsAlgo::HS512,
        _ => unreachable!(),
    };
    hs_sign(hs, &header_json, &payload_json, secret).map_err(map_hs_err)
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

/// Convenience: sign HS256 with default header.
pub fn sign_hs256_default(claims: &Value, secret: &[u8]) -> Result<String, JwtError> {
    let payload = to_string(claims);
    sign_hs256(niao_crypto::jwt::default_header_hs256(), &payload, secret).map_err(map_hs_err)
}

/// Convenience: sign HS512 with explicit header JSON.
pub fn sign_hs512_default(claims: &Value, secret: &[u8]) -> Result<String, JwtError> {
    let payload = to_string(claims);
    let header = r#"{"alg":"HS512","typ":"JWT"}"#;
    sign_hs512(header, &payload, secret).map_err(map_hs_err)
}
