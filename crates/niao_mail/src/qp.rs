//! Quoted-printable encode/decode (RFC 2045).

/// Encode bytes as quoted-printable, soft-wrapping at 76 columns.
pub fn encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len() + data.len() / 4);
    let mut col = 0usize;
    for &b in data {
        let needs_encode = b > 126
            || b < 32 && b != b'\t' && b != b'\n' && b != b'\r'
            || b == b'='
            || b == b'.' && col == 0;

        if b == b'\r' {
            continue;
        }
        if b == b'\n' {
            out.push_str("\r\n");
            col = 0;
            continue;
        }

        let encoded_len = if needs_encode { 3 } else { 1 };
        if col + encoded_len > 75 {
            out.push_str("=\r\n");
            col = 0;
        }
        if needs_encode {
            out.push('=');
            out.push(hex_digit(b >> 4));
            out.push(hex_digit(b & 0x0f));
            col += 3;
        } else {
            out.push(b as char);
            col += 1;
        }
    }
    out
}

/// Decode quoted-printable into bytes. Soft line breaks (`=\r\n` / `=\n`) are removed.
pub fn decode(input: &str) -> Result<Vec<u8>, String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'=' => {
                if i + 1 < bytes.len() && (bytes[i + 1] == b'\n') {
                    i += 2;
                    continue;
                }
                if i + 2 < bytes.len() && bytes[i + 1] == b'\r' && bytes[i + 2] == b'\n' {
                    i += 3;
                    continue;
                }
                if i + 2 >= bytes.len() {
                    return Err("truncated quoted-printable escape".into());
                }
                let hi = from_hex(bytes[i + 1])?;
                let lo = from_hex(bytes[i + 2])?;
                out.push((hi << 4) | lo);
                i += 3;
            }
            b'\r' => i += 1,
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    Ok(out)
}

fn hex_digit(n: u8) -> char {
    b"0123456789ABCDEF"[n as usize] as char
}

fn from_hex(b: u8) -> Result<u8, String> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        _ => Err(format!(
            "invalid hex digit in quoted-printable: {}",
            b as char
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_ascii() {
        let s = b"Hello = world\nNext line";
        let enc = encode(s);
        let dec = decode(&enc).unwrap();
        assert_eq!(dec, s);
    }

    #[test]
    fn soft_break_removed() {
        let dec = decode("Hello=\r\n world").unwrap();
        assert_eq!(dec, b"Hello world");
    }
}
