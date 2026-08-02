use crate::error::{check_len, NcryptError, NcryptResult, NONCE_LEN, TAG_LEN};
use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes128Gcm, Aes256Gcm, Key, Nonce,
};
use chacha20poly1305::ChaCha20Poly1305;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AeadCipher {
    Aes128Gcm,
    Aes256Gcm,
    ChaCha20Poly1305,
}

impl AeadCipher {
    pub fn parse(name: &str) -> NcryptResult<Self> {
        match name.to_ascii_lowercase().as_str() {
            "aes-128-gcm" | "aes128gcm" | "aes128" => Ok(Self::Aes128Gcm),
            "aes-256-gcm" | "aes256gcm" | "aes256" | "aes-gcm" | "aesgcm" => Ok(Self::Aes256Gcm),
            "chacha20-poly1305" | "chacha20poly1305" | "chacha" | "chacha20" => {
                Ok(Self::ChaCha20Poly1305)
            }
            other => Err(NcryptError::Unsupported(format!(
                "unknown AEAD cipher '{other}'"
            ))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Aes128Gcm => "aes-128-gcm",
            Self::Aes256Gcm => "aes-256-gcm",
            Self::ChaCha20Poly1305 => "chacha20-poly1305",
        }
    }

    pub fn key_len(self) -> usize {
        match self {
            Self::Aes128Gcm => 16,
            Self::Aes256Gcm | Self::ChaCha20Poly1305 => 32,
        }
    }
}

fn validate_key(cipher: AeadCipher, key: &[u8]) -> NcryptResult<()> {
    let expected = cipher.key_len();
    if key.len() != expected {
        return Err(NcryptError::InvalidKey(format!(
            "{} requires a {expected}-byte key",
            cipher.as_str()
        )));
    }
    Ok(())
}

fn validate_nonce(nonce: &[u8]) -> NcryptResult<()> {
    if nonce.len() != NONCE_LEN {
        return Err(NcryptError::InvalidArgument(format!(
            "nonce must be {NONCE_LEN} bytes"
        )));
    }
    Ok(())
}

/// Encrypt plaintext; returns `ciphertext || tag`.
pub fn aead_encrypt(
    cipher: AeadCipher,
    key: &[u8],
    nonce: &[u8],
    plaintext: &[u8],
    aad: Option<&[u8]>,
) -> NcryptResult<Vec<u8>> {
    check_len(plaintext.len())?;
    validate_key(cipher, key)?;
    validate_nonce(nonce)?;

    let pay = Payload {
        msg: plaintext,
        aad: aad.unwrap_or(&[]),
    };

    match cipher {
        AeadCipher::Aes128Gcm => {
            let c = Aes128Gcm::new(Key::<Aes128Gcm>::from_slice(key));
            c.encrypt(Nonce::from_slice(nonce), pay)
                .map_err(|_| NcryptError::EncryptFailed("AES-128-GCM encrypt failed".into()))
        }
        AeadCipher::Aes256Gcm => {
            let c = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
            c.encrypt(Nonce::from_slice(nonce), pay)
                .map_err(|_| NcryptError::EncryptFailed("AES-256-GCM encrypt failed".into()))
        }
        AeadCipher::ChaCha20Poly1305 => {
            let c = ChaCha20Poly1305::new(key.into());
            c.encrypt(Nonce::from_slice(nonce), pay)
                .map_err(|_| NcryptError::EncryptFailed("ChaCha20-Poly1305 encrypt failed".into()))
        }
    }
}

/// Decrypt `ciphertext || tag`.
pub fn aead_decrypt(
    cipher: AeadCipher,
    key: &[u8],
    nonce: &[u8],
    ciphertext_and_tag: &[u8],
    aad: Option<&[u8]>,
) -> NcryptResult<Vec<u8>> {
    if ciphertext_and_tag.len() < TAG_LEN {
        return Err(NcryptError::DecryptFailed(
            "ciphertext shorter than authentication tag".into(),
        ));
    }
    check_len(ciphertext_and_tag.len())?;
    validate_key(cipher, key)?;
    validate_nonce(nonce)?;

    let pay = Payload {
        msg: ciphertext_and_tag,
        aad: aad.unwrap_or(&[]),
    };

    match cipher {
        AeadCipher::Aes128Gcm => {
            let c = Aes128Gcm::new(Key::<Aes128Gcm>::from_slice(key));
            c.decrypt(Nonce::from_slice(nonce), pay)
                .map_err(|_| NcryptError::DecryptFailed("AES-128-GCM authentication failed".into()))
        }
        AeadCipher::Aes256Gcm => {
            let c = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
            c.decrypt(Nonce::from_slice(nonce), pay)
                .map_err(|_| NcryptError::DecryptFailed("AES-256-GCM authentication failed".into()))
        }
        AeadCipher::ChaCha20Poly1305 => {
            let c = ChaCha20Poly1305::new(key.into());
            c.decrypt(Nonce::from_slice(nonce), pay).map_err(|_| {
                NcryptError::DecryptFailed("ChaCha20-Poly1305 authentication failed".into())
            })
        }
    }
}

/// Seal: prepend random nonce → `nonce || ciphertext || tag`.
pub fn aead_seal(
    cipher: AeadCipher,
    key: &[u8],
    plaintext: &[u8],
    aad: Option<&[u8]>,
) -> NcryptResult<Vec<u8>> {
    let mut nonce = [0u8; NONCE_LEN];
    crate::rng::fill_random(&mut nonce);
    let ct = aead_encrypt(cipher, key, &nonce, plaintext, aad)?;
    let mut out = Vec::with_capacity(NONCE_LEN + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Open sealed blob from [`aead_seal`].
pub fn aead_open(
    cipher: AeadCipher,
    key: &[u8],
    sealed: &[u8],
    aad: Option<&[u8]>,
) -> NcryptResult<Vec<u8>> {
    if sealed.len() < NONCE_LEN + TAG_LEN {
        return Err(NcryptError::DecryptFailed("sealed blob too short".into()));
    }
    let (nonce, ct) = sealed.split_at(NONCE_LEN);
    aead_decrypt(cipher, key, nonce, ct, aad)
}

/// Split ciphertext+tag into body and tag.
pub fn split_tag(ciphertext_and_tag: &[u8]) -> NcryptResult<(&[u8], &[u8])> {
    if ciphertext_and_tag.len() < TAG_LEN {
        return Err(NcryptError::InvalidArgument(
            "buffer shorter than tag length".into(),
        ));
    }
    let split = ciphertext_and_tag.len() - TAG_LEN;
    Ok((&ciphertext_and_tag[..split], &ciphertext_and_tag[split..]))
}

/// Recombine ciphertext and tag.
pub fn join_tag(ciphertext: &[u8], tag: &[u8]) -> NcryptResult<Vec<u8>> {
    if tag.len() != TAG_LEN {
        return Err(NcryptError::InvalidArgument(format!(
            "tag must be {TAG_LEN} bytes"
        )));
    }
    let mut out = Vec::with_capacity(ciphertext.len() + TAG_LEN);
    out.extend_from_slice(ciphertext);
    out.extend_from_slice(tag);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aes256_roundtrip() {
        let key = [7u8; 32];
        let nonce = [1u8; 12];
        let pt = b"hello ncrypt";
        let ct = aead_encrypt(AeadCipher::Aes256Gcm, &key, &nonce, pt, None).unwrap();
        let back = aead_decrypt(AeadCipher::Aes256Gcm, &key, &nonce, &ct, None).unwrap();
        assert_eq!(back, pt);
    }

    #[test]
    fn seal_open_chacha() {
        let key = [9u8; 32];
        let pt = b"sealed payload";
        let sealed = aead_seal(AeadCipher::ChaCha20Poly1305, &key, pt, None).unwrap();
        let back = aead_open(AeadCipher::ChaCha20Poly1305, &key, &sealed, None).unwrap();
        assert_eq!(back, pt);
    }
}
