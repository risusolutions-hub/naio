use crate::error::JwtError;
use niao_codec::base64::decode_url_safe;
use niao_json_core::Value;

pub fn decode_unverified(token: &str) -> Result<(Value, Value), JwtError> {
    niao_crypto::jwt::decode_unverified(token).map_err(map_hs_err)
}

pub fn header(token: &str) -> Result<Value, JwtError> {
    let (header, _) = decode_unverified(token)?;
    Ok(header)
}

pub fn claims_unverified(token: &str) -> Result<Value, JwtError> {
    let (_, payload) = decode_unverified(token)?;
    Ok(payload)
}

pub fn valid(token: &str) -> bool {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 || parts.iter().any(|p| p.is_empty()) {
        return false;
    }
    for part in parts {
        if decode_url_safe(part).is_err() {
            return false;
        }
    }
    decode_unverified(token).is_ok()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_jwt_io_token() {
        let token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.\
            eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiYWRtaW4iOnRydWUsImlhdCI6MTUxNjIzOTAyMn0.\
            reGQzG3OKdoIMWLDKOZ4TICJit3EW69cQE72E2CfzRE";
        assert!(valid(token));
        let (h, p) = decode_unverified(token).unwrap();
        assert_eq!(h.get("alg").and_then(|v| v.as_str()), Some("HS256"));
        assert_eq!(p.get("sub").and_then(|v| v.as_str()), Some("1234567890"));
    }

    #[test]
    fn invalid_format() {
        assert!(!valid("not.a.jwt"));
        assert!(!valid("a.b"));
    }
}
