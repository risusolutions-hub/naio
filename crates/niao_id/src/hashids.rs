//! Hashids-compatible integer obfuscation (~hashids-py) via the `hash-ids` crate.

use hash_ids::{Error as InnerError, HashIds};

pub const DEFAULT_ALPHABET: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HashidsError {
    EmptyAlphabet,
    AlphabetTooShort,
    DuplicateChar(char),
    NonAsciiChar(char),
    EmptyInput,
    InvalidHash,
    Internal(String),
}

impl std::fmt::Display for HashidsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyAlphabet => write!(f, "hashids alphabet must not be empty"),
            Self::AlphabetTooShort => {
                write!(
                    f,
                    "hashids alphabet must have at least 16 unique characters"
                )
            }
            Self::DuplicateChar(c) => write!(f, "duplicate alphabet character '{c}'"),
            Self::NonAsciiChar(c) => write!(f, "non-ascii alphabet character '{c}'"),
            Self::EmptyInput => write!(f, "encode requires at least one number"),
            Self::InvalidHash => write!(f, "invalid hash string for this alphabet/salt"),
            Self::Internal(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for HashidsError {}

#[derive(Clone)]
pub struct Hashids {
    inner: HashIds,
}

impl Hashids {
    pub fn new(salt: &str, min_length: usize, alphabet: &str) -> Result<Self, HashidsError> {
        validate_alphabet(alphabet)?;
        let inner = HashIds::builder()
            .with_salt(salt)
            .with_min_length(min_length)
            .with_alphabet(alphabet)
            .finish()
            .map_err(map_inner)?;
        Ok(Self { inner })
    }

    pub fn encode(&self, numbers: &[u64]) -> Result<String, HashidsError> {
        if numbers.is_empty() {
            return Err(HashidsError::EmptyInput);
        }
        Ok(self.inner.encode(numbers))
    }

    pub fn decode(&self, hash: &str) -> Result<Vec<u64>, HashidsError> {
        self.inner
            .decode(hash)
            .map_err(|_| HashidsError::InvalidHash)
    }

    pub fn encode_hex(&self, hex: &str) -> Result<String, HashidsError> {
        let clean: String = hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        if clean.is_empty() || clean.len() % 2 != 0 {
            return Err(HashidsError::Internal(
                "hex input must be non-empty and even length".into(),
            ));
        }
        let mut nums = Vec::with_capacity(clean.len() / 2);
        let bytes = clean.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let hi =
                hex_nibble(bytes[i]).ok_or_else(|| HashidsError::Internal("invalid hex".into()))?;
            let lo = hex_nibble(bytes[i + 1])
                .ok_or_else(|| HashidsError::Internal("invalid hex".into()))?;
            nums.push(((hi << 4) | lo) as u64);
            i += 2;
        }
        Ok(self.inner.encode(&nums))
    }

    pub fn decode_hex(&self, hash: &str) -> Result<String, HashidsError> {
        let nums = self.decode(hash)?;
        let mut out = String::with_capacity(nums.len() * 2);
        for n in nums {
            out.push_str(&format!("{n:02x}"));
        }
        Ok(out)
    }
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn validate_alphabet(alphabet: &str) -> Result<(), HashidsError> {
    if alphabet.is_empty() {
        return Err(HashidsError::EmptyAlphabet);
    }
    let mut seen = std::collections::HashSet::new();
    for ch in alphabet.chars() {
        if ch as u32 > 127 {
            return Err(HashidsError::NonAsciiChar(ch));
        }
        if !seen.insert(ch) {
            return Err(HashidsError::DuplicateChar(ch));
        }
    }
    if seen.len() < 16 {
        return Err(HashidsError::AlphabetTooShort);
    }
    Ok(())
}

fn map_inner(e: InnerError) -> HashidsError {
    HashidsError::Internal(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_roundtrip() {
        let h = Hashids::new("neko", 0, DEFAULT_ALPHABET).unwrap();
        let nums = [1u64, 2, 3];
        let enc = h.encode(&nums).unwrap();
        let dec = h.decode(&enc).unwrap();
        assert_eq!(dec, nums);
    }

    #[test]
    fn min_length_padding() {
        let h = Hashids::new("", 8, DEFAULT_ALPHABET).unwrap();
        let enc = h.encode(&[42]).unwrap();
        assert!(enc.len() >= 8);
    }

    #[test]
    fn hex_roundtrip() {
        let h = Hashids::new("salt", 0, DEFAULT_ALPHABET).unwrap();
        let hex = "507f1f77bcf86cd799439011";
        let enc = h.encode_hex(hex).unwrap();
        assert_eq!(h.decode_hex(&enc).unwrap(), hex);
    }
}
