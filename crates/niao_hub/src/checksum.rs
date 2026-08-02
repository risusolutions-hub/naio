use crate::error::{HubError, HubResult};
use niao_crypto::{hex, sha256, sha512};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashAlgo {
    Sha256,
    Sha512,
}

impl HashAlgo {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "sha256" | "sha-256" => Some(Self::Sha256),
            "sha512" | "sha-512" => Some(Self::Sha512),
            _ => None,
        }
    }
}

/// Hash bytes with the selected algorithm, returning lowercase hex.
pub fn hash_bytes(data: &[u8], algo: HashAlgo) -> String {
    match algo {
        HashAlgo::Sha256 => hex::encode(&sha256(data)),
        HashAlgo::Sha512 => hex::encode(&sha512(data)),
    }
}

/// Stream a file through the hasher (8 KiB buffer).
pub fn hash_file(path: &Path, algo: HashAlgo) -> HubResult<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut buf = [0u8; 8192];
    match algo {
        HashAlgo::Sha256 => {
            let mut h = niao_crypto::Sha256::new();
            loop {
                let n = file.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                h.update(&buf[..n]);
            }
            Ok(hex::encode(&h.finalize()))
        }
        HashAlgo::Sha512 => {
            let mut h = niao_crypto::Sha512::new();
            loop {
                let n = file.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                h.update(&buf[..n]);
            }
            Ok(hex::encode(&h.finalize()))
        }
    }
}

fn normalize_hex(s: &str) -> String {
    s.trim()
        .trim_start_matches("sha256:")
        .trim_start_matches("sha512:")
        .to_ascii_lowercase()
}

/// Compare file hash to expected hex (optional `sha256:` prefix).
pub fn verify_file(path: &Path, expected: &str, algo: HashAlgo) -> HubResult<bool> {
    let actual = hash_file(path, algo)?;
    let exp = normalize_hex(expected);
    if actual == exp {
        Ok(true)
    } else {
        Err(HubError::Checksum {
            expected: exp,
            actual,
        })
    }
}

/// Compare in-memory bytes to expected hex digest.
pub fn verify_bytes(data: &[u8], expected: &str, algo: HashAlgo) -> HubResult<bool> {
    let actual = hash_bytes(data, algo);
    let exp = normalize_hex(expected);
    if actual == exp {
        Ok(true)
    } else {
        Err(HubError::Checksum {
            expected: exp,
            actual,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_abc() {
        let got = hash_bytes(b"abc", HashAlgo::Sha256);
        assert_eq!(
            got,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn verify_bytes_ok() {
        assert!(verify_bytes(
            b"abc",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            HashAlgo::Sha256
        )
        .unwrap());
    }
}
