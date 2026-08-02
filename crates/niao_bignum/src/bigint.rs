use crate::sign::Sign;
use crate::uint::BigUint;
use std::cmp::Ordering;
use std::fmt::{self, Write as _};
use std::str::FromStr;

#[derive(Clone, Eq, PartialEq)]
pub struct BigInt {
    sign: Sign,
    magnitude: BigUint,
}

impl BigInt {
    #[inline]
    pub fn zero() -> Self {
        Self {
            sign: Sign::NoSign,
            magnitude: BigUint::zero(),
        }
    }

    #[inline]
    pub fn is_zero(&self) -> bool {
        self.magnitude.is_zero()
    }

    pub fn sign(&self) -> Sign {
        if self.is_zero() {
            Sign::NoSign
        } else {
            self.sign
        }
    }

    pub fn abs(&self) -> Self {
        Self {
            sign: if self.is_zero() {
                Sign::NoSign
            } else {
                Sign::Plus
            },
            magnitude: self.magnitude.clone(),
        }
    }

    fn from_mag(sign: Sign, magnitude: BigUint) -> Self {
        if magnitude.is_zero() {
            Self::zero()
        } else {
            Self { sign, magnitude }
        }
    }

    pub fn from_u64(v: u64) -> Self {
        Self::from_mag(Sign::Plus, BigUint::from_u64(v))
    }

    pub fn from_i64(v: i64) -> Self {
        if v == 0 {
            Self::zero()
        } else if v > 0 {
            Self::from_mag(Sign::Plus, BigUint::from_u64(v as u64))
        } else {
            Self::from_mag(Sign::Minus, BigUint::from_u64((-(v as i128)) as u64))
        }
    }

    pub fn from_u128(v: u128) -> Self {
        Self::from_mag(Sign::Plus, BigUint::from_u128(v))
    }

    pub fn from_i128(v: i128) -> Self {
        if v == 0 {
            Self::zero()
        } else if v > 0 {
            Self::from_mag(Sign::Plus, BigUint::from_u128(v as u128))
        } else {
            Self::from_mag(Sign::Minus, BigUint::from_u128((-(v as i128)) as u128))
        }
    }

    pub fn parse_bytes(s: &[u8], radix: u32) -> Option<Self> {
        if s.is_empty() || radix < 2 || radix > 256 {
            return None;
        }
        let mut sign = Sign::Plus;
        let mut start = 0;
        if s[0] == b'-' {
            sign = Sign::Minus;
            start = 1;
        } else if s[0] == b'+' {
            start = 1;
        }
        if start >= s.len() {
            return None;
        }
        let mut digits = Vec::with_capacity(s.len() - start);
        for &byte in &s[start..] {
            let digit = match byte {
                b'0'..=b'9' => byte - b'0',
                b'a'..=b'z' => byte - b'a' + 10,
                b'A'..=b'Z' => byte - b'A' + 10,
                _ => return None,
            };
            if digit as u32 >= radix {
                return None;
            }
            digits.push(digit);
        }
        let mag = BigUint::from_radix_be(&digits, radix)?;
        Some(Self::from_mag(sign, mag))
    }

    pub fn to_radix_be(&self, radix: u32) -> (Sign, Vec<u8>) {
        (self.sign(), self.magnitude.to_radix_be(radix))
    }

    /// Returns `Some` when the value fits in `u64` (non-negative only).
    pub fn to_u64(&self) -> Option<u64> {
        if self.sign() == Sign::Minus {
            return None;
        }
        match self.magnitude.limbs.as_slice() {
            [] => Some(0),
            [lo] => Some(*lo),
            _ => None,
        }
    }

    /// Returns `Some` when the value fits in `i64`.
    pub fn to_i64(&self) -> Option<i64> {
        match self.sign() {
            Sign::NoSign => Some(0),
            Sign::Plus => {
                let u = self.to_u64()?;
                (u <= i64::MAX as u64).then(|| u as i64)
            }
            Sign::Minus => {
                if self.magnitude.limbs.len() > 1 {
                    return None;
                }
                let mag = self.magnitude.limbs.first().copied().unwrap_or(0);
                if mag > (i64::MAX as u64) + 1 {
                    None
                } else if mag == (i64::MAX as u64) + 1 {
                    Some(i64::MIN)
                } else {
                    Some(-(mag as i64))
                }
            }
        }
    }

    pub fn pow(&self, exp: u32) -> Self {
        if exp == 0 {
            return Self::from_u64(1);
        }
        if self.is_zero() {
            return Self::zero();
        }
        let mut base = self.clone();
        let mut result = Self::from_u64(1);
        let mut e = exp;
        while e > 0 {
            if e & 1 == 1 {
                result = &result * &base;
            }
            base = &base * &base;
            e >>= 1;
        }
        result
    }

    pub fn mod_floor(&self, other: &Self) -> Self {
        let (_, rem) = self.div_rem(other);
        if rem.sign() == Sign::Minus && other.sign() != Sign::Minus {
            rem + other
        } else {
            rem
        }
    }

    pub fn div_rem(&self, other: &Self) -> (Self, Self) {
        if other.is_zero() {
            panic!("division by zero");
        }
        if self.is_zero() {
            return (Self::zero(), Self::zero());
        }

        let self_neg = self.sign == Sign::Minus;
        let other_neg = other.sign == Sign::Minus;
        let (q_mag, r_mag) = self.magnitude.div_rem(&other.magnitude);

        let q_sign = if q_mag.is_zero() {
            Sign::NoSign
        } else if self_neg ^ other_neg {
            Sign::Minus
        } else {
            Sign::Plus
        };
        let r_sign = if r_mag.is_zero() {
            Sign::NoSign
        } else if self_neg {
            Sign::Minus
        } else {
            Sign::Plus
        };

        (Self::from_mag(q_sign, q_mag), Self::from_mag(r_sign, r_mag))
    }
}

impl Default for BigInt {
    fn default() -> Self {
        Self::zero()
    }
}

impl PartialOrd for BigInt {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BigInt {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.sign(), other.sign()) {
            (Sign::NoSign, Sign::NoSign) => Ordering::Equal,
            (Sign::Minus, _) if other.sign() != Sign::Minus => Ordering::Less,
            (_, Sign::Minus) if self.sign() != Sign::Minus => Ordering::Greater,
            (Sign::Plus, Sign::Plus) => self.magnitude.cmp_mag(&other.magnitude),
            (Sign::Minus, Sign::Minus) => other.magnitude.cmp_mag(&self.magnitude),
            _ => Ordering::Equal,
        }
    }
}

impl fmt::Display for BigInt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_zero() {
            return f.write_str("0");
        }
        if self.sign == Sign::Minus {
            f.write_str("-")?;
        }
        let digits = self.magnitude.to_radix_be(10);
        for d in digits {
            f.write_char(char::from(b'0' + d))?;
        }
        Ok(())
    }
}

impl fmt::Debug for BigInt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl FromStr for BigInt {
    type Err = ParseBigIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(ParseBigIntError);
        }
        Self::parse_bytes(trimmed.as_bytes(), 10).ok_or(ParseBigIntError)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseBigIntError;

impl fmt::Display for ParseBigIntError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid bigint literal")
    }
}

impl std::error::Error for ParseBigIntError {}

macro_rules! impl_from_int {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl From<$ty> for BigInt {
                fn from(v: $ty) -> Self {
                    if std::mem::size_of::<$ty>() <= 8 {
                        Self::from_i64(v as i64)
                    } else {
                        Self::from_i128(v as i128)
                    }
                }
            }
        )+
    };
}

impl_from_int!(i8, i16, i32, i64, u8, u16, u32, u64, i128, u128, isize, usize);

impl std::ops::Neg for BigInt {
    type Output = Self;

    fn neg(self) -> Self::Output {
        if self.is_zero() {
            self
        } else {
            Self {
                sign: self.sign.flip(),
                magnitude: self.magnitude,
            }
        }
    }
}

impl std::ops::Neg for &BigInt {
    type Output = BigInt;

    fn neg(self) -> Self::Output {
        -self.clone()
    }
}

impl std::ops::Add for &BigInt {
    type Output = BigInt;

    fn add(self, other: &BigInt) -> BigInt {
        match (self.sign(), other.sign()) {
            (Sign::NoSign, _) => other.clone(),
            (_, Sign::NoSign) => self.clone(),
            (Sign::Plus, Sign::Plus) => {
                BigInt::from_mag(Sign::Plus, &self.magnitude + &other.magnitude)
            }
            (Sign::Minus, Sign::Minus) => {
                BigInt::from_mag(Sign::Minus, &self.magnitude + &other.magnitude)
            }
            (Sign::Plus, Sign::Minus) => match self.magnitude.cmp_mag(&other.magnitude) {
                Ordering::Greater => {
                    BigInt::from_mag(Sign::Plus, self.magnitude.sub_mag(&other.magnitude))
                }
                Ordering::Less => {
                    BigInt::from_mag(Sign::Minus, other.magnitude.sub_mag(&self.magnitude))
                }
                Ordering::Equal => BigInt::zero(),
            },
            (Sign::Minus, Sign::Plus) => match self.magnitude.cmp_mag(&other.magnitude) {
                Ordering::Greater => {
                    BigInt::from_mag(Sign::Minus, self.magnitude.sub_mag(&other.magnitude))
                }
                Ordering::Less => {
                    BigInt::from_mag(Sign::Plus, other.magnitude.sub_mag(&self.magnitude))
                }
                Ordering::Equal => BigInt::zero(),
            },
        }
    }
}

impl std::ops::Add for BigInt {
    type Output = Self;
    fn add(self, other: Self) -> Self::Output {
        &self + &other
    }
}

impl std::ops::Add<&BigInt> for BigInt {
    type Output = BigInt;
    fn add(self, other: &BigInt) -> BigInt {
        &self + other
    }
}

impl std::ops::Sub for &BigInt {
    type Output = BigInt;

    fn sub(self, other: &BigInt) -> BigInt {
        self + &-other.clone()
    }
}

impl std::ops::Sub for BigInt {
    type Output = Self;
    fn sub(self, other: Self) -> Self::Output {
        &self - &other
    }
}

impl std::ops::Sub<&BigInt> for BigInt {
    type Output = BigInt;
    fn sub(self, other: &BigInt) -> BigInt {
        &self - other
    }
}

impl std::ops::Mul for &BigInt {
    type Output = BigInt;

    fn mul(self, other: &BigInt) -> BigInt {
        if self.is_zero() || other.is_zero() {
            return BigInt::zero();
        }
        let sign = if self.sign == other.sign {
            Sign::Plus
        } else {
            Sign::Minus
        };
        BigInt::from_mag(sign, self.magnitude.mul(&other.magnitude))
    }
}

impl std::ops::Mul for BigInt {
    type Output = Self;
    fn mul(self, other: Self) -> Self::Output {
        &self * &other
    }
}

impl std::ops::MulAssign for BigInt {
    fn mul_assign(&mut self, rhs: Self) {
        *self = &*self * &rhs;
    }
}

impl std::ops::Mul<&BigInt> for BigInt {
    type Output = BigInt;
    fn mul(self, other: &BigInt) -> BigInt {
        &self * other
    }
}

impl std::ops::Div for &BigInt {
    type Output = BigInt;

    fn div(self, other: &BigInt) -> BigInt {
        self.div_rem(other).0
    }
}

impl std::ops::Div for BigInt {
    type Output = Self;
    fn div(self, other: Self) -> Self::Output {
        &self / &other
    }
}

impl std::ops::Rem for &BigInt {
    type Output = BigInt;

    fn rem(self, other: &BigInt) -> BigInt {
        self.div_rem(other).1
    }
}

impl std::ops::Rem for BigInt {
    type Output = Self;
    fn rem(self, other: Self) -> Self::Output {
        &self % &other
    }
}

impl std::ops::Add for &BigUint {
    type Output = BigUint;
    fn add(self, other: &BigUint) -> BigUint {
        self.add_mag(other)
    }
}

impl std::ops::Sub for &BigUint {
    type Output = BigUint;
    fn sub(self, other: &BigUint) -> BigUint {
        self.sub_mag(other)
    }
}

impl std::ops::Mul for &BigUint {
    type Output = BigUint;
    fn mul(self, other: &BigUint) -> BigUint {
        self.mul(other)
    }
}

#[cfg(test)]
mod bigint_tests {
    use super::*;

    #[test]
    fn factorial_50() {
        let mut acc = BigInt::from(1);
        for i in 2..=50 {
            acc *= BigInt::from(i);
        }
        assert_eq!(
            acc.to_string(),
            "30414093201713378043612608166064768844377641568960512000000000000"
        );
    }

    #[test]
    fn overflow_addition() {
        let a = BigInt::from(i64::MAX);
        let b = BigInt::from(i64::MAX);
        let sum = &a + &b;
        assert_eq!(sum.to_string(), "18446744073709551614");
    }

    #[test]
    fn division_edge_cases() {
        let a = BigInt::from_str("999999999999999999999999999999").unwrap();
        let b = BigInt::from_str("123456789012345678901234567890").unwrap();
        let q = &a / &b;
        let r = &a % &b;
        assert_eq!((&q * &b + &r).to_string(), a.to_string());
        assert!(r.abs().cmp(&b.abs()) == Ordering::Less);
    }

    #[test]
    fn pow_small() {
        assert_eq!(BigInt::from(2).pow(10).to_string(), "1024");
        assert_eq!(BigInt::from(10).pow(0).to_string(), "1");
    }

    #[test]
    fn parse_and_display() {
        let n = BigInt::from_str("-12345678901234567890").unwrap();
        assert_eq!(n.to_string(), "-12345678901234567890");
        assert_eq!(BigInt::from_str("0").unwrap(), BigInt::zero());
    }
}
