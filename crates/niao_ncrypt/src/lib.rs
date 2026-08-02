//! Modern crypto for Niao: AES-GCM, ChaCha20-Poly1305, RSA, Ed25519/X25519,
//! HKDF/PBKDF2, X.509 parse, CSPRNG, constant-time compare.
//!
//! ~cryptography, pynacl, secrets (extends crypto's SHA/HMAC).

mod aead;
mod compare;
mod ed25519;
mod error;
mod kdf;
mod parallel;
mod rng;
mod rsa;
mod x25519;
mod x509;

pub use aead::{aead_decrypt, aead_encrypt, aead_open, aead_seal, join_tag, split_tag, AeadCipher};
pub use compare::compare_digest;
pub use ed25519::{
    ed25519_from_private, ed25519_from_seed, ed25519_generate, ed25519_private_bytes,
    ed25519_public_bytes, ed25519_public_from_bytes, ed25519_sign, ed25519_verify, Ed25519KeyPair,
    PUBLIC_LEN as ED25519_PUBLIC_LEN, SECRET_LEN as ED25519_SECRET_LEN,
    SIGNATURE_LEN as ED25519_SIGNATURE_LEN,
};
pub use error::{
    check_len, check_token_len, NcryptError, NcryptResult, MAX_BYTES, MAX_TOKEN_BYTES, NONCE_LEN,
    TAG_LEN,
};
pub use kdf::{hkdf, hkdf_expand, hkdf_extract, pbkdf2_derive, HashAlg};
pub use parallel::{parallel_aead_decrypt, parallel_aead_encrypt};
pub use rng::{fill_random, token_bytes, token_hex, token_urlsafe};
pub use rsa::{
    rsa_decrypt, rsa_encrypt, rsa_generate, rsa_max_plaintext_len, rsa_private_from_pem,
    rsa_private_to_pem, rsa_public_from_pem, rsa_public_to_pem, rsa_sign, rsa_verify, RsaHash,
    RsaKeyPair, RsaPadding, RsaSignPadding,
};
pub use x25519::{
    x25519_from_private, x25519_generate, x25519_private_bytes, x25519_public_bytes,
    x25519_public_from_bytes, x25519_shared, X25519KeyPair, PUBLIC_LEN as X25519_PUBLIC_LEN,
    SECRET_LEN as X25519_SECRET_LEN,
};
pub use x509::{x509_fingerprint_sha256, x509_parse, x509_pem_to_der, ParsedCert};

#[cfg(test)]
mod integration {
    use super::*;

    #[test]
    fn full_smoke() {
        let key = token_bytes(32).unwrap();
        let sealed = aead_seal(AeadCipher::Aes256Gcm, &key, b"test", None).unwrap();
        let open = aead_open(AeadCipher::Aes256Gcm, &key, &sealed, None).unwrap();
        assert_eq!(open, b"test");

        let ed = ed25519_generate().unwrap();
        let sig = ed25519_sign(&ed, b"msg").unwrap();
        assert!(ed25519_verify(&ed.verifying, b"msg", &sig).unwrap());

        let x = x25519_generate().unwrap();
        let peer = x25519_generate().unwrap();
        let s1 = x25519_shared(&x.secret, &peer.public);
        let s2 = x25519_shared(&peer.secret, &x.public);
        assert_eq!(s1, s2);

        let dk = hkdf(b"ikm", 32, Some(b"salt"), Some(b"info"), HashAlg::Sha256).unwrap();
        assert_eq!(dk.len(), 32);

        assert!(compare_digest(&dk, &dk));
    }
}
