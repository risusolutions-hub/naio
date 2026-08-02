use crate::error::{OAuthError, OAuthResult};
use crate::random::random_verifier;
use niao_codec::base64::encode_url_safe_no_pad;
use niao_crypto::sha256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PkceChallengeMethod {
    S256,
    Plain,
}

impl PkceChallengeMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::S256 => "S256",
            Self::Plain => "plain",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            "S256" => Some(Self::S256),
            "PLAIN" => Some(Self::Plain),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PkcePair {
    pub verifier: String,
    pub challenge: String,
    pub method: PkceChallengeMethod,
}

/// Build PKCE pair with S256 challenge (default).
///
/// >>> use niao_oauth::pkce_pair;
/// >>> let p = pkce_pair(true);
/// >>> p.method.as_str() == "S256" && !p.verifier.is_empty()
/// true
pub fn pkce_pair(use_s256: bool) -> PkcePair {
    let verifier = random_verifier();
    let method = if use_s256 {
        PkceChallengeMethod::S256
    } else {
        PkceChallengeMethod::Plain
    };
    let challenge = pkce_challenge(&verifier, method);
    PkcePair {
        verifier,
        challenge,
        method,
    }
}

/// Compute PKCE code challenge from verifier.
///
/// >>> use niao_oauth::{pkce_challenge, PkceChallengeMethod};
/// >>> pkce_challenge("abc", PkceChallengeMethod::Plain) == "abc"
/// true
pub fn pkce_challenge(verifier: &str, method: PkceChallengeMethod) -> String {
    match method {
        PkceChallengeMethod::Plain => verifier.to_string(),
        PkceChallengeMethod::S256 => encode_url_safe_no_pad(&sha256(verifier.as_bytes())),
    }
}

/// Validate verifier length per RFC 7636.
pub fn validate_verifier(verifier: &str) -> OAuthResult<()> {
    let len = verifier.len();
    if (43..=128).contains(&len) {
        Ok(())
    } else {
        Err(OAuthError::Pkce(format!(
            "code_verifier length must be 43..=128, got {len}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s256_known_vector() {
        // RFC 7636 appendix B
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = pkce_challenge(verifier, PkceChallengeMethod::S256);
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }
}
