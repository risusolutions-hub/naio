use crate::error::{check_token_len, NcryptResult};
use niao_codec::{base64, Base64Config};
use niao_crypto::hex;
use niao_rand::fill_os_random;

/// Fill `buf` with OS CSPRNG bytes.
pub fn fill_random(buf: &mut [u8]) {
    fill_os_random(buf);
}

/// `secrets.token_bytes(n)`.
pub fn token_bytes(n: usize) -> NcryptResult<Vec<u8>> {
    check_token_len(n)?;
    let mut out = vec![0u8; n];
    fill_random(&mut out);
    Ok(out)
}

/// `secrets.token_hex(n)` — n bytes encoded as 2n hex chars.
pub fn token_hex(n: usize) -> NcryptResult<String> {
    Ok(hex::encode(&token_bytes(n)?))
}

/// `secrets.token_urlsafe(n)` — n random bytes as URL-safe base64 without padding.
pub fn token_urlsafe(n: usize) -> NcryptResult<String> {
    let bytes = token_bytes(n)?;
    Ok(base64::encode(&bytes, Base64Config::URL_SAFE_NO_PAD))
}
