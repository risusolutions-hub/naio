//! TOTP (RFC 6238) — time-based one-time passwords.

use crate::digest::Digest;
use crate::error::OtpError;
use crate::hotp::{generate_hotp, validate_digits, verify_token, Hotp};

pub const DEFAULT_INTERVAL: u64 = 30;

#[derive(Debug, Clone)]
pub struct Totp {
    inner: Hotp,
    interval: u64,
}

impl Totp {
    pub fn new(
        secret_b32: &str,
        digits: u32,
        interval: u64,
        digest: Digest,
    ) -> Result<Self, OtpError> {
        if interval == 0 {
            return Err(OtpError::InvalidInterval(interval));
        }
        Ok(Self {
            inner: Hotp::new(secret_b32, digits, digest)?,
            interval,
        })
    }

    pub fn from_bytes(
        secret: &[u8],
        digits: u32,
        interval: u64,
        digest: Digest,
    ) -> Result<Self, OtpError> {
        if interval == 0 {
            return Err(OtpError::InvalidInterval(interval));
        }
        Ok(Self {
            inner: Hotp::from_bytes(secret, digits, digest)?,
            interval,
        })
    }

    pub fn with_labels(mut self, name: Option<String>, issuer: Option<String>) -> Self {
        self.inner = self.inner.with_labels(name, issuer);
        self
    }

    pub fn digits(&self) -> u32 {
        self.inner.digits()
    }

    pub fn interval(&self) -> u64 {
        self.interval
    }

    pub fn digest(&self) -> Digest {
        self.inner.digest()
    }

    pub fn secret_base32(&self) -> String {
        self.inner.secret_base32()
    }

    pub fn at(&self, unix_time: u64) -> String {
        let counter = unix_time / self.interval;
        generate_hotp(
            self.inner.secret_bytes(),
            counter,
            self.inner.digits(),
            self.inner.digest(),
        )
    }

    pub fn now(&self, unix_time: u64) -> String {
        self.at(unix_time)
    }

    pub fn verify(&self, token: &str, unix_time: u64, window: u64) -> bool {
        self.verify_at(token, unix_time, window).is_some()
    }

    /// Return the Unix timestamp of the matching time step, if any.
    pub fn verify_at(&self, token: &str, unix_time: u64, window: u64) -> Option<u64> {
        for offset in 0..=window {
            for delta in [0i64, -(offset as i64), offset as i64] {
                if delta == 0 && offset > 0 {
                    continue;
                }
                let t = if delta < 0 {
                    unix_time.saturating_sub((-delta) as u64 * self.interval)
                } else {
                    unix_time.saturating_add(delta as u64 * self.interval)
                };
                if verify_token(&self.at(t), token) {
                    return Some(t);
                }
            }
        }
        None
    }

    pub fn provisioning_uri(&self, name: &str, issuer: Option<&str>) -> String {
        crate::uri::build_totp_uri(
            name,
            issuer.or(self.inner.issuer()),
            &self.secret_base32(),
            self.digits(),
            self.digest(),
            self.interval,
        )
    }
}

pub fn totp_at(
    secret_b32: &str,
    unix_time: u64,
    digits: u32,
    interval: u64,
    digest: Digest,
) -> Result<String, OtpError> {
    validate_digits(digits)?;
    if interval == 0 {
        return Err(OtpError::InvalidInterval(interval));
    }
    let secret = crate::base32::decode(secret_b32)?;
    let counter = unix_time / interval;
    Ok(generate_hotp(&secret, counter, digits, digest))
}

/// Batch TOTP at many Unix timestamps (parallel for large slices).
pub fn totp_at_bulk(
    secret_b32: &str,
    times: &[u64],
    digits: u32,
    interval: u64,
    digest: Digest,
) -> Result<Vec<String>, OtpError> {
    validate_digits(digits)?;
    if interval == 0 {
        return Err(OtpError::InvalidInterval(interval));
    }
    let secret = crate::base32::decode(secret_b32)?;
    if times.len() < 256 {
        return Ok(times
            .iter()
            .map(|&t| generate_hotp(&secret, t / interval, digits, digest))
            .collect());
    }
    use rayon::prelude::*;
    Ok(times
        .par_iter()
        .map(|&t| generate_hotp(&secret, t / interval, digits, digest))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::digest::DEFAULT_DIGEST;
    use crate::hotp::DEFAULT_DIGITS;

    const RFC_SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
    const GOOGLE_SECRET: &str = "JBSWY3DPEHPK3PXP";

    #[test]
    fn rfc6238_sha1_vectors() {
        let t = Totp::new(RFC_SECRET, 8, DEFAULT_INTERVAL, DEFAULT_DIGEST).unwrap();
        assert_eq!(t.at(59), "94287082");
        assert_eq!(t.at(1111111109), "07081804");
        assert_eq!(t.at(1111111111), "14050471");
        assert_eq!(t.at(1234567890), "89005924");
        assert_eq!(t.at(2000000000), "69279037");
    }

    #[test]
    fn six_digit_default() {
        let t = Totp::new(RFC_SECRET, DEFAULT_DIGITS, DEFAULT_INTERVAL, DEFAULT_DIGEST).unwrap();
        assert_eq!(t.at(59), "287082");
        assert_eq!(t.at(1111111109), "081804");
        assert_eq!(t.at(1111111111), "050471");
    }

    #[test]
    fn google_example_totp() {
        let t = Totp::new(
            GOOGLE_SECRET,
            DEFAULT_DIGITS,
            DEFAULT_INTERVAL,
            DEFAULT_DIGEST,
        )
        .unwrap();
        assert_eq!(t.at(59), "996554");
    }

    #[test]
    fn verify_window() {
        let t = Totp::new(RFC_SECRET, DEFAULT_DIGITS, DEFAULT_INTERVAL, DEFAULT_DIGEST).unwrap();
        let code = t.at(1111111111);
        assert!(t.verify(&code, 1111111111, 0));
        assert!(t.verify(&code, 1111111111 + 30, 1));
        assert!(!t.verify("000000", 1111111111, 0));
    }
}
