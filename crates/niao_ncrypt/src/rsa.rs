use crate::error::{check_len, NcryptError, NcryptResult};
use niao_crypto::{sha256, sha512};
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pkcs1v15::{SigningKey, VerifyingKey};
use rsa::pkcs8::{
    DecodePrivateKey, DecodePublicKey, EncodePrivateKey, EncodePublicKey, LineEnding,
};
use rsa::pss::{BlindedSigningKey, Signature, VerifyingKey as PssVerifyingKey};
use rsa::sha2::{Sha256, Sha512};
use rsa::signature::{RandomizedSigner, SignatureEncoding, Verifier};
use rsa::traits::PublicKeyParts;
use rsa::{Oaep, Pkcs1v15Encrypt, RsaPrivateKey, RsaPublicKey};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RsaPadding {
    OaepSha256,
    OaepSha512,
    Pkcs1v15,
}

impl RsaPadding {
    pub fn parse(name: &str) -> NcryptResult<Self> {
        match name.to_ascii_lowercase().as_str() {
            "oaep" | "oaep-sha256" | "oaep_sha256" => Ok(Self::OaepSha256),
            "oaep-sha512" | "oaep_sha512" => Ok(Self::OaepSha512),
            "pkcs1" | "pkcs1v15" | "pkcs1_v1_5" => Ok(Self::Pkcs1v15),
            other => Err(NcryptError::Unsupported(format!(
                "unknown RSA padding '{other}'"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RsaHash {
    Sha256,
    Sha512,
}

impl RsaHash {
    pub fn parse(name: &str) -> NcryptResult<Self> {
        match name.to_ascii_lowercase().as_str() {
            "sha256" | "sha-256" => Ok(Self::Sha256),
            "sha512" | "sha-512" => Ok(Self::Sha512),
            other => Err(NcryptError::Unsupported(format!(
                "unknown RSA hash '{other}'"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RsaSignPadding {
    Pss,
    Pkcs1v15,
}

impl RsaSignPadding {
    pub fn parse(name: &str) -> NcryptResult<Self> {
        match name.to_ascii_lowercase().as_str() {
            "pss" | "rsa-pss" => Ok(Self::Pss),
            "pkcs1" | "pkcs1v15" | "pkcs1_v1_5" => Ok(Self::Pkcs1v15),
            other => Err(NcryptError::Unsupported(format!(
                "unknown RSA sign padding '{other}'"
            ))),
        }
    }
}

pub struct RsaKeyPair {
    pub private: RsaPrivateKey,
    pub public: RsaPublicKey,
}

pub fn rsa_generate(bits: usize) -> NcryptResult<RsaKeyPair> {
    if !matches!(bits, 2048 | 3072 | 4096) {
        return Err(NcryptError::InvalidArgument(
            "RSA key size must be 2048, 3072, or 4096".into(),
        ));
    }
    let mut rng = rand::thread_rng();
    let private = RsaPrivateKey::new(&mut rng, bits)
        .map_err(|e| NcryptError::InvalidArgument(format!("RSA key generation failed: {e}")))?;
    let public = RsaPublicKey::from(&private);
    Ok(RsaKeyPair { private, public })
}

pub fn rsa_public_from_pem(pem: &str) -> NcryptResult<RsaPublicKey> {
    RsaPublicKey::from_public_key_pem(pem)
        .map_err(|e| NcryptError::ParseFailed(format!("invalid RSA public PEM: {e}")))
}

pub fn rsa_private_from_pem(pem: &str) -> NcryptResult<RsaPrivateKey> {
    RsaPrivateKey::from_pkcs8_pem(pem)
        .or_else(|_| RsaPrivateKey::from_pkcs1_pem(pem))
        .map_err(|e| NcryptError::ParseFailed(format!("invalid RSA private PEM: {e}")))
}

pub fn rsa_public_to_pem(key: &RsaPublicKey) -> NcryptResult<String> {
    key.to_public_key_pem(LineEnding::LF)
        .map_err(|e| NcryptError::InvalidArgument(format!("RSA public PEM encode failed: {e}")))
}

pub fn rsa_private_to_pem(key: &RsaPrivateKey) -> NcryptResult<String> {
    key.to_pkcs8_pem(LineEnding::LF)
        .map_err(|e| NcryptError::InvalidArgument(format!("RSA private PEM encode failed: {e}")))
        .map(|s| s.to_string())
}

pub fn rsa_encrypt(
    public: &RsaPublicKey,
    data: &[u8],
    padding: RsaPadding,
) -> NcryptResult<Vec<u8>> {
    check_len(data.len())?;
    let mut rng = rand::thread_rng();
    match padding {
        RsaPadding::OaepSha256 => {
            let enc = Oaep::new::<Sha256>();
            public
                .encrypt(&mut rng, enc, data)
                .map_err(|e| NcryptError::EncryptFailed(format!("RSA-OAEP encrypt failed: {e}")))
        }
        RsaPadding::OaepSha512 => {
            let enc = Oaep::new::<Sha512>();
            public
                .encrypt(&mut rng, enc, data)
                .map_err(|e| NcryptError::EncryptFailed(format!("RSA-OAEP encrypt failed: {e}")))
        }
        RsaPadding::Pkcs1v15 => public
            .encrypt(&mut rng, Pkcs1v15Encrypt, data)
            .map_err(|e| {
                NcryptError::EncryptFailed(format!("RSA PKCS#1 v1.5 encrypt failed: {e}"))
            }),
    }
}

pub fn rsa_decrypt(
    private: &RsaPrivateKey,
    data: &[u8],
    padding: RsaPadding,
) -> NcryptResult<Vec<u8>> {
    check_len(data.len())?;
    match padding {
        RsaPadding::OaepSha256 => {
            let dec = Oaep::new::<Sha256>();
            private
                .decrypt(dec, data)
                .map_err(|e| NcryptError::DecryptFailed(format!("RSA-OAEP decrypt failed: {e}")))
        }
        RsaPadding::OaepSha512 => {
            let dec = Oaep::new::<Sha512>();
            private
                .decrypt(dec, data)
                .map_err(|e| NcryptError::DecryptFailed(format!("RSA-OAEP decrypt failed: {e}")))
        }
        RsaPadding::Pkcs1v15 => private.decrypt(Pkcs1v15Encrypt, data).map_err(|e| {
            NcryptError::DecryptFailed(format!("RSA PKCS#1 v1.5 decrypt failed: {e}"))
        }),
    }
}

pub fn rsa_sign(
    private: &RsaPrivateKey,
    data: &[u8],
    hash: RsaHash,
    padding: RsaSignPadding,
) -> NcryptResult<Vec<u8>> {
    check_len(data.len())?;
    let mut rng = rand::thread_rng();
    match (hash, padding) {
        (RsaHash::Sha256, RsaSignPadding::Pss) => {
            let key = BlindedSigningKey::<Sha256>::new(private.clone());
            Ok(key.sign_with_rng(&mut rng, data).to_bytes().to_vec())
        }
        (RsaHash::Sha512, RsaSignPadding::Pss) => {
            let key = BlindedSigningKey::<Sha512>::new(private.clone());
            Ok(key.sign_with_rng(&mut rng, data).to_bytes().to_vec())
        }
        (RsaHash::Sha256, RsaSignPadding::Pkcs1v15) => {
            let digest = sha256(data);
            let key = SigningKey::<Sha256>::new_unprefixed(private.clone());
            Ok(key.sign_with_rng(&mut rng, &digest).to_bytes().to_vec())
        }
        (RsaHash::Sha512, RsaSignPadding::Pkcs1v15) => {
            let digest = sha512(data);
            let key = SigningKey::<Sha512>::new_unprefixed(private.clone());
            Ok(key.sign_with_rng(&mut rng, &digest).to_bytes().to_vec())
        }
    }
}

pub fn rsa_verify(
    public: &RsaPublicKey,
    data: &[u8],
    signature: &[u8],
    hash: RsaHash,
    padding: RsaSignPadding,
) -> NcryptResult<bool> {
    check_len(data.len())?;
    let ok = match (hash, padding) {
        (RsaHash::Sha256, RsaSignPadding::Pss) => {
            let key = PssVerifyingKey::<Sha256>::new(public.clone());
            let sig = Signature::try_from(signature).map_err(|e| {
                NcryptError::VerifyFailed(format!("invalid RSA-PSS signature: {e}"))
            })?;
            key.verify(data, &sig).is_ok()
        }
        (RsaHash::Sha512, RsaSignPadding::Pss) => {
            let key = PssVerifyingKey::<Sha512>::new(public.clone());
            let sig = Signature::try_from(signature).map_err(|e| {
                NcryptError::VerifyFailed(format!("invalid RSA-PSS signature: {e}"))
            })?;
            key.verify(data, &sig).is_ok()
        }
        (RsaHash::Sha256, RsaSignPadding::Pkcs1v15) => {
            let digest = sha256(data);
            let key = VerifyingKey::<Sha256>::new_unprefixed(public.clone());
            let sig = rsa::pkcs1v15::Signature::try_from(signature)
                .map_err(|e| NcryptError::VerifyFailed(format!("invalid RSA signature: {e}")))?;
            key.verify(&digest, &sig).is_ok()
        }
        (RsaHash::Sha512, RsaSignPadding::Pkcs1v15) => {
            let digest = sha512(data);
            let key = VerifyingKey::<Sha512>::new_unprefixed(public.clone());
            let sig = rsa::pkcs1v15::Signature::try_from(signature)
                .map_err(|e| NcryptError::VerifyFailed(format!("invalid RSA signature: {e}")))?;
            key.verify(&digest, &sig).is_ok()
        }
    };
    Ok(ok)
}

pub fn rsa_max_plaintext_len(public: &RsaPublicKey, padding: RsaPadding) -> NcryptResult<usize> {
    let k = (public.n().bits() + 7) / 8;
    let overhead = match padding {
        RsaPadding::OaepSha256 => 2 * 32 + 2,
        RsaPadding::OaepSha512 => 2 * 64 + 2,
        RsaPadding::Pkcs1v15 => 11,
    };
    if k <= overhead {
        return Err(NcryptError::InvalidKey(
            "RSA key too small for padding".into(),
        ));
    }
    Ok(k - overhead)
}
