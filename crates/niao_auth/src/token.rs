//! Random URL-safe tokens and constant-time string compare.

use crate::error::{AuthError, AuthResult};
use niao_codec::base64::encode_url_safe_no_pad;
use niao_crypto::constant_time_eq;
use niao_rand::fill_os_random;

/// Default random token size in bytes (before encoding).
pub const DEFAULT_TOKEN_BYTES: usize = 32;

/// Generate a cryptographically random URL-safe token.
pub fn generate_token(nbytes: usize) -> AuthResult<String> {
    if nbytes == 0 || nbytes > 1024 {
        return Err(AuthError::InvalidParameter(
            "token nbytes must be 1..=1024".into(),
        ));
    }
    let mut buf = vec![0u8; nbytes];
    fill_os_random(&mut buf);
    Ok(encode_url_safe_no_pad(&buf))
}

/// Constant-time equality for UTF-8 strings (length-mismatch-safe).
pub fn compare(a: &str, b: &str) -> bool {
    constant_time_eq(a.as_bytes(), b.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_length_and_uniqueness() {
        let a = generate_token(16).unwrap();
        let b = generate_token(16).unwrap();
        assert_ne!(a, b);
        assert!(!a.is_empty());
    }

    #[test]
    fn compare_eq() {
        assert!(compare("abc", "abc"));
        assert!(!compare("abc", "abd"));
        assert!(!compare("abc", "ab"));
    }

    #[test]
    fn token_rejects_zero() {
        assert!(generate_token(0).is_err());
    }
}
