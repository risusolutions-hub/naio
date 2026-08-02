use std::cmp::Ordering;

pub(crate) const LIMB_BITS: u32 = 64;
/// Karatsuba kicks in above 256 bits (4 × u64 limbs).
pub(crate) const KARATSUBA_THRESHOLD: usize = 4;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BigUint {
    pub limbs: Vec<u64>,
}

fn add_into(dst: &mut Vec<u64>, src: &[u64], shift: usize) {
    if src.is_empty() {
        return;
    }
    let need = shift + src.len();
    if dst.len() < need {
        dst.resize(need, 0);
    }
    let mut carry = 0u128;
    for (i, &s) in src.iter().enumerate() {
        let idx = shift + i;
        let sum = dst[idx] as u128 + s as u128 + carry;
        dst[idx] = sum as u64;
        carry = sum >> 64;
    }
    let mut idx = shift + src.len();
    while carry != 0 {
        if idx >= dst.len() {
            dst.push(carry as u64);
            carry = 0;
        } else {
            let sum = dst[idx] as u128 + carry;
            dst[idx] = sum as u64;
            carry = sum >> 64;
            idx += 1;
        }
    }
}

fn sub_from(dst: &mut [u64], src: &[u64]) {
    let mut borrow = 0i128;
    for (i, &s) in src.iter().enumerate() {
        let mut diff = dst[i] as i128 - s as i128 - borrow;
        if diff < 0 {
            diff += 1i128 << 64;
            borrow = 1;
        } else {
            borrow = 0;
        }
        dst[i] = diff as u64;
    }
    debug_assert_eq!(borrow, 0);
}

fn schoolbook_into(a: &[u64], b: &[u64], out: &mut [u64]) {
    out.fill(0);
    for (i, &av) in a.iter().enumerate() {
        if av == 0 {
            continue;
        }
        let mut carry = 0u128;
        for (j, &bv) in b.iter().enumerate() {
            let idx = i + j;
            let prod = av as u128 * bv as u128 + out[idx] as u128 + carry;
            out[idx] = prod as u64;
            carry = prod >> 64;
        }
        if carry != 0 {
            let idx = i + b.len();
            let sum = out[idx] as u128 + carry;
            out[idx] = sum as u64;
            if sum >> 64 != 0 && idx + 1 < out.len() {
                out[idx + 1] = out[idx + 1].wrapping_add((sum >> 64) as u64);
            }
        }
    }
}

fn add_slices(a_lo: &[u64], a_hi: &[u64], out: &mut Vec<u64>) {
    let n = a_lo.len().max(a_hi.len());
    out.clear();
    out.resize(n + 1, 0);
    let mut carry = 0u128;
    for i in 0..n {
        let sum = *a_lo.get(i).unwrap_or(&0) as u128 + *a_hi.get(i).unwrap_or(&0) as u128 + carry;
        out[i] = sum as u64;
        carry = sum >> 64;
    }
    if carry != 0 {
        out[n] = carry as u64;
    } else if out.len() > 1 && out.last() == Some(&0) {
        out.pop();
    }
}

fn mul_karatsuba_into(a: &[u64], b: &[u64], out: &mut Vec<u64>) {
    out.fill(0);
    let n = a.len().max(b.len());
    if n <= KARATSUBA_THRESHOLD {
        schoolbook_into(a, b, out);
        return;
    }
    let m = (n + 1) / 2;
    let a_lo = &a[..a.len().min(m)];
    let a_hi = if a.len() > m { &a[m..] } else { &[] as &[u64] };
    let b_lo = &b[..b.len().min(m)];
    let b_hi = if b.len() > m { &b[m..] } else { &[] as &[u64] };

    let z_len = m * 2 + 2;
    let mut z0 = vec![0u64; z_len];
    let mut z1 = vec![0u64; z_len + 2];
    let mut z2 = vec![0u64; z_len];
    let mut sum_a = Vec::with_capacity(m + 1);
    let mut sum_b = Vec::with_capacity(m + 1);

    mul_karatsuba_into(a_lo, b_lo, &mut z0);
    mul_karatsuba_into(a_hi, b_hi, &mut z2);
    add_slices(a_lo, a_hi, &mut sum_a);
    add_slices(b_lo, b_hi, &mut sum_b);
    mul_karatsuba_into(&sum_a, &sum_b, &mut z1);
    sub_from(&mut z1, &z0);
    sub_from(&mut z1, &z2);

    add_into(out, &z0, 0);
    add_into(out, &z1, m);
    add_into(out, &z2, m + m);
}

impl BigUint {
    #[inline]
    pub fn zero() -> Self {
        Self { limbs: vec![0] }
    }

    #[inline]
    pub fn is_zero(&self) -> bool {
        self.limbs.len() == 1 && self.limbs[0] == 0
    }

    pub fn from_u64(v: u64) -> Self {
        if v == 0 {
            Self::zero()
        } else {
            Self { limbs: vec![v] }
        }
    }

    pub fn from_u128(v: u128) -> Self {
        if v == 0 {
            return Self::zero();
        }
        let lo = v as u64;
        let hi = (v >> 64) as u64;
        if hi == 0 {
            Self { limbs: vec![lo] }
        } else {
            Self {
                limbs: vec![lo, hi],
            }
        }
    }

    pub fn normalize(&mut self) {
        while self.limbs.len() > 1 && self.limbs.last() == Some(&0) {
            self.limbs.pop();
        }
        if self.limbs.is_empty() {
            self.limbs.push(0);
        }
    }

    pub fn normalized(mut self) -> Self {
        self.normalize();
        self
    }

    #[inline]
    pub fn bit_len(&self) -> u32 {
        if self.is_zero() {
            return 0;
        }
        let top = *self.limbs.last().unwrap();
        (self.limbs.len() as u32 - 1) * LIMB_BITS + (64 - top.leading_zeros())
    }

    pub fn cmp_mag(&self, other: &Self) -> Ordering {
        let la = self.limbs.len();
        let lb = other.limbs.len();
        match la.cmp(&lb) {
            Ordering::Equal => {
                for (a, b) in self.limbs.iter().rev().zip(other.limbs.iter().rev()) {
                    match a.cmp(b) {
                        Ordering::Equal => {}
                        ord => return ord,
                    }
                }
                Ordering::Equal
            }
            ord => ord,
        }
    }

    pub fn add_mag(&self, other: &Self) -> Self {
        let n = self.limbs.len().max(other.limbs.len());
        let mut out = vec![0u64; n + 1];
        add_into(&mut out, &self.limbs, 0);
        add_into(&mut out, &other.limbs, 0);
        Self { limbs: out }.normalized()
    }

    pub fn sub_mag(&self, other: &Self) -> Self {
        debug_assert!(self.cmp_mag(other) != Ordering::Less);
        let mut out = self.limbs.clone();
        sub_from(&mut out, &other.limbs);
        Self { limbs: out }.normalized()
    }

    pub fn mul_schoolbook(&self, other: &Self) -> Self {
        if self.is_zero() || other.is_zero() {
            return Self::zero();
        }
        let mut result = vec![0u64; self.limbs.len() + other.limbs.len()];
        schoolbook_into(&self.limbs, &other.limbs, &mut result);
        Self { limbs: result }.normalized()
    }

    pub fn mul_limb(&self, limb: u64) -> Self {
        if limb == 0 || self.is_zero() {
            return Self::zero();
        }
        let mut out = Vec::with_capacity(self.limbs.len() + 1);
        let mut carry = 0u128;
        for &a in &self.limbs {
            let prod = a as u128 * limb as u128 + carry;
            out.push(prod as u64);
            carry = prod >> 64;
        }
        if carry != 0 {
            out.push(carry as u64);
        }
        Self { limbs: out }.normalized()
    }

    pub fn mul_karatsuba(&self, other: &Self) -> Self {
        if self.is_zero() || other.is_zero() {
            return Self::zero();
        }
        let mut out = vec![0u64; self.limbs.len() + other.limbs.len()];
        mul_karatsuba_into(&self.limbs, &other.limbs, &mut out);
        Self { limbs: out }.normalized()
    }

    pub fn mul(&self, other: &Self) -> Self {
        let n = self.limbs.len().max(other.limbs.len());
        if n <= KARATSUBA_THRESHOLD {
            self.mul_schoolbook(other)
        } else {
            self.mul_karatsuba(other)
        }
    }

    pub fn div_rem(&self, divisor: &Self) -> (Self, Self) {
        assert!(!divisor.is_zero(), "division by zero");
        match self.cmp_mag(divisor) {
            Ordering::Less => return (Self::zero(), self.clone()),
            Ordering::Equal => return (Self::from_u64(1), Self::zero()),
            Ordering::Greater => {}
        }
        if divisor.limbs.len() == 1 {
            return self.div_rem_limb(divisor.limbs[0]);
        }

        let bits = self.bit_len();
        let mut quotient = Self::zero();
        let mut remainder = Self::zero();
        for bit in (0..bits).rev() {
            remainder = remainder.shl_limbs(1);
            if self.bit_at(bit) {
                remainder.limbs[0] |= 1;
                remainder.normalize();
            }
            if remainder.cmp_mag(divisor) != Ordering::Less {
                remainder = remainder.sub_mag(divisor);
                quotient = quotient.add_mag(&Self::from_u64(1).shl_limbs(bit));
            }
        }
        (quotient.normalized(), remainder)
    }

    #[inline]
    fn bit_at(&self, bit: u32) -> bool {
        let limb = (bit / LIMB_BITS) as usize;
        let offset = bit % LIMB_BITS;
        self.limbs.get(limb).copied().unwrap_or(0) & (1 << offset) != 0
    }

    fn div_rem_limb(&self, divisor: u64) -> (Self, Self) {
        let mut q = Vec::with_capacity(self.limbs.len());
        let mut rem = 0u128;
        for &limb in self.limbs.iter().rev() {
            let cur = (rem << 64) | limb as u128;
            let digit = cur / divisor as u128;
            rem = cur % divisor as u128;
            q.push(digit as u64);
        }
        q.reverse();
        (Self { limbs: q }.normalized(), Self::from_u64(rem as u64))
    }

    fn shl_limbs(&self, bits: u32) -> Self {
        if bits == 0 || self.is_zero() {
            return self.clone();
        }
        let limb_shift = (bits / LIMB_BITS) as usize;
        let bit_shift = bits % LIMB_BITS;
        let mut out = vec![0u64; limb_shift];
        if bit_shift == 0 {
            out.extend_from_slice(&self.limbs);
            return Self { limbs: out }.normalized();
        }
        let mut carry = 0u64;
        for &limb in &self.limbs {
            out.push((limb << bit_shift) | carry);
            carry = limb >> (LIMB_BITS - bit_shift);
        }
        if carry != 0 {
            out.push(carry);
        }
        Self { limbs: out }.normalized()
    }

    pub fn to_radix_be(&self, radix: u32) -> Vec<u8> {
        if self.is_zero() {
            return vec![0];
        }
        if radix == 10 {
            return self.to_decimal_digit_values();
        }
        let mut n = self.clone();
        let base = BigUint::from_u64(radix as u64);
        let mut digits = Vec::new();
        while !n.is_zero() {
            let (q, r) = n.div_rem(&base);
            digits.push(r.limbs[0] as u8);
            n = q;
        }
        digits.reverse();
        digits
    }

    fn to_decimal_digit_values(&self) -> Vec<u8> {
        const CHUNK: u64 = 1_000_000_000;
        let mut n = self.clone();
        let mut chunks = Vec::new();
        while !n.is_zero() {
            let (q, r) = n.div_rem_limb(CHUNK);
            chunks.push(r.limbs[0]);
            n = q;
        }
        let mut out = Vec::new();
        for (idx, chunk) in chunks.iter().rev().enumerate() {
            let s = if idx == 0 {
                chunk.to_string()
            } else {
                format!("{chunk:09}")
            };
            out.extend(s.bytes().map(|b| b - b'0'));
        }
        out
    }

    pub fn from_radix_be(digits: &[u8], radix: u32) -> Option<Self> {
        let base = radix as u64;
        let mut acc = Self::zero();
        for &d in digits {
            if d as u64 >= base {
                return None;
            }
            acc = acc.mul_limb(base).add_mag(&Self::from_u64(d as u64));
        }
        Some(acc)
    }
}

#[cfg(test)]
mod uint_tests {
    use super::*;

    #[test]
    fn div_small() {
        let a = BigUint::from_u64(100);
        let b = BigUint::from_u64(7);
        let (q, r) = a.div_rem(&b);
        assert_eq!(q.limbs[0], 14);
        assert_eq!(r.limbs[0], 2);
    }

    #[test]
    fn karatsuba_matches_schoolbook() {
        let mut a = BigUint::from_u64(0xDEAD_BEEF_CAFE_BABEu64);
        let mut b = BigUint::from_u64(0x0123_4567_89AB_CDEFu64);
        for _ in 0..6 {
            a = a.mul_limb(0x100000001B3).add_mag(&BigUint::from_u64(0x27));
            b = b.mul_limb(0x100000001B3).add_mag(&BigUint::from_u64(0x42));
        }
        assert_eq!(a.mul_schoolbook(&b), a.mul_karatsuba(&b));
    }

    #[test]
    fn div_large() {
        fn dec(s: &str) -> BigUint {
            let digits: Vec<u8> = s.bytes().map(|b| b - b'0').collect();
            BigUint::from_radix_be(&digits, 10).unwrap()
        }
        let a = dec("999999999999999999999999999999");
        let b = dec("123456789012345678901234567890");
        let (q, r) = a.div_rem(&b);
        let prod = q.mul(&b).add_mag(&r);
        assert_eq!(prod.cmp_mag(&a), Ordering::Equal);
        assert!(r.cmp_mag(&b) == Ordering::Less);
    }
}
