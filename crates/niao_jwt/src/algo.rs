use crate::error::JwtError;
use jsonwebtoken::Algorithm;

pub const SUPPORTED: &[&str] = &[
    "HS256", "HS384", "HS512", "RS256", "RS384", "RS512", "PS256", "PS384", "PS512", "ES256",
    "ES384", "EdDSA",
];

pub fn parse_alg(s: &str) -> Result<Algorithm, JwtError> {
    match s {
        "HS256" => Ok(Algorithm::HS256),
        "HS384" => Ok(Algorithm::HS384),
        "HS512" => Ok(Algorithm::HS512),
        "RS256" => Ok(Algorithm::RS256),
        "RS384" => Ok(Algorithm::RS384),
        "RS512" => Ok(Algorithm::RS512),
        "PS256" => Ok(Algorithm::PS256),
        "PS384" => Ok(Algorithm::PS384),
        "PS512" => Ok(Algorithm::PS512),
        "ES256" => Ok(Algorithm::ES256),
        "ES384" => Ok(Algorithm::ES384),
        "EdDSA" => Ok(Algorithm::EdDSA),
        "none" | "None" | "NONE" => Err(JwtError::Algorithm),
        other => Err(JwtError::Message(format!(
            "unsupported algorithm '{other}'"
        ))),
    }
}

pub fn is_hmac(algo: Algorithm) -> bool {
    matches!(algo, Algorithm::HS256 | Algorithm::HS384 | Algorithm::HS512)
}

pub fn is_fast_hmac(algo: Algorithm) -> bool {
    matches!(algo, Algorithm::HS256 | Algorithm::HS512)
}
