use crate::algo::{is_hmac, parse_alg};
use crate::error::JwtError;
use jsonwebtoken::jwk::KeyAlgorithm;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey};

/// Signing or verifying key material.
#[derive(Clone)]
pub struct Key {
    pub alg: Option<Algorithm>,
    secret: Option<Vec<u8>>,
    enc: Option<EncodingKey>,
    dec: Option<DecodingKey>,
}

impl Key {
    pub fn from_secret(secret: &[u8], alg: Option<&str>) -> Result<Self, JwtError> {
        let algo = alg.map(parse_alg).transpose()?.unwrap_or(Algorithm::HS256);
        if !is_hmac(algo) {
            return Err(JwtError::Key(format!(
                "algorithm {algo:?} requires PEM or JWK key, not a shared secret"
            )));
        }
        Ok(Self {
            alg: Some(algo),
            secret: Some(secret.to_vec()),
            enc: Some(EncodingKey::from_secret(secret)),
            dec: Some(DecodingKey::from_secret(secret)),
        })
    }

    pub fn from_pem(pem: &str, alg: Option<&str>) -> Result<Self, JwtError> {
        let algo = alg.map(parse_alg).transpose()?;
        let enc = EncodingKey::from_rsa_pem(pem.as_bytes())
            .or_else(|_| EncodingKey::from_ec_pem(pem.as_bytes()))
            .or_else(|_| EncodingKey::from_ed_pem(pem.as_bytes()))
            .map_err(|e| JwtError::Key(e.to_string()))?;
        let dec = DecodingKey::from_rsa_pem(pem.as_bytes())
            .or_else(|_| DecodingKey::from_ec_pem(pem.as_bytes()))
            .or_else(|_| DecodingKey::from_ed_pem(pem.as_bytes()))
            .map_err(|e| JwtError::Key(e.to_string()))?;
        Ok(Self {
            alg: algo,
            secret: None,
            enc: Some(enc),
            dec: Some(dec),
        })
    }

    pub fn from_jwk_json(json: &str) -> Result<Self, JwtError> {
        use jsonwebtoken::jwk::JwkSet;
        let set: JwkSet = serde_json::from_str(json).map_err(|e| JwtError::Jwks(e.to_string()))?;
        let jwk = set
            .keys
            .first()
            .ok_or_else(|| JwtError::Jwks("JWKS contains no keys".into()))?;
        Self::from_jwk(jwk)
    }

    pub fn from_jwk(jwk: &jsonwebtoken::jwk::Jwk) -> Result<Self, JwtError> {
        let dec = DecodingKey::from_jwk(jwk).map_err(|e| JwtError::Jwks(e.to_string()))?;
        let alg = jwk.common.key_algorithm.and_then(key_alg_to_algorithm);
        Ok(Self {
            alg,
            secret: None,
            enc: None,
            dec: Some(dec),
        })
    }

    pub fn encoding(&self) -> Result<&EncodingKey, JwtError> {
        self.enc
            .as_ref()
            .ok_or_else(|| JwtError::Key("key cannot sign (missing private material)".into()))
    }

    pub fn decoding(&self) -> Result<&DecodingKey, JwtError> {
        self.dec
            .as_ref()
            .ok_or_else(|| JwtError::Key("key cannot verify".into()))
    }

    pub fn hmac_secret(&self) -> Option<&[u8]> {
        self.secret.as_deref()
    }
}

fn key_alg_to_algorithm(a: KeyAlgorithm) -> Option<Algorithm> {
    Some(match a {
        KeyAlgorithm::HS256 => Algorithm::HS256,
        KeyAlgorithm::HS384 => Algorithm::HS384,
        KeyAlgorithm::HS512 => Algorithm::HS512,
        KeyAlgorithm::ES256 => Algorithm::ES256,
        KeyAlgorithm::ES384 => Algorithm::ES384,
        KeyAlgorithm::RS256 => Algorithm::RS256,
        KeyAlgorithm::RS384 => Algorithm::RS384,
        KeyAlgorithm::RS512 => Algorithm::RS512,
        KeyAlgorithm::PS256 => Algorithm::PS256,
        KeyAlgorithm::PS384 => Algorithm::PS384,
        KeyAlgorithm::PS512 => Algorithm::PS512,
        KeyAlgorithm::EdDSA => Algorithm::EdDSA,
        _ => return None,
    })
}
