use crate::error::{NcryptError, NcryptResult};
use rand::rngs::OsRng;
use x25519_dalek::{PublicKey, StaticSecret};

pub const SECRET_LEN: usize = 32;
pub const PUBLIC_LEN: usize = 32;

pub struct X25519KeyPair {
    pub secret: StaticSecret,
    pub public: PublicKey,
}

pub fn x25519_generate() -> NcryptResult<X25519KeyPair> {
    let secret = StaticSecret::random_from_rng(OsRng);
    let public = PublicKey::from(&secret);
    Ok(X25519KeyPair { secret, public })
}

pub fn x25519_from_private(private: &[u8]) -> NcryptResult<X25519KeyPair> {
    if private.len() != SECRET_LEN {
        return Err(NcryptError::InvalidKey(format!(
            "X25519 private key must be {SECRET_LEN} bytes"
        )));
    }
    let mut arr = [0u8; SECRET_LEN];
    arr.copy_from_slice(private);
    let secret = StaticSecret::from(arr);
    let public = PublicKey::from(&secret);
    Ok(X25519KeyPair { secret, public })
}

pub fn x25519_public_from_bytes(bytes: &[u8]) -> NcryptResult<PublicKey> {
    if bytes.len() != PUBLIC_LEN {
        return Err(NcryptError::InvalidKey(format!(
            "X25519 public key must be {PUBLIC_LEN} bytes"
        )));
    }
    let mut arr = [0u8; PUBLIC_LEN];
    arr.copy_from_slice(bytes);
    Ok(PublicKey::from(arr))
}

pub fn x25519_shared(secret: &StaticSecret, peer_public: &PublicKey) -> Vec<u8> {
    secret.diffie_hellman(peer_public).to_bytes().to_vec()
}

pub fn x25519_public_bytes(key: &PublicKey) -> Vec<u8> {
    key.to_bytes().to_vec()
}

pub fn x25519_private_bytes(pair: &X25519KeyPair) -> Vec<u8> {
    pair.secret.to_bytes().to_vec()
}
