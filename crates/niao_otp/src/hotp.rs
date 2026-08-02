//! HOTP (RFC 4226) — counter-based one-time passwords.

use crate::base32;
use crate::digest::Digest;
use crate::error::OtpError;
use niao_crypto::constant_time_eq;

pub const DEFAULT_DIGITS: u32 = 6;
pub const MIN_DIGITS: u32 = 1;
pub const MAX_DIGITS: u32 = 10;

#[derive(Debug, Clone)]
pub struct Hotp {
    secret: Vec<u8>,
    digits: u32,
    digest: Digest,
    issuer: Option<String>,
    name: Option<String>,
}

impl Hotp {
    pub fn new(secret_b32: &str, digits: u32, digest: Digest) -> Result<Self, OtpError> {
        let secret = base32::decode(secret_b32)?;
        if secret.is_empty() {
            return Err(OtpError::InvalidSecret("empty after base32 decode".into()));
        }
        validate_digits(digits)?;
        Ok(Self {
            secret,
            digits,
            digest,
            issuer: None,
            name: None,
        })
    }

    pub fn from_bytes(secret: &[u8], digits: u32, digest: Digest) -> Result<Self, OtpError> {
        if secret.is_empty() {
            return Err(OtpError::InvalidSecret("empty secret".into()));
        }
        validate_digits(digits)?;
        Ok(Self {
            secret: secret.to_vec(),
            digits,
            digest,
            issuer: None,
            name: None,
        })
    }

    pub fn with_labels(mut self, name: Option<String>, issuer: Option<String>) -> Self {
        self.name = name;
        self.issuer = issuer;
        self
    }

    pub fn digits(&self) -> u32 {
        self.digits
    }

    pub fn digest(&self) -> Digest {
        self.digest
    }

    pub fn issuer(&self) -> Option<&str> {
        self.issuer.as_deref()
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn secret_base32(&self) -> String {
        base32::encode(&self.secret)
    }

    pub(crate) fn secret_bytes(&self) -> &[u8] {
        &self.secret
    }

    pub fn at(&self, counter: u64) -> String {
        generate_hotp(&self.secret, counter, self.digits, self.digest)
    }

    pub fn verify(&self, token: &str, counter: u64) -> bool {
        verify_token(&self.at(counter), token)
    }

    /// Verify at `counter` and look ahead up to `window` steps (RFC 4226 look-ahead).
    pub fn verify_window(&self, token: &str, counter: u64, window: u64) -> Option<u64> {
        for i in 0..=window {
            let c = counter.saturating_add(i);
            if self.verify(token, c) {
                return Some(c);
            }
        }
        None
    }

    pub fn provisioning_uri(
        &self,
        name: &str,
        issuer: Option<&str>,
        counter: Option<u64>,
    ) -> String {
        crate::uri::build_hotp_uri(
            name,
            issuer.or(self.issuer.as_deref()),
            &self.secret_base32(),
            self.digits,
            self.digest,
            counter,
        )
    }
}

pub fn hotp_at(
    secret_b32: &str,
    counter: u64,
    digits: u32,
    digest: Digest,
) -> Result<String, OtpError> {
    let secret = base32::decode(secret_b32)?;
    validate_digits(digits)?;
    Ok(generate_hotp(&secret, counter, digits, digest))
}

#[inline]
pub fn generate_hotp(secret: &[u8], counter: u64, digits: u32, digest: Digest) -> String {
    let msg = counter.to_be_bytes();
    let hmac = digest.hmac(secret, &msg);
    dynamic_truncate(&hmac, digits)
}

fn dynamic_truncate(hmac: &[u8], digits: u32) -> String {
    let offset = (hmac[hmac.len() - 1] & 0x0f) as usize;
    let binary = ((u32::from(hmac[offset]) & 0x7f) << 24)
        | (u32::from(hmac[offset + 1]) << 16)
        | (u32::from(hmac[offset + 2]) << 8)
        | u32::from(hmac[offset + 3]);
    let modulo = 10_u32.pow(digits);
    let otp = binary % modulo;
    format!("{otp:0width$}", width = digits as usize)
}

pub fn verify_token(expected: &str, token: &str) -> bool {
    if expected.len() != token.len() {
        return false;
    }
    constant_time_eq(expected.as_bytes(), token.as_bytes())
}

pub fn validate_digits(digits: u32) -> Result<(), OtpError> {
    if !(MIN_DIGITS..=MAX_DIGITS).contains(&digits) {
        return Err(OtpError::InvalidDigits(digits));
    }
    Ok(())
}

/// Batch HOTP at many counters (parallel for large slices).
pub fn hotp_at_bulk(
    secret_b32: &str,
    counters: &[u64],
    digits: u32,
    digest: Digest,
) -> Result<Vec<String>, OtpError> {
    let secret = base32::decode(secret_b32)?;
    validate_digits(digits)?;
    if counters.len() < 256 {
        return Ok(counters
            .iter()
            .map(|&c| generate_hotp(&secret, c, digits, digest))
            .collect());
    }
    use rayon::prelude::*;
    Ok(counters
        .par_iter()
        .map(|&c| generate_hotp(&secret, c, digits, digest))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::digest::DEFAULT_DIGEST;

    const SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    #[test]
    fn rfc4226_vectors() {
        let h = Hotp::new(SECRET, DEFAULT_DIGITS, DEFAULT_DIGEST).unwrap();
        assert_eq!(h.at(0), "755224");
        assert_eq!(h.at(1), "287082");
        assert_eq!(h.at(2), "359152");
        assert_eq!(h.at(3), "969429");
        assert_eq!(h.at(4), "338314");
        assert_eq!(h.at(5), "254676");
        assert_eq!(h.at(6), "287922");
        assert_eq!(h.at(7), "162583");
        assert_eq!(h.at(8), "399871");
        assert_eq!(h.at(9), "520489");
    }

    #[test]
    fn verify_window() {
        let h = Hotp::new(SECRET, DEFAULT_DIGITS, DEFAULT_DIGEST).unwrap();
        assert_eq!(h.verify_window("287082", 0, 2), Some(1));
        assert_eq!(h.verify_window("000000", 0, 0), None);
    }

    #[test]
    fn google_example_secret() {
        let h = Hotp::new("JBSWY3DPEHPK3PXP", DEFAULT_DIGITS, DEFAULT_DIGEST).unwrap();
        assert_eq!(h.at(0), "282760");
        assert_eq!(h.at(1), "996554");
    }
}
