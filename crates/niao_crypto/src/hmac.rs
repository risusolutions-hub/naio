//! HMAC (RFC 2104 / 4231).

use crate::sha256::Sha256;
use crate::sha512::Sha512;

pub enum HmacAlgorithm {
    Sha256,
    Sha512,
}

pub fn hmac(algo: HmacAlgorithm, key: &[u8], data: &[u8]) -> Vec<u8> {
    match algo {
        HmacAlgorithm::Sha256 => hmac_sha256(key, data).to_vec(),
        HmacAlgorithm::Sha512 => hmac_sha512(key, data).to_vec(),
    }
}

pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let k = normalize_key::<BLOCK>(key, |k| {
        let mut h = Sha256::new();
        h.update(k);
        h.finalize().to_vec()
    });
    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let mut inner = Sha256::new();
    inner.update(&ipad);
    inner.update(data);
    let inner_digest = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(&opad);
    outer.update(&inner_digest);
    outer.finalize()
}

pub fn hmac_sha512(key: &[u8], data: &[u8]) -> [u8; 64] {
    const BLOCK: usize = 128;
    let k = normalize_key::<BLOCK>(key, |k| {
        let mut h = Sha512::new();
        h.update(k);
        h.finalize().to_vec()
    });
    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let mut inner = Sha512::new();
    inner.update(&ipad);
    inner.update(data);
    let inner_digest = inner.finalize();
    let mut outer = Sha512::new();
    outer.update(&opad);
    outer.update(&inner_digest);
    outer.finalize()
}

fn normalize_key<const BLOCK: usize>(key: &[u8], hash_key: impl FnOnce(&[u8]) -> Vec<u8>) -> [u8; BLOCK] {
    let mut out = [0u8; BLOCK];
    if key.len() > BLOCK {
        let digest = hash_key(key);
        out[..digest.len()].copy_from_slice(&digest);
    } else {
        out[..key.len()].copy_from_slice(key);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex;

    #[test]
    fn rfc4231_sha256_case1() {
        let key = [0x0bu8; 20];
        let data = b"Hi There";
        let expected = hex::decode(
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7",
        )
        .unwrap();
        assert_eq!(&hmac_sha256(&key, data)[..], &expected[..]);
    }

    #[test]
    fn rfc4231_sha256_case2() {
        let key = b"Jefe";
        let data = b"what do ya want for nothing?";
        let expected = hex::decode(
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843",
        )
        .unwrap();
        assert_eq!(&hmac_sha256(key, data)[..], &expected[..]);
    }
}
