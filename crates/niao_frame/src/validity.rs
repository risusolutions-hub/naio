//! Null / validity bitmap (1 bit per row).

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Validity {
    bits: Vec<u8>,
    len: usize,
}

impl Validity {
    #[inline]
    pub fn all_valid(len: usize) -> Self {
        let byte_len = len.div_ceil(8);
        Self {
            bits: vec![0xFF; byte_len],
            len,
        }
    }

    #[inline]
    pub fn all_null(len: usize) -> Self {
        let byte_len = len.div_ceil(8);
        Self {
            bits: vec![0; byte_len],
            len,
        }
    }

    pub fn from_bools(mask: &[bool]) -> Self {
        let mut v = Self::all_valid(mask.len());
        for (i, &ok) in mask.iter().enumerate() {
            if !ok {
                v.set_null(i);
            }
        }
        v
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_valid(&self, i: usize) -> bool {
        if i >= self.len {
            return false;
        }
        (self.bits[i / 8] >> (i % 8)) & 1 == 1
    }

    #[inline]
    pub fn is_null(&self, i: usize) -> bool {
        !self.is_valid(i)
    }

    #[inline]
    pub fn set_null(&mut self, i: usize) {
        if i < self.len {
            self.bits[i / 8] &= !(1 << (i % 8));
        }
    }

    #[inline]
    pub fn set_valid(&mut self, i: usize) {
        if i < self.len {
            self.bits[i / 8] |= 1 << (i % 8);
        }
    }

    pub fn null_count(&self) -> usize {
        (0..self.len).filter(|&i| self.is_null(i)).count()
    }

    pub fn take(&self, indices: &[usize]) -> Self {
        let mut out = Self::all_valid(indices.len());
        for (j, &i) in indices.iter().enumerate() {
            if !self.is_valid(i) {
                out.set_null(j);
            }
        }
        out
    }

    pub fn slice(&self, start: usize, end: usize) -> Self {
        let end = end.min(self.len);
        let start = start.min(end);
        let mut out = Self::all_valid(end - start);
        for (j, i) in (start..end).enumerate() {
            if !self.is_valid(i) {
                out.set_null(j);
            }
        }
        out
    }
}
