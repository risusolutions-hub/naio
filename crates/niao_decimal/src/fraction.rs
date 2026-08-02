//! Exact rational numbers as reduced numerator/denominator pairs.

use crate::error::{DecimalError, DecimalResult};
use niao_bignum::BigInt;
use std::fmt;
use std::str::FromStr;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fraction {
    numer: BigInt,
    denom: BigInt,
}

impl Fraction {
    pub fn new(numer: BigInt, denom: BigInt) -> DecimalResult<Self> {
        if denom.is_zero() {
            return Err(DecimalError::DivisionByZero);
        }
        Ok(Self::from_raw(numer, denom))
    }

    pub fn from_raw(mut numer: BigInt, mut denom: BigInt) -> Self {
        if denom < BigInt::from(0) {
            numer = -&numer;
            denom = -&denom;
        }
        if numer.is_zero() {
            return Self {
                numer: BigInt::from(0),
                denom: BigInt::from(1),
            };
        }
        let g = gcd_bigint(&numer.abs(), &denom);
        Self {
            numer: &numer / &g,
            denom: &denom / &g,
        }
    }

    pub fn zero() -> Self {
        Self {
            numer: BigInt::from(0),
            denom: BigInt::from(1),
        }
    }

    pub fn one() -> Self {
        Self {
            numer: BigInt::from(1),
            denom: BigInt::from(1),
        }
    }

    pub fn numer(&self) -> &BigInt {
        &self.numer
    }

    pub fn denom(&self) -> &BigInt {
        &self.denom
    }

    pub fn is_zero(&self) -> bool {
        self.numer.is_zero()
    }

    pub fn abs(&self) -> Self {
        Self::from_raw(self.numer.abs(), self.denom.clone())
    }

    pub fn neg(&self) -> Self {
        Self::from_raw(-&self.numer, self.denom.clone())
    }

    pub fn add(&self, other: &Self) -> DecimalResult<Self> {
        let numer = &self.numer * &other.denom + &other.numer * &self.denom;
        let denom = &self.denom * &other.denom;
        Ok(Self::from_raw(numer, denom))
    }

    pub fn sub(&self, other: &Self) -> DecimalResult<Self> {
        let numer = &self.numer * &other.denom - &other.numer * &self.denom;
        let denom = &self.denom * &other.denom;
        Ok(Self::from_raw(numer, denom))
    }

    pub fn mul(&self, other: &Self) -> DecimalResult<Self> {
        Ok(Self::from_raw(
            &self.numer * &other.numer,
            &self.denom * &other.denom,
        ))
    }

    pub fn div(&self, other: &Self) -> DecimalResult<Self> {
        if other.numer.is_zero() {
            return Err(DecimalError::DivisionByZero);
        }
        Ok(Self::from_raw(
            &self.numer * &other.denom,
            &self.denom * &other.numer,
        ))
    }

    pub fn pow(&self, exp: i64) -> DecimalResult<Self> {
        if exp == 0 {
            return Ok(Self::one());
        }
        if self.numer.is_zero() {
            return Ok(Self::zero());
        }
        let exp_u = exp.unsigned_abs();
        let (n, d) = if exp > 0 {
            (self.numer.pow(exp_u as u32), self.denom.pow(exp_u as u32))
        } else {
            (self.denom.pow(exp_u as u32), self.numer.pow(exp_u as u32))
        };
        Ok(Self::from_raw(n, d))
    }

    pub fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let left = &self.numer * &other.denom;
        let right = &other.numer * &self.denom;
        left.cmp(&right)
    }

    pub fn limit_denominator(&self, max_denominator: &BigInt) -> Self {
        if max_denominator <= &BigInt::from(0) {
            return self.clone();
        }
        if self.denom <= *max_denominator {
            return self.clone();
        }
        let mut n0 = BigInt::from(0);
        let mut d0 = BigInt::from(1);
        let mut n1 = BigInt::from(1);
        let mut d1 = BigInt::from(0);
        let mut n = self.numer.abs();
        let mut d = self.denom.clone();
        loop {
            if d.is_zero() {
                break;
            }
            let (q, r) = n.div_rem(&d);
            let n2 = &n0 + &(&q * &n1);
            let d2 = &d0 + &(&q * &d1);
            if d2 > *max_denominator {
                if d1.is_zero() {
                    let out = Self::from_raw(q, BigInt::from(1));
                    return if self.numer < BigInt::from(0) {
                        out.neg()
                    } else {
                        out
                    };
                }
                if d0.is_zero() {
                    let out = Self::from_raw(BigInt::from(1), d1);
                    return if self.numer < BigInt::from(0) {
                        out.neg()
                    } else {
                        out
                    };
                }
                let num = max_denominator - &d0;
                let k = &num / &d1;
                let bound1 = Self::from_raw(&n0 + &(&k * &n1), &d0 + &(&k * &d1));
                let num = max_denominator - &d1;
                let k = &num / &d0;
                let bound2 = Self::from_raw(&n1 + &(&k * &n0), &d1 + &(&k * &d0));
                let out = if bound2.cmp(&bound1) != std::cmp::Ordering::Greater {
                    bound2
                } else {
                    bound1
                };
                return if self.numer < BigInt::from(0) {
                    out.neg()
                } else {
                    out
                };
            }
            n0 = n1;
            d0 = d1;
            n1 = n2;
            d1 = d2;
            n = d;
            d = r;
        }
        let out = Self::from_raw(n1, d1);
        if self.numer < BigInt::from(0) {
            out.neg()
        } else {
            out
        }
    }

    pub fn to_f64(&self) -> f64 {
        let n = self.numer.to_string().parse::<f64>().unwrap_or(0.0);
        let d = self.denom.to_string().parse::<f64>().unwrap_or(1.0);
        n / d
    }
}

impl fmt::Display for Fraction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.numer, self.denom)
    }
}

impl FromStr for Fraction {
    type Err = DecimalError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_fraction(s)
    }
}

pub fn parse_fraction(s: &str) -> DecimalResult<Fraction> {
    let s = s.trim();
    if s.is_empty() {
        return Err(DecimalError::Parse("empty fraction".into()));
    }
    if let Some((a, b)) = s.split_once('/') {
        let numer = BigInt::from_str(a.trim())
            .map_err(|_| DecimalError::Parse(format!("invalid numerator '{a}'")))?;
        let denom = BigInt::from_str(b.trim())
            .map_err(|_| DecimalError::Parse(format!("invalid denominator '{b}'")))?;
        Fraction::new(numer, denom)
    } else {
        let numer = BigInt::from_str(s)
            .map_err(|_| DecimalError::Parse(format!("invalid fraction '{s}'")))?;
        Ok(Fraction::from_raw(numer, BigInt::from(1)))
    }
}

fn gcd_bigint(a: &BigInt, b: &BigInt) -> BigInt {
    let mut x = a.clone();
    let mut y = b.clone();
    while !y.is_zero() {
        let (_, r) = x.div_rem(&y);
        x = y;
        y = r;
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduces_and_adds() {
        let a = Fraction::from_raw(BigInt::from(1), BigInt::from(2));
        let b = Fraction::from_raw(BigInt::from(1), BigInt::from(3));
        let c = a.add(&b).unwrap();
        assert_eq!(c.to_string(), "5/6");
    }

    #[test]
    fn limit_denominator() {
        let f = Fraction::from_raw(BigInt::from(1), BigInt::from(3));
        let l = f.limit_denominator(&BigInt::from(4));
        assert_eq!(l.denom().to_string(), "3");
    }
}
