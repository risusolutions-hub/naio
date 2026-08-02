//! RFC 4648 base32 encode/decode (unpadded, OTP alphabet A-Z2-7).

use crate::error::OtpError;

const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

#[inline]
fn decode_char(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'2'..=b'7' => Some(c - b'2' + 26),
        _ => None,
    }
}

/// Decode base32 (case-insensitive, ignores padding `=` and whitespace).
pub fn decode(input: &str) -> Result<Vec<u8>, OtpError> {
    let mut out = Vec::with_capacity(input.len() * 5 / 8);
    let mut buffer: u64 = 0;
    let mut bits = 0;

    for &b in input.as_bytes() {
        if b == b'=' || b.is_ascii_whitespace() {
            continue;
        }
        let c = if (b'a'..=b'z').contains(&b) {
            b - 32
        } else {
            b
        };
        let v = decode_char(c).ok_or_else(|| {
            OtpError::InvalidBase32(format!("invalid character {:?}", char::from(c)))
        })?;
        buffer = (buffer << 5) | u64::from(v);
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }

    if out.is_empty() && !input.is_empty() {
        let trimmed: String = input
            .chars()
            .filter(|c| !c.is_ascii_whitespace() && *c != '=')
            .collect();
        if !trimmed.is_empty() {
            return Err(OtpError::InvalidBase32("no decodable bytes".into()));
        }
    }
    Ok(out)
}

/// Encode bytes to unpadded uppercase base32.
pub fn encode(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity((bytes.len() * 8 + 4) / 5);
    let mut buffer: u64 = 0;
    let mut bits = 0;
    for &b in bytes {
        buffer = (buffer << 8) | u64::from(b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let idx = ((buffer >> bits) & 0x1f) as usize;
            out.push(ALPHABET[idx] as char);
            buffer &= (1 << bits) - 1;
        }
    }
    if bits > 0 {
        let idx = ((buffer << (5 - bits)) & 0x1f) as usize;
        out.push(ALPHABET[idx] as char);
    }
    out
}

/// Generate a random base32 secret of `length` characters (default 32).
pub fn random_base32(length: usize) -> Result<String, OtpError> {
    if length == 0 {
        return Err(OtpError::EmptyInput);
    }
    let byte_len = (length * 5 + 7) / 8;
    let mut bytes = vec![0u8; byte_len];
    niao_rand::fill_os_random(&mut bytes);
    let mut out = encode(&bytes);
    out.truncate(length);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_secret_roundtrip() {
        let decoded = decode("JBSWY3DPEHPK3PXP").unwrap();
        assert_eq!(decoded.len(), 10);
        assert_eq!(&decoded[..6], b"Hello!");
    }

    #[test]
    fn roundtrip() {
        let data = b"test secret bytes 123";
        let enc = encode(data);
        assert_eq!(decode(&enc).unwrap(), data);
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(
            decode("jbswy3dpehpk3pxp").unwrap(),
            decode("JBSWY3DPEHPK3PXP").unwrap()
        );
    }
}
