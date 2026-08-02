//! RFC 2047 encoded-word encode/decode for unstructured headers.

use niao_codec::base64;

/// Encode a header value with RFC 2047 when non-ASCII is present.
pub fn encode_header(text: &str) -> String {
    if text.is_ascii() && !text.contains('\r') && !text.contains('\n') {
        return text.to_string();
    }
    let mut out = String::new();
    for chunk in text.as_bytes().chunks(45) {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str("=?UTF-8?B?");
        out.push_str(&base64::encode_standard(chunk));
        out.push_str("?=");
    }
    out
}

/// Decode RFC 2047 encoded-words in an unstructured header value.
pub fn decode_header(text: &str) -> Result<String, crate::error::MailError> {
    let bytes = text.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'=' && i + 1 < bytes.len() && bytes[i + 1] == b'?' {
            if let Some((decoded, next)) = try_decode_word(bytes, i) {
                out.push_str(&decoded);
                i = next;
                // Collapse whitespace between adjacent encoded-words (RFC 2047).
                let mut j = i;
                while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                    j += 1;
                }
                if j + 1 < bytes.len() && bytes[j] == b'=' && bytes[j + 1] == b'?' {
                    i = j;
                }
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    Ok(out)
}

fn try_decode_word(bytes: &[u8], start: usize) -> Option<(String, usize)> {
    if start + 6 >= bytes.len() || bytes[start] != b'=' || bytes[start + 1] != b'?' {
        return None;
    }
    let mut parts = Vec::new();
    let mut i = start + 2;
    for _ in 0..3 {
        let begin = i;
        while i < bytes.len() && bytes[i] != b'?' {
            i += 1;
        }
        if i >= bytes.len() {
            return None;
        }
        parts.push(&bytes[begin..i]);
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'=' {
        return None;
    }
    i += 1;
    let charset = std::str::from_utf8(parts[0]).ok()?.to_ascii_lowercase();
    let encoding = parts[1];
    let text = parts[2];
    let raw = match encoding {
        b"B" | b"b" => base64::decode_standard(std::str::from_utf8(text).ok()?).ok()?,
        b"Q" | b"q" => decode_q(text)?,
        _ => return None,
    };
    let decoded = match charset.as_str() {
        "utf-8" | "utf8" | "us-ascii" | "ascii" => String::from_utf8(raw).ok()?,
        "iso-8859-1" | "latin1" => raw.into_iter().map(|b| b as char).collect(),
        _ => String::from_utf8_lossy(&raw).into_owned(),
    };
    Some((decoded, i))
}

fn decode_q(text: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        match text[i] {
            b'_' => {
                out.push(b' ');
                i += 1;
            }
            b'=' => {
                if i + 2 >= text.len() {
                    return None;
                }
                let hi = from_hex(text[i + 1])?;
                let lo = from_hex(text[i + 2])?;
                out.push((hi << 4) | lo);
                i += 3;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    Some(out)
}

fn from_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_passthrough() {
        assert_eq!(encode_header("Hello"), "Hello");
        assert_eq!(decode_header("Hello").unwrap(), "Hello");
    }

    #[test]
    fn unicode_roundtrip() {
        let enc = encode_header("café");
        assert!(enc.contains("=?UTF-8?B?"));
        assert_eq!(decode_header(&enc).unwrap(), "café");
    }

    #[test]
    fn q_encoded() {
        assert_eq!(decode_header("=?UTF-8?Q?caf=C3=A9?=").unwrap(), "café");
    }
}
