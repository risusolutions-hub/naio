use crate::client::OAuthClient;
use crate::error::{OAuthError, OAuthResult};
use crate::json_util::{object_get_str, value_as_u64, value_to_object};
use niao_codec::base64::decode_url_safe;
use niao_crypto::jwt::{decode_unverified, verify, JwtError, Validation};
use niao_http::get;
use niao_json_core::{parse, Object, Value};
use rsa::pkcs1v15::VerifyingKey;
use rsa::signature::Verifier;
use rsa::{BigUint, RsaPublicKey};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, Default)]
pub struct IdTokenValidation {
    pub issuer: Option<String>,
    pub audience: Option<String>,
    pub nonce: Option<String>,
    pub leeway: u64,
    pub validate_exp: bool,
    pub validate_nbf: bool,
    pub max_age: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct VerifiedClaims {
    pub header: Object,
    pub claims: Object,
}

static JWKS_CACHE: OnceLock<Mutex<HashMap<String, HashMap<String, RsaPublicKey>>>> =
    OnceLock::new();

fn jwks_cache() -> &'static Mutex<HashMap<String, HashMap<String, RsaPublicKey>>> {
    JWKS_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn decode_id_token(token: &str) -> OAuthResult<(Object, Object)> {
    let (header, payload) = decode_unverified(token).map_err(map_jwt_err)?;
    Ok((value_to_object(header)?, value_to_object(payload)?))
}

pub fn verify_id_token(
    client: &OAuthClient,
    token: &str,
    validation: &IdTokenValidation,
) -> OAuthResult<VerifiedClaims> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(OAuthError::IdToken("invalid JWT format".into()));
    }
    let header_json = decode_segment(parts[0])?;
    let header_map = json_str_to_object(&header_json)?;
    let alg = header_map.get("alg").and_then(|v| v.as_str()).unwrap_or("");
    match alg {
        "HS256" | "HS512" => {
            let secret = client
                .client_secret
                .as_ref()
                .ok_or_else(|| OAuthError::IdToken("HS* ID tokens require client_secret".into()))?;
            let val = Validation {
                validate_exp: validation.validate_exp,
                validate_nbf: validation.validate_nbf,
                leeway: validation.leeway,
            };
            let payload = verify(token, secret.as_bytes(), &val).map_err(map_jwt_err)?;
            let claims = value_to_object(payload)?;
            validate_oidc_claims(client, &claims, validation)?;
            Ok(VerifiedClaims {
                header: header_map,
                claims,
            })
        }
        "RS256" => {
            let kid = header_map.get("kid").and_then(|v| v.as_str());
            let jwks_uri = client
                .jwks_uri
                .as_ref()
                .ok_or_else(|| OAuthError::IdToken("jwks_uri not configured for RS256".into()))?;
            let key = resolve_rsa_key(jwks_uri, kid)?;
            verify_rs256(token, &key)?;
            let payload_json = decode_segment(parts[1])?;
            let claims = json_str_to_object(&payload_json)?;
            if validation.validate_exp || validation.validate_nbf {
                validate_times_object(&claims, validation)?;
            }
            validate_oidc_claims(client, &claims, validation)?;
            Ok(VerifiedClaims {
                header: header_map,
                claims,
            })
        }
        other => Err(OAuthError::IdToken(format!(
            "unsupported id_token alg: {other}"
        ))),
    }
}

fn validate_oidc_claims(
    client: &OAuthClient,
    claims: &Object,
    validation: &IdTokenValidation,
) -> OAuthResult<()> {
    if let Some(expected_iss) = validation.issuer.as_ref().or(client.issuer.as_ref()) {
        let iss = object_get_str(claims, "iss").unwrap_or_default();
        if iss != *expected_iss {
            return Err(OAuthError::IdToken(format!(
                "issuer mismatch: expected {expected_iss}, got {iss}"
            )));
        }
    }
    if let Some(aud) = validation.audience.as_ref().or(Some(&client.client_id)) {
        let ok = match claims.get("aud") {
            Some(Value::String(s)) => s == aud,
            Some(Value::Array(items)) => items.iter().any(|v| v.as_str() == Some(aud.as_str())),
            _ => false,
        };
        if !ok {
            return Err(OAuthError::IdToken("audience mismatch".into()));
        }
    }
    if let Some(expected_nonce) = &validation.nonce {
        let nonce = object_get_str(claims, "nonce").unwrap_or_default();
        if nonce != *expected_nonce {
            return Err(OAuthError::IdToken("nonce mismatch".into()));
        }
    }
    if let Some(max_age) = validation.max_age {
        let auth_time = claims.get("auth_time").and_then(value_as_u64).unwrap_or(0);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if now.saturating_sub(auth_time) > max_age {
            return Err(OAuthError::IdToken("auth_time exceeds max_age".into()));
        }
    }
    Ok(())
}

fn validate_times_object(claims: &Object, validation: &IdTokenValidation) -> OAuthResult<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if validation.validate_exp {
        if let Some(exp) = claims.get("exp").and_then(value_as_u64) {
            if now > exp.saturating_add(validation.leeway) {
                return Err(OAuthError::IdToken("token expired".into()));
            }
        }
    }
    if validation.validate_nbf {
        if let Some(nbf) = claims.get("nbf").and_then(value_as_u64) {
            if now + validation.leeway < nbf {
                return Err(OAuthError::IdToken("token not yet valid".into()));
            }
        }
    }
    Ok(())
}

fn verify_rs256(token: &str, key: &RsaPublicKey) -> OAuthResult<()> {
    let parts: Vec<&str> = token.split('.').collect();
    let signing_input = format!("{}.{}", parts[0], parts[1]);
    let sig_bytes =
        decode_url_safe(parts[2]).map_err(|_| OAuthError::IdToken("invalid signature".into()))?;
    let digest = Sha256::digest(signing_input.as_bytes());
    let vk = VerifyingKey::<Sha256>::new_unprefixed(key.clone());
    let sig = rsa::pkcs1v15::Signature::try_from(sig_bytes.as_slice())
        .map_err(|_| OAuthError::IdToken("invalid signature bytes".into()))?;
    vk.verify(&digest, &sig)
        .map_err(|_| OAuthError::IdToken("invalid RS256 signature".into()))
}

fn resolve_rsa_key(jwks_uri: &str, kid: Option<&str>) -> OAuthResult<RsaPublicKey> {
    if let Ok(cache) = jwks_cache().lock() {
        if let Some(keys) = cache.get(jwks_uri) {
            if let Some(kid) = kid {
                if let Some(k) = keys.get(kid) {
                    return Ok(k.clone());
                }
            } else if keys.len() == 1 {
                return Ok(keys.values().next().unwrap().clone());
            }
        }
    }
    let resp = get(jwks_uri)
        .set("Accept", "application/json")
        .send()
        .map_err(|e| OAuthError::IdToken(e.to_string()))?;
    if resp.status < 200 || resp.status >= 300 {
        return Err(OAuthError::IdToken(format!(
            "JWKS fetch failed: status {}",
            resp.status
        )));
    }
    let text = String::from_utf8_lossy(&resp.body);
    let root = value_to_object(parse(&text).map_err(|e| OAuthError::IdToken(e.to_string()))?)?;
    let keys_val = root
        .get("keys")
        .cloned()
        .ok_or_else(|| OAuthError::IdToken("JWKS missing keys".into()))?;
    let mut parsed = HashMap::new();
    if let Value::Array(items) = keys_val {
        for item in items {
            if let Value::Object(kobj) = item {
                if kobj.get("kty").and_then(|v| v.as_str()) != Some("RSA") {
                    continue;
                }
                let n = kobj
                    .get("n")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| OAuthError::IdToken("JWK missing n".into()))?;
                let e = kobj
                    .get("e")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| OAuthError::IdToken("JWK missing e".into()))?;
                let kid_key = kobj
                    .get("kid")
                    .and_then(|v| v.as_str())
                    .unwrap_or("default")
                    .to_string();
                let rsa_key = rsa_from_components(n, e)?;
                parsed.insert(kid_key, rsa_key);
            }
        }
    }
    if parsed.is_empty() {
        return Err(OAuthError::IdToken("no RSA keys in JWKS".into()));
    }
    if let Ok(mut cache) = jwks_cache().lock() {
        cache.insert(jwks_uri.to_string(), parsed.clone());
    }
    if let Some(kid) = kid {
        parsed
            .get(kid)
            .cloned()
            .ok_or_else(|| OAuthError::IdToken(format!("kid {kid} not found in JWKS")))
    } else if parsed.len() == 1 {
        Ok(parsed.values().next().unwrap().clone())
    } else {
        Err(OAuthError::IdToken(
            "multiple JWKS keys; kid required".into(),
        ))
    }
}

fn rsa_from_components(n_b64: &str, e_b64: &str) -> OAuthResult<RsaPublicKey> {
    let n_bytes =
        decode_url_safe(n_b64).map_err(|_| OAuthError::IdToken("invalid JWK n".into()))?;
    let e_bytes =
        decode_url_safe(e_b64).map_err(|_| OAuthError::IdToken("invalid JWK e".into()))?;
    let n = BigUint::from_bytes_be(&n_bytes);
    let e = BigUint::from_bytes_be(&e_bytes);
    RsaPublicKey::new(n, e).map_err(|e| OAuthError::IdToken(format!("invalid RSA key: {e}")))
}

fn decode_segment(seg: &str) -> OAuthResult<String> {
    let bytes =
        decode_url_safe(seg).map_err(|_| OAuthError::IdToken("invalid base64 segment".into()))?;
    String::from_utf8(bytes).map_err(|_| OAuthError::IdToken("invalid utf8 in JWT".into()))
}

fn json_str_to_object(json: &str) -> OAuthResult<Object> {
    value_to_object(parse(json).map_err(|e| OAuthError::IdToken(e.to_string()))?)
}

fn map_jwt_err(e: JwtError) -> OAuthError {
    OAuthError::IdToken(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_crypto::jwt::{default_header_hs256, sign_hs256};

    #[test]
    fn verify_hs256_id_token() {
        let payload = r#"{"iss":"https://idp","aud":"cid","sub":"u1","exp":9999999999}"#;
        let token = sign_hs256(default_header_hs256(), payload, b"secret").unwrap();
        let client = OAuthClient::builder("cid", "https://idp/token")
            .client_secret("secret")
            .issuer("https://idp")
            .build()
            .unwrap();
        let val = IdTokenValidation {
            issuer: Some("https://idp".into()),
            audience: Some("cid".into()),
            validate_exp: false,
            ..Default::default()
        };
        let claims = verify_id_token(&client, &token, &val).unwrap();
        assert_eq!(
            claims.claims.get("sub").and_then(|v| v.as_str()),
            Some("u1")
        );
    }
}
