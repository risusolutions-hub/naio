use crate::error::{Error, Result};

pub struct BitReader<'a> {
    data: &'a [u8],
    pub(crate) pos: usize,
    bit_buf: u32,
    bit_count: u8,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            bit_buf: 0,
            bit_count: 0,
        }
    }

    pub fn remaining_bytes(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn ensure_bits(&mut self, n: u8) -> Result<()> {
        while self.bit_count < n {
            if self.pos >= self.data.len() {
                return Err(Error::Truncated);
            }
            self.bit_buf |= u32::from(self.data[self.pos]) << self.bit_count;
            self.pos += 1;
            self.bit_count += 8;
        }
        Ok(())
    }

    pub fn take_bits(&mut self, n: u8) -> Result<u32> {
        self.ensure_bits(n)?;
        let mask = if n == 32 {
            u32::MAX
        } else {
            (1u32 << n) - 1
        };
        let v = self.bit_buf & mask;
        self.bit_buf >>= n;
        self.bit_count -= n;
        Ok(v)
    }

    pub fn align_byte(&mut self) {
        if self.bit_count > 0 {
            self.bit_count = 0;
            self.bit_buf = 0;
        }
    }
}
