use crate::error::{NcryptError, NcryptResult};
use hkdf::Hkdf;
use pbkdf2::pbkdf2_hmac;
use sha2::{Sha256, Sha512};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HashAlg {
    Sha256,
    Sha512,
}

impl HashAlg {
    pub fn parse(name: &str) -> NcryptResult<Self> {
        match name.to_ascii_lowercase().as_str() {
            "sha256" | "sha-256" | "sha2-256" => Ok(Self::Sha256),
            "sha512" | "sha-512" | "sha2-512" => Ok(Self::Sha512),
            other => Err(NcryptError::Unsupported(format!(
                "unsupported hash '{other}'"
            ))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sha256 => "sha256",
            Self::Sha512 => "sha512",
        }
    }
}

/// HKDF one-shot derive (extract + expand).
pub fn hkdf(
    ikm: &[u8],
    length: usize,
    salt: Option<&[u8]>,
    info: Option<&[u8]>,
    hash: HashAlg,
) -> NcryptResult<Vec<u8>> {
    if length == 0 || length > 255 * 32 {
        return Err(NcryptError::InvalidArgument(
            "HKDF length must be 1..=8160".into(),
        ));
    }
    let salt = salt.unwrap_or(&[]);
    let info = info.unwrap_or(&[]);
    let mut okm = vec![0u8; length];
    match hash {
        HashAlg::Sha256 => {
            let hk = Hkdf::<Sha256>::new(Some(salt), ikm);
            hk.expand(info, &mut okm)
                .map_err(|_| NcryptError::InvalidArgument("HKDF expand failed".into()))?;
        }
        HashAlg::Sha512 => {
            let hk = Hkdf::<Sha512>::new(Some(salt), ikm);
            hk.expand(info, &mut okm)
                .map_err(|_| NcryptError::InvalidArgument("HKDF expand failed".into()))?;
        }
    }
    Ok(okm)
}

/// HKDF-Extract only.
pub fn hkdf_extract(ikm: &[u8], salt: Option<&[u8]>, hash: HashAlg) -> NcryptResult<Vec<u8>> {
    let salt = salt.unwrap_or(&[]);
    let prk = match hash {
        HashAlg::Sha256 => Hkdf::<Sha256>::extract(Some(salt), ikm).0.to_vec(),
        HashAlg::Sha512 => Hkdf::<Sha512>::extract(Some(salt), ikm).0.to_vec(),
    };
    Ok(prk)
}

/// HKDF-Expand only.
pub fn hkdf_expand(
    prk: &[u8],
    length: usize,
    info: Option<&[u8]>,
    hash: HashAlg,
) -> NcryptResult<Vec<u8>> {
    if length == 0 || length > 255 * 32 {
        return Err(NcryptError::InvalidArgument(
            "HKDF length must be 1..=8160".into(),
        ));
    }
    let info = info.unwrap_or(&[]);
    let mut okm = vec![0u8; length];
    match hash {
        HashAlg::Sha256 => {
            let hk = Hkdf::<Sha256>::from_prk(prk)
                .map_err(|_| NcryptError::InvalidKey("invalid HKDF PRK length".into()))?;
            hk.expand(info, &mut okm)
                .map_err(|_| NcryptError::InvalidArgument("HKDF expand failed".into()))?;
        }
        HashAlg::Sha512 => {
            let hk = Hkdf::<Sha512>::from_prk(prk)
                .map_err(|_| NcryptError::InvalidKey("invalid HKDF PRK length".into()))?;
            hk.expand(info, &mut okm)
                .map_err(|_| NcryptError::InvalidArgument("HKDF expand failed".into()))?;
        }
    }
    Ok(okm)
}

/// PBKDF2-HMAC key derivation.
pub fn pbkdf2_derive(
    password: &[u8],
    salt: &[u8],
    iterations: u32,
    length: usize,
    hash: HashAlg,
) -> NcryptResult<Vec<u8>> {
    if iterations == 0 {
        return Err(NcryptError::InvalidArgument(
            "PBKDF2 iterations must be >= 1".into(),
        ));
    }
    if length == 0 || length > 1024 {
        return Err(NcryptError::InvalidArgument(
            "PBKDF2 length must be 1..=1024".into(),
        ));
    }
    if salt.is_empty() {
        return Err(NcryptError::InvalidArgument(
            "PBKDF2 salt must not be empty".into(),
        ));
    }
    let mut out = vec![0u8; length];
    match hash {
        HashAlg::Sha256 => pbkdf2_hmac::<Sha256>(password, salt, iterations, &mut out),
        HashAlg::Sha512 => pbkdf2_hmac::<Sha512>(password, salt, iterations, &mut out),
    }
    Ok(out)
}
