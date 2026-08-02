//! Mutable bit-level buffer (Python `bitstring` subset).

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitString {
    /// Packed bits, MSB-first within each byte.
    bytes: Vec<u8>,
    /// Total logical bit length (may be less than bytes.len() * 8).
    bit_len: usize,
    /// Read/write cursor in bits.
    pos: usize,
}

impl BitString {
    pub fn new(bit_len: usize) -> Self {
        let byte_len = bit_len.div_ceil(8);
        BitString {
            bytes: vec![0u8; byte_len],
            bit_len,
            pos: 0,
        }
    }

    pub fn from_bytes(data: &[u8], bit_len: Option<usize>) -> Self {
        let bl = bit_len.unwrap_or(data.len() * 8);
        if bl > data.len() * 8 {
            let mut bytes = data.to_vec();
            bytes.resize(bl.div_ceil(8), 0);
            BitString {
                bytes,
                bit_len: bl,
                pos: 0,
            }
        } else {
            BitString {
                bytes: data.to_vec(),
                bit_len: bl,
                pos: 0,
            }
        }
    }

    pub fn len(&self) -> usize {
        self.bit_len
    }

    pub fn is_empty(&self) -> bool {
        self.bit_len == 0
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn seek(&mut self, pos: usize) {
        self.pos = pos.min(self.bit_len);
    }

    pub fn get(&self, index: usize) -> Result<bool, String> {
        if index >= self.bit_len {
            return Err(format!(
                "bit index {index} out of range (len={})",
                self.bit_len
            ));
        }
        Ok(self.bit_at(index))
    }

    pub fn set(&mut self, index: usize, value: bool) -> Result<(), String> {
        if index >= self.bit_len {
            return Err(format!(
                "bit index {index} out of range (len={})",
                self.bit_len
            ));
        }
        self.set_bit_at(index, value);
        Ok(())
    }

    /// Read `n` bits from cursor, MSB-first within the field.
    pub fn read(&mut self, n: usize) -> Result<u64, String> {
        if n == 0 {
            return Ok(0);
        }
        if n > 64 {
            return Err("cannot read more than 64 bits at once".into());
        }
        if self.pos + n > self.bit_len {
            return Err(format!(
                "read {n} bits at pos {} exceeds length {}",
                self.pos, self.bit_len
            ));
        }
        let mut out = 0u64;
        for i in 0..n {
            if self.bit_at(self.pos + i) {
                out = (out << 1) | 1;
            } else {
                out <<= 1;
            }
        }
        self.pos += n;
        Ok(out)
    }

    /// Write low `n` bits of `value` at cursor.
    pub fn write(&mut self, n: usize, value: u64) -> Result<(), String> {
        if n == 0 {
            return Ok(());
        }
        if n > 64 {
            return Err("cannot write more than 64 bits at once".into());
        }
        if self.pos + n > self.bit_len {
            return Err(format!(
                "write {n} bits at pos {} exceeds length {}",
                self.pos, self.bit_len
            ));
        }
        for i in (0..n).rev() {
            let bit = (value >> i) & 1 == 1;
            self.set_bit_at(self.pos, bit);
            self.pos += 1;
        }
        Ok(())
    }

    /// Append bits from another BitString.
    pub fn append(&mut self, other: &BitString) {
        let old_len = self.bit_len;
        let new_len = old_len + other.bit_len;
        self.bytes.resize(new_len.div_ceil(8), 0);
        self.bit_len = new_len;
        for i in 0..other.bit_len {
            self.set_bit_at(old_len + i, other.bit_at(i));
        }
    }

    /// Extract a sub-range `[start, end)`.
    pub fn slice(&self, start: usize, end: usize) -> Result<BitString, String> {
        if start > end || end > self.bit_len {
            return Err(format!(
                "invalid slice [{start}, {end}) for length {}",
                self.bit_len
            ));
        }
        let mut out = BitString::new(end - start);
        for i in start..end {
            out.set_bit_at(i - start, self.bit_at(i));
        }
        Ok(out)
    }

    /// Bytes representation; `pad` right-pads to byte boundary when true.
    pub fn to_bytes(&self, pad: bool) -> Vec<u8> {
        let end = if pad {
            self.bit_len.div_ceil(8)
        } else {
            self.bit_len / 8 + usize::from(self.bit_len % 8 != 0)
        };
        let mut out = self.bytes.clone();
        out.truncate(end);
        if !pad && self.bit_len % 8 != 0 {
            let mask = 0xFF_u8 << (8 - self.bit_len % 8);
            if let Some(last) = out.last_mut() {
                *last &= mask;
            }
        }
        out
    }

    pub fn hex(&self) -> String {
        let bytes = self.to_bytes(true);
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    pub fn uintbe(&self, bit_len: usize) -> Result<u64, String> {
        if bit_len == 0 || bit_len > 64 {
            return Err("uintbe bit_len must be 1..=64".into());
        }
        if self.bit_len < bit_len {
            return Err("bitstring shorter than requested width".into());
        }
        let mut out = 0u64;
        for i in 0..bit_len {
            if self.bit_at(i) {
                out = (out << 1) | 1;
            } else {
                out <<= 1;
            }
        }
        Ok(out)
    }

    pub fn uintle(&self, bit_len: usize) -> Result<u64, String> {
        if bit_len == 0 || bit_len > 64 {
            return Err("uintle bit_len must be 1..=64".into());
        }
        if self.bit_len < bit_len {
            return Err("bitstring shorter than requested width".into());
        }
        let mut out = 0u64;
        for i in 0..bit_len {
            if self.bit_at(i) {
                out |= 1u64 << i;
            }
        }
        Ok(out)
    }

    #[inline]
    fn bit_at(&self, index: usize) -> bool {
        let byte = index / 8;
        let bit = 7 - (index % 8);
        (self.bytes[byte] >> bit) & 1 == 1
    }

    fn set_bit_at(&mut self, index: usize, value: bool) {
        let byte = index / 8;
        let bit = 7 - (index % 8);
        if value {
            self.bytes[byte] |= 1 << bit;
        } else {
            self.bytes[byte] &= !(1 << bit);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_write_roundtrip() {
        let mut bs = BitString::new(16);
        bs.write(5, 0b10101).unwrap();
        bs.seek(0);
        assert_eq!(bs.read(5).unwrap(), 0b10101);
    }

    #[test]
    fn from_bytes_uintbe() {
        let bs = BitString::from_bytes(&[0b1010_0000], None);
        assert_eq!(bs.uintbe(4).unwrap(), 0b1010);
    }
}
