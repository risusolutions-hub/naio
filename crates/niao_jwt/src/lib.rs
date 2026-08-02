//! JWT / JWS sign + verify (HS/RS/ES/EdDSA), claims validation, JWKS fetch.
//! ~PyJWT / python-jose subset.

mod algo;
mod decode;
mod error;
mod jwks;
mod keys;
mod options;
mod sign;
mod verify;

pub use algo::SUPPORTED;
pub use decode::{claims_unverified, decode_unverified, header, valid};
pub use error::JwtError;
pub use jwks::{
    fetch_jwks, jwks_from_value, jwks_to_value, parse_jwks, verify_all, verify_jwks, Jwks,
};
pub use keys::Key;
pub use options::{FetchOptions, SignOptions, VerifyOptions};
pub use sign::{sign, sign_hs256_default, sign_hs512_default};
pub use verify::{header_alg, now_secs, verify};

#[cfg(test)]
mod tests {
    use super::*;
    use niao_json_core::parse;

    const SECRET: &[u8] = b"your-256-bit-secret";

    #[test]
    fn hs256_roundtrip() {
        let claims = parse(r#"{"sub":"user1","exp":9999999999}"#).unwrap();
        let token = sign_hs256_default(&claims, SECRET).unwrap();
        let opts = VerifyOptions {
            validate_exp: false,
            ..Default::default()
        };
        let out = verify(
            &token,
            &Key::from_secret(SECRET, Some("HS256")).unwrap(),
            &opts,
        )
        .unwrap();
        assert_eq!(out.get("sub").and_then(|v| v.as_str()), Some("user1"));
    }

    #[test]
    fn jwt_io_vector() {
        let token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.\
            eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiYWRtaW4iOnRydWUsImlhdCI6MTUxNjIzOTAyMn0.\
            reGQzG3OKdoIMWLDKOZ4TICJit3EW69cQE72E2CfzRE";
        let opts = VerifyOptions {
            validate_exp: false,
            ..Default::default()
        };
        let out = verify(
            token,
            &Key::from_secret(SECRET, Some("HS256")).unwrap(),
            &opts,
        )
        .unwrap();
        assert_eq!(out.get("sub").and_then(|v| v.as_str()), Some("1234567890"));
    }

    #[test]
    fn reject_alg_none() {
        let token = "eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0.eyJzdWIiOiIxMjM0NTY3ODkwIn0.";
        let opts = VerifyOptions::default();
        assert!(verify(
            token,
            &Key::from_secret(b"x", Some("HS256")).unwrap(),
            &opts
        )
        .is_err());
    }

    #[test]
    fn exp_validation() {
        let claims = parse(r#"{"sub":"u","exp":1}"#).unwrap();
        let token = sign_hs256_default(&claims, SECRET).unwrap();
        let opts = VerifyOptions::default();
        assert!(matches!(
            verify(
                &token,
                &Key::from_secret(SECRET, Some("HS256")).unwrap(),
                &opts
            ),
            Err(JwtError::Expired)
        ));
    }
}
