//! HMAC digest selection for HOTP/TOTP.

use crate::error::OtpError;
use niao_crypto::{hmac_sha1, hmac_sha256, hmac_sha512};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Digest {
    Sha1,
    Sha256,
    Sha512,
}

impl Digest {
    pub fn parse(s: &str) -> Result<Self, OtpError> {
        match s.to_ascii_lowercase().as_str() {
            "sha1" | "sha-1" => Ok(Self::Sha1),
            "sha256" | "sha-256" => Ok(Self::Sha256),
            "sha512" | "sha-512" => Ok(Self::Sha512),
            other => Err(OtpError::InvalidDigest(other.to_string())),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Sha1 => "SHA1",
            Self::Sha256 => "SHA256",
            Self::Sha512 => "SHA512",
        }
    }

    #[inline]
    pub fn hmac(&self, key: &[u8], data: &[u8]) -> Vec<u8> {
        match self {
            Self::Sha1 => hmac_sha1(key, data).to_vec(),
            Self::Sha256 => hmac_sha256(key, data).to_vec(),
            Self::Sha512 => hmac_sha512(key, data).to_vec(),
        }
    }
}

pub const DEFAULT_DIGEST: Digest = Digest::Sha1;
