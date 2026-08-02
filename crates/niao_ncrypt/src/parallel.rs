use crate::aead::{aead_decrypt, aead_encrypt, AeadCipher};
use crate::error::NcryptResult;
use niao_parallel::map;

pub fn parallel_aead_encrypt(
    blocks: &[Vec<u8>],
    key: &[u8],
    nonce: &[u8],
    cipher: AeadCipher,
    threads: usize,
) -> NcryptResult<Vec<Vec<u8>>> {
    let out: NcryptResult<Vec<Vec<u8>>> = map(blocks, threads, |pt| {
        aead_encrypt(cipher, key, nonce, pt, None)
    })
    .into_iter()
    .collect();
    out
}

pub fn parallel_aead_decrypt(
    blocks: &[Vec<u8>],
    key: &[u8],
    nonce: &[u8],
    cipher: AeadCipher,
    threads: usize,
) -> NcryptResult<Vec<Vec<u8>>> {
    let out: NcryptResult<Vec<Vec<u8>>> = map(blocks, threads, |ct| {
        aead_decrypt(cipher, key, nonce, ct, None)
    })
    .into_iter()
    .collect();
    out
}
