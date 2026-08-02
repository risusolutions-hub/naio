//! Protobuf-style varints and zigzag encoding.

#[inline]
pub fn uvarint_encode(mut n: u64) -> Vec<u8> {
    if n == 0 {
        return vec![0];
    }
    let mut out = Vec::with_capacity(10);
    while n >= 0x80 {
        out.push((n as u8) | 0x80);
        n >>= 7;
    }
    out.push(n as u8);
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarintResult {
    pub value: u64,
    pub bytes_read: usize,
}

/// Decode unsigned varint starting at `offset`. Returns error if truncated or overflow.
pub fn uvarint_decode(data: &[u8], offset: usize) -> Result<VarintResult, String> {
    let mut result: u64 = 0;
    let mut shift = 0u32;
    let mut i = offset;
    while i < data.len() {
        let byte = data[i];
        i += 1;
        if shift >= 64 {
            return Err("uvarint overflow".into());
        }
        result |= u64::from(byte & 0x7F) << shift;
        if byte & 0x80 == 0 {
            return Ok(VarintResult {
                value: result,
                bytes_read: i - offset,
            });
        }
        shift += 7;
        if shift > 70 {
            return Err("uvarint too long".into());
        }
    }
    Err("unexpected end of buffer during uvarint decode".into())
}

#[inline]
pub fn zigzag_encode(n: i64) -> u64 {
    ((n << 1) ^ (n >> 63)) as u64
}

#[inline]
pub fn zigzag_decode(n: u64) -> i64 {
    ((n >> 1) as i64) ^ (-((n & 1) as i64))
}

#[inline]
pub fn varint_encode(n: i64) -> Vec<u8> {
    uvarint_encode(zigzag_encode(n))
}

pub fn varint_decode(data: &[u8], offset: usize) -> Result<(i64, usize), String> {
    let r = uvarint_decode(data, offset)?;
    Ok((zigzag_decode(r.value), r.bytes_read))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        for n in [0u64, 1, 127, 128, 16_383, 16_384, u64::MAX] {
            let enc = uvarint_encode(n);
            let dec = uvarint_decode(&enc, 0).unwrap();
            assert_eq!(dec.value, n);
            assert_eq!(dec.bytes_read, enc.len());
        }
        for n in [0i64, -1, 1, -128, 127, i64::MIN, i64::MAX] {
            let enc = varint_encode(n);
            let (v, len) = varint_decode(&enc, 0).unwrap();
            assert_eq!(v, n);
            assert_eq!(len, enc.len());
        }
    }
}
