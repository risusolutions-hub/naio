//! RFC 4648 base64 encode/decode with standard and URL-safe alphabets.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alphabet {
    Standard,
    UrlSafe,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Base64Config {
    pub alphabet: Alphabet,
    pub padding: bool,
}

impl Default for Base64Config {
    fn default() -> Self {
        Self {
            alphabet: Alphabet::Standard,
            padding: true,
        }
    }
}

impl Base64Config {
    pub const STANDARD: Self = Self {
        alphabet: Alphabet::Standard,
        padding: true,
    };

    pub const STANDARD_NO_PAD: Self = Self {
        alphabet: Alphabet::Standard,
        padding: false,
    };

    pub const URL_SAFE: Self = Self {
        alphabet: Alphabet::UrlSafe,
        padding: true,
    };

    pub const URL_SAFE_NO_PAD: Self = Self {
        alphabet: Alphabet::UrlSafe,
        padding: false,
    };
}

#[derive(Debug, PartialEq, Eq)]
pub enum EncodeError {
    EmptyInput,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DecodeError {
    InvalidLength,
    InvalidByte(u8),
    InvalidPadding,
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput => write!(f, "empty input"),
        }
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength => write!(f, "invalid base64 length"),
            Self::InvalidByte(b) => write!(f, "invalid base64 byte: {b}"),
            Self::InvalidPadding => write!(f, "invalid base64 padding"),
        }
    }
}

impl std::error::Error for EncodeError {}
impl std::error::Error for DecodeError {}

#[inline]
fn encode_table(alphabet: Alphabet) -> &'static [u8; 64] {
    match alphabet {
        Alphabet::Standard => {
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
        }
        Alphabet::UrlSafe => {
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_"
        }
    }
}

#[inline]
fn decode_lut(alphabet: Alphabet) -> &'static [i8; 256] {
    match alphabet {
        Alphabet::Standard => &DECODE_STANDARD,
        Alphabet::UrlSafe => &DECODE_URL_SAFE,
    }
}

const DECODE_STANDARD: [i8; 256] = make_decode_lut(b"+/");
const DECODE_URL_SAFE: [i8; 256] = make_decode_lut(b"-_");

const fn make_decode_lut(last_two: &[u8; 2]) -> [i8; 256] {
    let mut lut = [-1i8; 256];
    let mut i = 0u8;
    while i < 26 {
        lut[(b'A' + i) as usize] = i as i8;
        lut[(b'a' + i) as usize] = (26 + i) as i8;
        i += 1;
    }
    let mut i = 0u8;
    while i < 10 {
        lut[(b'0' + i) as usize] = (52 + i) as i8;
        i += 1;
    }
    lut[last_two[0] as usize] = 62;
    lut[last_two[1] as usize] = 63;
    lut
}

/// Encode bytes to base64 using the given config.
pub fn encode(data: &[u8], config: Base64Config) -> String {
    if data.is_empty() {
        return String::new();
    }
    let table = encode_table(config.alphabet);
    let full_groups = data.len() / 3;
    let rem = data.len() % 3;
    let pad = if config.padding {
        match rem {
            0 => 0,
            1 => 2,
            _ => 1,
        }
    } else {
        0
    };
    let out_len = full_groups * 4 + if rem == 0 { 0 } else { 4 - pad };
    let mut out = Vec::with_capacity(out_len);
    let mut i = 0;
    while i + 3 <= data.len() {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8) | (data[i + 2] as u32);
        out.push(table[((n >> 18) & 63) as usize]);
        out.push(table[((n >> 12) & 63) as usize]);
        out.push(table[((n >> 6) & 63) as usize]);
        out.push(table[(n & 63) as usize]);
        i += 3;
    }
    if rem == 1 {
        let n = (data[i] as u32) << 16;
        out.push(table[((n >> 18) & 63) as usize]);
        out.push(table[((n >> 12) & 63) as usize]);
        if config.padding {
            out.push(b'=');
            out.push(b'=');
        }
    } else if rem == 2 {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8);
        out.push(table[((n >> 18) & 63) as usize]);
        out.push(table[((n >> 12) & 63) as usize]);
        out.push(table[((n >> 6) & 63) as usize]);
        if config.padding {
            out.push(b'=');
        }
    }
    // SAFETY: table chars are ASCII.
    unsafe { String::from_utf8_unchecked(out) }
}

/// Decode a base64 string using the given config.
pub fn decode(input: &str, config: Base64Config) -> Result<Vec<u8>, DecodeError> {
    let lut = decode_lut(config.alphabet);
    let mut bytes = input.as_bytes();
    if bytes.is_empty() {
        return Ok(Vec::new());
    }

    let mut pad = 0usize;
    while bytes.last() == Some(&b'=') {
        pad += 1;
        bytes = &bytes[..bytes.len() - 1];
    }
    if pad > 2 {
        return Err(DecodeError::InvalidPadding);
    }
    if bytes.is_empty() {
        return if pad > 0 {
            Ok(Vec::new())
        } else {
            Ok(Vec::new())
        };
    }
    if bytes.len() % 4 == 1 {
        return Err(DecodeError::InvalidLength);
    }

    let mut out = Vec::with_capacity(bytes.len() * 3 / 4 + 1);
    let mut i = 0usize;
    while i + 4 <= bytes.len() {
        let a = decode_byte(bytes[i], lut)?;
        let b = decode_byte(bytes[i + 1], lut)?;
        let c = decode_byte(bytes[i + 2], lut)?;
        let d = decode_byte(bytes[i + 3], lut)?;
        out.push(((a as u8) << 2) | ((b as u8) >> 4));
        out.push(((b as u8) << 4) | ((c as u8) >> 2));
        out.push(((c as u8) << 6) | (d as u8));
        i += 4;
    }

    let rem = bytes.len() - i;
    if rem == 2 {
        let a = decode_byte(bytes[i], lut)?;
        let b = decode_byte(bytes[i + 1], lut)?;
        out.push(((a as u8) << 2) | ((b as u8) >> 4));
        if pad != 0 && pad != 2 {
            return Err(DecodeError::InvalidPadding);
        }
    } else if rem == 3 {
        let a = decode_byte(bytes[i], lut)?;
        let b = decode_byte(bytes[i + 1], lut)?;
        let c = decode_byte(bytes[i + 2], lut)?;
        out.push(((a as u8) << 2) | ((b as u8) >> 4));
        out.push(((b as u8) << 4) | ((c as u8) >> 2));
        if pad != 0 && pad != 1 {
            return Err(DecodeError::InvalidPadding);
        }
    } else if rem != 0 {
        return Err(DecodeError::InvalidLength);
    } else if pad > 0 {
        return Err(DecodeError::InvalidPadding);
    }

    Ok(out)
}

#[inline]
fn decode_byte(b: u8, lut: &[i8; 256]) -> Result<i8, DecodeError> {
    let v = lut[b as usize];
    if v < 0 {
        Err(DecodeError::InvalidByte(b))
    } else {
        Ok(v)
    }
}

/// Standard padded encode (most common).
#[inline]
pub fn encode_standard(data: &[u8]) -> String {
    encode(data, Base64Config::STANDARD)
}

/// Standard padded decode.
#[inline]
pub fn decode_standard(input: &str) -> Result<Vec<u8>, DecodeError> {
    decode(input, Base64Config::STANDARD)
}

/// URL-safe encode without padding.
#[inline]
pub fn encode_url_safe_no_pad(data: &[u8]) -> String {
    encode(data, Base64Config::URL_SAFE_NO_PAD)
}

/// URL-safe decode (padding optional).
#[inline]
pub fn decode_url_safe(input: &str) -> Result<Vec<u8>, DecodeError> {
    decode(input, Base64Config::URL_SAFE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc4648_vectors_standard() {
        let cases = [
            (b"" as &[u8], ""),
            (b"f", "Zg=="),
            (b"fo", "Zm8="),
            (b"foo", "Zm9v"),
            (b"foob", "Zm9vYg=="),
            (b"fooba", "Zm9vYmE="),
            (b"foobar", "Zm9vYmFy"),
        ];
        for (raw, enc) in cases {
            assert_eq!(encode_standard(raw), enc);
            assert_eq!(decode_standard(enc).unwrap(), raw);
        }
    }

    #[test]
    fn url_safe_roundtrip() {
        let data = b"subjects?d=hello world";
        let enc = encode_url_safe_no_pad(data);
        assert!(!enc.contains('='));
        assert_eq!(decode_url_safe(&enc).unwrap(), data);
    }

    #[test]
    fn no_pad_encode_decode() {
        let data = b"hello world!!";
        let enc = encode(data, Base64Config::STANDARD_NO_PAD);
        assert!(!enc.contains('='));
        assert_eq!(
            decode(&enc, Base64Config::STANDARD_NO_PAD).unwrap(),
            data.as_slice()
        );
    }

    #[test]
    fn invalid_char() {
        assert!(matches!(
            decode_standard("Z?=="),
            Err(DecodeError::InvalidByte(b'?'))
        ));
    }
}
