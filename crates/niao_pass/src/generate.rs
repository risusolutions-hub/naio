//! Secure random password generation.

use crate::error::{PassError, PassResult};
use niao_rand::{fill_os_random, thread_rng, Rng};

pub const DEFAULT_ALPHABET: &str =
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%^&*()-_=+[]{}";

pub fn generate(length: usize, alphabet: Option<&str>) -> PassResult<String> {
    if length == 0 {
        return Err(PassError::InvalidParameter("length must be > 0".into()));
    }
    if length > 256 {
        return Err(PassError::InvalidParameter("length must be <= 256".into()));
    }
    let alphabet = alphabet.unwrap_or(DEFAULT_ALPHABET);
    if alphabet.is_empty() {
        return Err(PassError::InvalidParameter(
            "alphabet must not be empty".into(),
        ));
    }
    let chars: Vec<char> = alphabet.chars().collect();
    let mut out = String::with_capacity(length);
    let mut rng = thread_rng();
    for _ in 0..length {
        let idx = rng.gen_range_usize(0, chars.len());
        out.push(chars[idx]);
    }
    Ok(out)
}

pub fn generate_bytes(length: usize) -> PassResult<Vec<u8>> {
    if length == 0 {
        return Err(PassError::InvalidParameter("length must be > 0".into()));
    }
    if length > 256 {
        return Err(PassError::InvalidParameter("length must be <= 256".into()));
    }
    let mut buf = vec![0u8; length];
    fill_os_random(&mut buf);
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_len() {
        let p = generate(16, None).unwrap();
        assert_eq!(p.chars().count(), 16);
    }
}
