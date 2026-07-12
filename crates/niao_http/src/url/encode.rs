//! Percent-encoding and application/x-www-form-urlencoded helpers.

#[inline]
fn hex(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        _ => (b'A' + n - 10) as char,
    }
}

#[inline]
fn from_hex(b: u8) -> Result<u8, String> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err("bad hex".into()),
    }
}

/// RFC 3986 unreserved + sub-delims safe set; everything else percent-encoded.
#[inline]
pub fn percent_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(hex(b >> 4));
            out.push(hex(b & 0xf));
        }
    }
    out
}

/// Decode percent-escapes and treat `+` as space (HTML form convention).
pub fn percent_decode(s: &str) -> Result<String, String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'+' {
            out.push(b' ');
            i += 1;
        } else if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err("bad percent escape".into());
            }
            let hi = from_hex(bytes[i + 1])?;
            let lo = from_hex(bytes[i + 2])?;
            out.push((hi << 4) | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "invalid utf8".into())
}

/// Application/x-www-form-urlencoded: spaces as `+`, rest like percent_encode.
pub fn form_urlencode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else if b == b' ' {
            out.push('+');
        } else {
            out.push('%');
            out.push(hex(b >> 4));
            out.push(hex(b & 0xf));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let s = "hello world!@#";
        assert_eq!(percent_decode(&percent_encode(s.as_bytes())).unwrap(), s);
    }

    #[test]
    fn form_space_plus() {
        assert_eq!(form_urlencode(b"a b"), "a+b");
        assert_eq!(percent_decode("a+b").unwrap(), "a b");
    }
}
