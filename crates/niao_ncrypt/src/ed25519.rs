use crate::error::{check_len, NcryptError, NcryptResult};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;

pub const SECRET_LEN: usize = 32;
pub const PUBLIC_LEN: usize = 32;
pub const SIGNATURE_LEN: usize = 64;

pub struct Ed25519KeyPair {
    pub signing: SigningKey,
    pub verifying: VerifyingKey,
}

pub fn ed25519_generate() -> NcryptResult<Ed25519KeyPair> {
    let signing = SigningKey::generate(&mut OsRng);
    let verifying = signing.verifying_key();
    Ok(Ed25519KeyPair { signing, verifying })
}

pub fn ed25519_from_seed(seed: &[u8]) -> NcryptResult<Ed25519KeyPair> {
    if seed.len() != SECRET_LEN {
        return Err(NcryptError::InvalidKey(format!(
            "Ed25519 seed must be {SECRET_LEN} bytes"
        )));
    }
    let mut arr = [0u8; SECRET_LEN];
    arr.copy_from_slice(seed);
    let signing = SigningKey::from_bytes(&arr);
    let verifying = signing.verifying_key();
    Ok(Ed25519KeyPair { signing, verifying })
}

pub fn ed25519_from_private(private: &[u8]) -> NcryptResult<Ed25519KeyPair> {
    ed25519_from_seed(private)
}

pub fn ed25519_public_from_bytes(bytes: &[u8]) -> NcryptResult<VerifyingKey> {
    if bytes.len() != PUBLIC_LEN {
        return Err(NcryptError::InvalidKey(format!(
            "Ed25519 public key must be {PUBLIC_LEN} bytes"
        )));
    }
    let mut arr = [0u8; PUBLIC_LEN];
    arr.copy_from_slice(bytes);
    VerifyingKey::from_bytes(&arr)
        .map_err(|e| NcryptError::InvalidKey(format!("invalid Ed25519 public key: {e}")))
}

pub fn ed25519_sign(pair: &Ed25519KeyPair, message: &[u8]) -> NcryptResult<Vec<u8>> {
    check_len(message.len())?;
    Ok(pair.signing.sign(message).to_bytes().to_vec())
}

pub fn ed25519_verify(
    public: &VerifyingKey,
    message: &[u8],
    signature: &[u8],
) -> NcryptResult<bool> {
    check_len(message.len())?;
    if signature.len() != SIGNATURE_LEN {
        return Err(NcryptError::VerifyFailed(format!(
            "Ed25519 signature must be {SIGNATURE_LEN} bytes"
        )));
    }
    let mut arr = [0u8; SIGNATURE_LEN];
    arr.copy_from_slice(signature);
    let sig = Signature::from_bytes(&arr);
    Ok(public.verify(message, &sig).is_ok())
}

pub fn ed25519_public_bytes(key: &VerifyingKey) -> Vec<u8> {
    key.to_bytes().to_vec()
}

pub fn ed25519_private_bytes(pair: &Ed25519KeyPair) -> Vec<u8> {
    pair.signing.to_bytes().to_vec()
}
