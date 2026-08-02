//! URL-safe compact unique string IDs (~nanoid).

use crate::entropy::fill_random;
use niao_rand::{thread_rng, Rng};

pub const DEFAULT_ALPHABET: &str =
    "_-0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
pub const DEFAULT_SIZE: usize = 21;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NanoidError {
    EmptyAlphabet,
    AlphabetTooLarge,
    InvalidSize,
    InvalidAlphabetChar(char),
}

impl std::fmt::Display for NanoidError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyAlphabet => write!(f, "nanoid alphabet must not be empty"),
            Self::AlphabetTooLarge => write!(f, "nanoid alphabet must have at most 255 characters"),
            Self::InvalidSize => write!(f, "nanoid size must be > 0"),
            Self::InvalidAlphabetChar(c) => {
                write!(f, "duplicate or invalid alphabet character '{c}'")
            }
        }
    }
}

impl std::error::Error for NanoidError {}

/// Generate a nanoid with default alphabet and size 21.
pub fn nanoid() -> String {
    nanoid_with(DEFAULT_SIZE, DEFAULT_ALPHABET).expect("default nanoid params")
}

/// Generate with custom size (default alphabet).
pub fn nanoid_size(size: usize) -> Result<String, NanoidError> {
    nanoid_with(size, DEFAULT_ALPHABET)
}

/// Generate with custom alphabet and size.
pub fn nanoid_with(size: usize, alphabet: &str) -> Result<String, NanoidError> {
    if size == 0 {
        return Err(NanoidError::InvalidSize);
    }
    let table = Alphabet::parse(alphabet)?;
    Ok(generate(&table, size))
}

/// Batch-generate `count` IDs (amortizes alphabet setup).
pub fn nanoid_bulk(count: usize, size: usize, alphabet: &str) -> Result<Vec<String>, NanoidError> {
    if size == 0 {
        return Err(NanoidError::InvalidSize);
    }
    let table = Alphabet::parse(alphabet)?;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        out.push(generate(&table, size));
    }
    Ok(out)
}

struct Alphabet {
    chars: Vec<u8>,
    mask: u8,
    step: usize,
}

impl Alphabet {
    fn parse(s: &str) -> Result<Self, NanoidError> {
        if s.is_empty() {
            return Err(NanoidError::EmptyAlphabet);
        }
        if s.len() > 255 {
            return Err(NanoidError::AlphabetTooLarge);
        }
        let mut chars = Vec::with_capacity(s.len());
        for ch in s.chars() {
            if ch as u32 > 255 {
                return Err(NanoidError::InvalidAlphabetChar(ch));
            }
            let b = ch as u8;
            if chars.contains(&b) {
                return Err(NanoidError::InvalidAlphabetChar(ch));
            }
            chars.push(b);
        }
        let len = chars.len();
        let mask = (2u32 << (31 - (len as u32).leading_zeros())) - 1;
        let mask = mask as u8;
        let step = ((1.6 * (mask as f64) * (size_hint(len) as f64)).ceil() as usize).max(1);
        Ok(Self { chars, mask, step })
    }
}

#[inline]
fn size_hint(alphabet_len: usize) -> usize {
    DEFAULT_SIZE.max(alphabet_len)
}

fn generate(table: &Alphabet, size: usize) -> String {
    let mut id = Vec::with_capacity(size);
    let mut bytes = vec![0u8; table.step];
    while id.len() < size {
        fill_random(&mut bytes);
        for &b in &bytes {
            let idx = b & table.mask;
            if (idx as usize) < table.chars.len() {
                id.push(table.chars[idx as usize]);
                if id.len() == size {
                    break;
                }
            }
        }
    }
    // SAFETY: alphabet is ASCII subset
    unsafe { String::from_utf8_unchecked(id) }
}

/// Fast non-crypto nanoid using thread-local PRNG (for tests / deterministic benches).
pub fn nanoid_fast(size: usize) -> String {
    let table = Alphabet::parse(DEFAULT_ALPHABET).unwrap();
    let mut id = Vec::with_capacity(size);
    let mut rng = thread_rng();
    while id.len() < size {
        let idx = rng.gen_range_usize(0, table.chars.len());
        id.push(table.chars[idx]);
    }
    unsafe { String::from_utf8_unchecked(id) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn default_shape() {
        let id = nanoid();
        assert_eq!(id.len(), DEFAULT_SIZE);
        assert!(id.chars().all(|c| DEFAULT_ALPHABET.contains(c)));
    }

    #[test]
    fn unique_batch() {
        let ids = nanoid_bulk(256, 16, DEFAULT_ALPHABET).unwrap();
        let set: HashSet<_> = ids.iter().collect();
        assert_eq!(set.len(), 256);
    }

    #[test]
    fn rejects_dup_alphabet() {
        assert!(nanoid_with(8, "aabc").is_err());
    }
}
