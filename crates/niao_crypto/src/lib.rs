//! Zero-dependency SHA-256/512, HMAC, and JWT (HS256/HS512).

pub mod ct;
pub mod hex;
pub mod hmac;
pub mod jwt;
pub mod sha1;
pub mod sha256;
pub mod sha512;

pub use ct::eq as constant_time_eq;
pub use hmac::{hmac, hmac_sha1, hmac_sha256, hmac_sha512, HmacAlgorithm};
pub use jwt::{sign_hs256, sign_hs512, verify, Algorithm as JwtAlgorithm, JwtError, Validation};
pub use sha1::{hash as sha1, Sha1};
pub use sha256::{hash as sha256, Sha256};
pub use sha512::{hash as sha512, Sha512};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex;

    #[test]
    fn nist_sha256_empty() {
        let got = hex::encode(&sha256(b""));
        assert_eq!(
            got,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn nist_sha256_abc() {
        let got = hex::encode(&sha256(b"abc"));
        assert_eq!(
            got,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn nist_sha512_abc() {
        let got = hex::encode(&sha512(b"abc"));
        assert_eq!(
            got,
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
        );
    }

    #[test]
    fn incremental_matches_one_shot() {
        let data = b"hello world";
        let mut h = Sha256::new();
        h.update(&data[..5]);
        h.update(&data[5..]);
        assert_eq!(h.finalize(), sha256(data));
    }
}
