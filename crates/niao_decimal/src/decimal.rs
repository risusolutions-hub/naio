//! Arbitrary-precision decimal arithmetic (coefficient × 10^exponent).

use crate::context::Context;
use crate::error::{DecimalError, DecimalResult};
use crate::fraction::Fraction;
use crate::rounding::increment_coeff;
use niao_bignum::{BigInt, Sign};
use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecimalKind {
    Finite { sign: Sign, coeff: BigInt, exp: i64 },
    NaN { signaling: bool },
    Infinity { sign: Sign },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decimal {
    kind: DecimalKind,
}

impl Decimal {
    pub fn zero() -> Self {
        Self::finite(Sign::NoSign, BigInt::zero(), 0)
    }

    pub fn one() -> Self {
        Self::finite(Sign::Plus, BigInt::from(1), 0)
    }

    pub fn nan() -> Self {
        Self {
            kind: DecimalKind::NaN { signaling: false },
        }
    }

    pub fn snan() -> Self {
        Self {
            kind: DecimalKind::NaN { signaling: true },
        }
    }

    pub fn infinity(sign: Sign) -> Self {
        Self {
            kind: DecimalKind::Infinity { sign },
        }
    }

    pub fn from_coeff_exp(sign: Sign, coeff: BigInt, exp: i64) -> Self {
        Self::finite(sign, coeff, exp)
    }

    pub(crate) fn finite(sign: Sign, coeff: BigInt, exp: i64) -> Self {
        if coeff.is_zero() {
            Self {
                kind: DecimalKind::Finite {
                    sign: Sign::NoSign,
                    coeff: BigInt::zero(),
                    exp: 0,
                },
            }
        } else {
            let sign = if sign == Sign::Minus {
                Sign::Minus
            } else {
                Sign::Plus
            };
            Self {
                kind: DecimalKind::Finite { sign, coeff, exp },
            }
        }
    }

    pub fn is_finite(&self) -> bool {
        matches!(self.kind, DecimalKind::Finite { .. })
    }

    pub fn is_nan(&self) -> bool {
        matches!(self.kind, DecimalKind::NaN { .. })
    }

    pub fn is_infinite(&self) -> bool {
        matches!(self.kind, DecimalKind::Infinity { .. })
    }

    pub fn is_zero(&self) -> bool {
        match &self.kind {
            DecimalKind::Finite { coeff, .. } => coeff.is_zero(),
            _ => false,
        }
    }

    pub fn sign(&self) -> Sign {
        match &self.kind {
            DecimalKind::Finite { sign, .. } => *sign,
            DecimalKind::NaN { .. } => Sign::NoSign,
            DecimalKind::Infinity { sign } => *sign,
        }
    }

    pub fn coeff(&self) -> Option<&BigInt> {
        match &self.kind {
            DecimalKind::Finite { coeff, .. } => Some(coeff),
            _ => None,
        }
    }

    pub fn exp(&self) -> Option<i64> {
        match &self.kind {
            DecimalKind::Finite { exp, .. } => Some(*exp),
            _ => None,
        }
    }

    pub fn as_tuple(&self) -> Option<(Sign, BigInt, i64)> {
        match &self.kind {
            DecimalKind::Finite { sign, coeff, exp } => Some((*sign, coeff.clone(), *exp)),
            _ => None,
        }
    }

    pub fn normalize(&self) -> Self {
        match &self.kind {
            DecimalKind::Finite { sign, coeff, exp } => {
                if coeff.is_zero() {
                    return Self::zero();
                }
                let s = coeff.to_string();
                let trailing = s.chars().rev().take_while(|c| *c == '0').count();
                if trailing == 0 {
                    return self.clone();
                }
                let trim = s.len() - trailing;
                let new_coeff = BigInt::from_str(&s[..trim]).unwrap_or_else(|_| coeff.clone());
                Self::finite(*sign, new_coeff, exp + trailing as i64)
            }
            _ => self.clone(),
        }
    }

    pub fn abs(&self) -> Self {
        match &self.kind {
            DecimalKind::Finite { coeff, exp, .. } => Self::finite(Sign::Plus, coeff.clone(), *exp),
            DecimalKind::Infinity { .. } => Self::infinity(Sign::Plus),
            _ => self.clone(),
        }
    }

    pub fn neg(&self) -> Self {
        match &self.kind {
            DecimalKind::Finite { sign, coeff, exp } => {
                let new_sign = if coeff.is_zero() {
                    Sign::NoSign
                } else if *sign == Sign::Minus {
                    Sign::Plus
                } else {
                    Sign::Minus
                };
                Self::finite(new_sign, coeff.clone(), *exp)
            }
            DecimalKind::Infinity { sign } => Self::infinity(sign.flip()),
            _ => self.clone(),
        }
    }

    pub fn copy_sign(&self, other: &Self) -> Self {
        if !self.is_finite() || !other.is_finite() {
            return self.clone();
        }
        let mag = self.abs();
        match other.sign() {
            Sign::Minus => mag.neg(),
            _ => mag,
        }
    }

    pub fn compare(&self, other: &Self) -> Option<Ordering> {
        if self.is_nan() || other.is_nan() {
            return None;
        }
        if self.is_infinite() || other.is_infinite() {
            return Some(self.total_cmp(other));
        }
        let (s1, c1, e1) = self.as_tuple().unwrap();
        let (s2, c2, e2) = other.as_tuple().unwrap();
        if s1 != s2 && !c1.is_zero() && !c2.is_zero() {
            return Some(if s1 == Sign::Minus {
                Ordering::Less
            } else {
                Ordering::Greater
            });
        }
        let aligned = align_coeffs(c1, e1, c2, e2);
        Some(
            aligned
                .0
                .cmp(&aligned.2)
                .then_with(|| aligned.1.cmp(&aligned.3)),
        )
    }

    fn total_cmp(&self, other: &Self) -> Ordering {
        use DecimalKind::*;
        match (&self.kind, &other.kind) {
            (Finite { .. }, Finite { .. }) => self.compare(other).unwrap_or(Ordering::Equal),
            (NaN { .. }, NaN { .. }) => Ordering::Equal,
            (NaN { .. }, _) => Ordering::Greater,
            (_, NaN { .. }) => Ordering::Less,
            (Infinity { sign: a }, Infinity { sign: b }) => match (a, b) {
                (Sign::Minus, Sign::Plus) => Ordering::Less,
                (Sign::Plus, Sign::Minus) => Ordering::Greater,
                _ => Ordering::Equal,
            },
            (Infinity { sign }, Finite { .. }) => {
                if *sign == Sign::Minus {
                    Ordering::Less
                } else {
                    Ordering::Greater
                }
            }
            (Finite { .. }, Infinity { sign }) => {
                if *sign == Sign::Minus {
                    Ordering::Greater
                } else {
                    Ordering::Less
                }
            }
        }
    }

    pub fn add(&self, other: &Self, ctx: &Context) -> DecimalResult<Self> {
        binary_op(self, other, ctx, |a, b, c| a.add_aligned(b, c))
    }

    pub fn sub(&self, other: &Self, ctx: &Context) -> DecimalResult<Self> {
        binary_op(self, other, ctx, |a, b, c| a.sub_aligned(b, c))
    }

    pub fn mul(&self, other: &Self, ctx: &Context) -> DecimalResult<Self> {
        binary_op(self, other, ctx, |a, b, c| a.mul_aligned(b, c))
    }

    pub fn div(&self, other: &Self, ctx: &Context) -> DecimalResult<Self> {
        if other.is_zero() {
            return Err(DecimalError::DivisionByZero);
        }
        binary_op(self, other, ctx, |a, b, c| a.div_aligned(b, c))
    }

    pub fn rem(&self, other: &Self, ctx: &Context) -> DecimalResult<Self> {
        let quot = self.div(other, ctx)?;
        let prod = quot.mul(other, ctx)?;
        self.sub(&prod, ctx)
    }

    pub fn divmod(&self, other: &Self, ctx: &Context) -> DecimalResult<(Self, Self)> {
        let q = self.div(other, ctx)?;
        let r = self.rem(other, ctx)?;
        Ok((q, r))
    }

    pub fn pow(&self, exp: i64, ctx: &Context) -> DecimalResult<Self> {
        if !self.is_finite() {
            return Err(DecimalError::InvalidOperation(
                "pow on non-finite decimal".into(),
            ));
        }
        if exp == 0 {
            return Ok(Self::one());
        }
        if self.is_zero() {
            return if exp > 0 {
                Ok(self.clone())
            } else {
                Err(DecimalError::DivisionByZero)
            };
        }
        let mut base = self.clone();
        let mut result = Self::one();
        let mut e = exp.unsigned_abs();
        while e > 0 {
            if e & 1 == 1 {
                result = result.mul(&base, ctx)?;
            }
            base = base.mul(&base, ctx)?;
            e >>= 1;
        }
        if exp < 0 {
            Ok(Self::one().div(&result, ctx)?)
        } else {
            Ok(result)
        }
    }

    pub fn quantize(&self, exp: i64, ctx: &Context) -> DecimalResult<Self> {
        if !self.is_finite() {
            return Err(DecimalError::InvalidOperation(
                "quantize on non-finite decimal".into(),
            ));
        }
        let (sign, coeff, self_exp) = self.as_tuple().unwrap();
        if self_exp == exp {
            return Ok(self.clone());
        }
        let shift = self_exp - exp;
        let mut coeff = coeff;
        if shift > 0 {
            coeff = coeff * pow10_bigint(shift as u32);
        } else if shift < 0 {
            let pow10 = pow10_bigint((-shift) as u32);
            let (q, r) = coeff.div_rem(&pow10);
            let rem_digit = if r.is_zero() {
                0u8
            } else {
                let rs = r.to_string();
                let first = rs.chars().next().unwrap_or('0');
                first as u8 - b'0'
            };
            coeff = q;
            increment_coeff(&mut coeff, ctx.rounding, sign, rem_digit);
        }
        let mut out = Self::finite(sign, coeff, exp);
        out = out.apply_context(ctx)?;
        Ok(out)
    }

    pub fn rescale(&self, exp: i64, ctx: &Context) -> DecimalResult<Self> {
        self.quantize(exp, ctx)
    }

    pub fn to_integral(&self, ctx: &Context) -> DecimalResult<Self> {
        let (sign, coeff, exp) = match self.as_tuple() {
            Some(t) => t,
            None => return Err(DecimalError::InvalidOperation("non-finite".into())),
        };
        if exp >= 0 {
            return Ok(self.clone());
        }
        let mut c = coeff;
        let pow10 = pow10_bigint((-exp) as u32);
        let (q, r) = c.div_rem(&pow10);
        let rem_digit = if r.is_zero() {
            0u8
        } else {
            let rs = r.to_string();
            rs.chars().next().unwrap_or('0') as u8 - b'0'
        };
        c = q;
        increment_coeff(&mut c, ctx.rounding, sign, rem_digit);
        Ok(Self::finite(sign, c, 0))
    }

    pub fn sqrt(&self, ctx: &Context) -> DecimalResult<Self> {
        if !self.is_finite() || self.is_zero() {
            return Ok(self.clone());
        }
        if self.sign() == Sign::Minus {
            return Err(DecimalError::InvalidOperation("sqrt of negative".into()));
        }
        let prec = ctx.prec.max(1) as i64;
        let target_exp = -prec;
        let mut x = self.clone();
        x = x.rescale(target_exp, ctx)?;
        // Newton: x_{n+1} = (x_n + a/x_n)/2
        let two = Self::finite(Sign::Plus, BigInt::from(2), 0);
        let mut guess = Self::finite(Sign::Plus, BigInt::from(1), target_exp / 2);
        for _ in 0..(prec as usize + 8) {
            let next = guess.add(&self.div(&guess, ctx)?, ctx)?.div(&two, ctx)?;
            if next.compare(&guess) == Some(Ordering::Equal) {
                guess = next;
                break;
            }
            guess = next;
        }
        guess.quantize(target_exp, ctx)
    }

    pub fn from_fraction(frac: &Fraction, ctx: &Context) -> DecimalResult<Self> {
        let numer = frac.numer().clone();
        let denom = frac.denom().clone();
        let n = Self::finite(
            if numer < BigInt::from(0) {
                Sign::Minus
            } else {
                Sign::Plus
            },
            numer.abs(),
            0,
        );
        let d = Self::finite(Sign::Plus, denom, 0);
        n.div(&d, ctx)
    }

    pub fn from_i64(v: i64) -> Self {
        if v == 0 {
            Self::zero()
        } else if v > 0 {
            Self::finite(Sign::Plus, BigInt::from(v), 0)
        } else {
            Self::finite(Sign::Minus, BigInt::from(-v), 0)
        }
    }

    pub fn from_f64_repr(v: f64) -> DecimalResult<Self> {
        if v.is_nan() {
            return Ok(Self::nan());
        }
        if v.is_infinite() {
            return Ok(Self::infinity(if v.is_sign_negative() {
                Sign::Minus
            } else {
                Sign::Plus
            }));
        }
        let s = format!("{v:.17e}");
        parse_decimal(&s)
    }

    fn add_aligned(&self, other: &Self, ctx: &Context) -> DecimalResult<Self> {
        let (s1, c1, e1) = self.as_tuple().unwrap();
        let (s2, c2, e2) = other.as_tuple().unwrap();
        let (ac1, ae, ac2, _) = align_coeffs(c1, e1, c2, e2);
        let sum = if s1 == s2 {
            &ac1 + &ac2
        } else if ac1 >= ac2 {
            &ac1 - &ac2
        } else {
            &ac2 - &ac1
        };
        let sign = if sum.is_zero() {
            Sign::NoSign
        } else if s1 == s2 {
            s1
        } else if ac1 >= ac2 {
            s1
        } else {
            s2
        };
        let mut out = Self::finite(sign, sum.abs(), ae);
        out = out.apply_context(ctx)?;
        Ok(out)
    }

    fn sub_aligned(&self, other: &Self, ctx: &Context) -> DecimalResult<Self> {
        self.add_aligned(&other.neg(), ctx)
    }

    fn mul_aligned(&self, other: &Self, ctx: &Context) -> DecimalResult<Self> {
        let (s1, c1, e1) = self.as_tuple().unwrap();
        let (s2, c2, e2) = other.as_tuple().unwrap();
        let sign = if s1 == s2 || s1 == Sign::NoSign || s2 == Sign::NoSign {
            Sign::Plus
        } else {
            Sign::Minus
        };
        let coeff = &c1 * &c2;
        let exp = e1 + e2;
        let mut out = Self::finite(sign, coeff, exp);
        out = out.apply_context(ctx)?;
        Ok(out)
    }

    fn div_aligned(&self, other: &Self, ctx: &Context) -> DecimalResult<Self> {
        let (s1, c1, e1) = self.as_tuple().unwrap();
        let (s2, c2, e2) = other.as_tuple().unwrap();
        if c2.is_zero() {
            return Err(DecimalError::DivisionByZero);
        }
        let sign = if s1 == s2 || s1 == Sign::NoSign || s2 == Sign::NoSign {
            Sign::Plus
        } else {
            Sign::Minus
        };
        let extra = ctx.prec as i32 + 4;
        let target_exp = e1 - e2 - extra as i64;
        let scaled = &c1 * &pow10_bigint((ctx.prec + 4) as u32);
        let (q, r) = scaled.div_rem(&c2);
        let mut coeff = q;
        let rem_digit = if r.is_zero() {
            0u8
        } else {
            let rs = r.to_string();
            rs.chars().next().unwrap_or('0') as u8 - b'0'
        };
        increment_coeff(&mut coeff, ctx.rounding, sign, rem_digit);
        let mut out = Self::finite(sign, coeff, target_exp);
        out = out.apply_context(ctx)?;
        Ok(out)
    }

    fn apply_context(&self, ctx: &Context) -> DecimalResult<Self> {
        let (sign, coeff, exp) = match self.as_tuple() {
            Some(t) => t,
            None => return Ok(self.clone()),
        };
        if coeff.is_zero() {
            return Ok(Self::zero());
        }
        let digits = coeff.to_string().len() as u32;
        if digits <= ctx.prec {
            return Ok(self.clone());
        }
        let drop = digits - ctx.prec;
        let pow10 = pow10_bigint(drop);
        let (q, r) = coeff.div_rem(&pow10);
        let mut new_coeff = q;
        let rem_digit = if r.is_zero() {
            0u8
        } else {
            let rs = r.to_string();
            rs.chars().next().unwrap_or('0') as u8 - b'0'
        };
        increment_coeff(&mut new_coeff, ctx.rounding, sign, rem_digit);
        let new_exp = exp + drop as i64;
        if new_exp > ctx.emax as i64 {
            return Err(DecimalError::Overflow);
        }
        if new_exp < ctx.emin as i64 {
            return Err(DecimalError::Underflow);
        }
        Ok(Self::finite(sign, new_coeff, new_exp))
    }

    pub fn to_sci_string(&self) -> String {
        match &self.kind {
            DecimalKind::NaN { .. } => "NaN".into(),
            DecimalKind::Infinity { sign } => {
                if *sign == Sign::Minus {
                    "-Infinity".into()
                } else {
                    "Infinity".into()
                }
            }
            DecimalKind::Finite { sign, coeff, exp } => format_finite(sign, coeff, *exp, true),
        }
    }

    pub fn to_eng_string(&self) -> String {
        match &self.kind {
            DecimalKind::NaN { .. } => "NaN".into(),
            DecimalKind::Infinity { sign } => {
                if *sign == Sign::Minus {
                    "-Infinity".into()
                } else {
                    "Infinity".into()
                }
            }
            DecimalKind::Finite { sign, coeff, exp } => {
                let mut e = *exp;
                while e % 3 != 0 {
                    e -= 1;
                }
                format_finite(sign, coeff, e, true)
            }
        }
    }
}

impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            DecimalKind::NaN { .. } => f.write_str("NaN"),
            DecimalKind::Infinity { sign } => {
                if *sign == Sign::Minus {
                    f.write_str("-Infinity")
                } else {
                    f.write_str("Infinity")
                }
            }
            DecimalKind::Finite { sign, coeff, exp } => {
                f.write_str(&format_finite(sign, coeff, *exp, false))
            }
        }
    }
}

impl FromStr for Decimal {
    type Err = DecimalError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_decimal(s)
    }
}

pub fn parse_decimal(s: &str) -> DecimalResult<Decimal> {
    let s = s.trim();
    if s.is_empty() {
        return Err(DecimalError::Parse("empty decimal".into()));
    }
    let lower = s.to_ascii_lowercase();
    if lower == "nan" {
        return Ok(Decimal::nan());
    }
    if lower == "snan" {
        return Ok(Decimal::snan());
    }
    if lower == "inf" || lower == "infinity" {
        return Ok(Decimal::infinity(Sign::Plus));
    }
    if lower == "-inf" || lower == "-infinity" {
        return Ok(Decimal::infinity(Sign::Minus));
    }

    let mut sign = Sign::Plus;
    let mut i = 0usize;
    let bytes = s.as_bytes();
    if bytes[i] == b'+' {
        i += 1;
    } else if bytes[i] == b'-' {
        sign = Sign::Minus;
        i += 1;
    }

    let mut int_part = String::new();
    let mut frac_part = String::new();
    let mut saw_dot = false;
    let mut saw_digit = false;

    while i < bytes.len() {
        let c = bytes[i];
        if c == b'_' {
            i += 1;
            continue;
        }
        if c == b'.' {
            if saw_dot {
                return Err(DecimalError::Parse("multiple decimal points".into()));
            }
            saw_dot = true;
            i += 1;
            continue;
        }
        if c == b'e' || c == b'E' {
            break;
        }
        if !c.is_ascii_digit() {
            return Err(DecimalError::Parse(format!(
                "invalid character '{}'",
                c as char
            )));
        }
        saw_digit = true;
        if saw_dot {
            frac_part.push(c as char);
        } else {
            int_part.push(c as char);
        }
        i += 1;
    }

    if !saw_digit {
        return Err(DecimalError::Parse("no digits".into()));
    }

    let mut exp_adjust: i64 = 0;
    if i < bytes.len() {
        let rest = &s[i..];
        let exp_s = rest.trim_start_matches(['e', 'E']).trim();
        exp_adjust = exp_s
            .parse::<i64>()
            .map_err(|_| DecimalError::Parse(format!("invalid exponent '{exp_s}'")))?;
    }

    if int_part.is_empty() {
        int_part.push('0');
    }
    let digits = format!("{}{}", int_part, frac_part);
    let digits = digits.trim_start_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let coeff =
        BigInt::from_str(digits).map_err(|_| DecimalError::Parse("invalid coefficient".into()))?;
    let exp = exp_adjust - frac_part.len() as i64;
    if coeff.is_zero() {
        Ok(Decimal::zero())
    } else {
        Ok(Decimal::finite(sign, coeff, exp))
    }
}

fn align_coeffs(c1: BigInt, e1: i64, c2: BigInt, e2: i64) -> (BigInt, i64, BigInt, i64) {
    let common = e1.min(e2);
    let ac1 = if e1 > common {
        c1 * pow10_bigint((e1 - common) as u32)
    } else {
        c1
    };
    let ac2 = if e2 > common {
        c2 * pow10_bigint((e2 - common) as u32)
    } else {
        c2
    };
    (ac1, common, ac2, common)
}

fn pow10_bigint(n: u32) -> BigInt {
    let mut v = BigInt::from(1u32);
    for _ in 0..n {
        v = &v * &BigInt::from(10u32);
    }
    v
}

fn format_finite(sign: &Sign, coeff: &BigInt, exp: i64, sci: bool) -> String {
    let mut s = coeff.to_string();
    let negative = *sign == Sign::Minus;
    if exp >= 0 {
        if exp as usize >= s.len().saturating_sub(1) {
            s.push_str(&"0".repeat(exp as usize - s.len() + 1));
        } else {
            let split = s.len() as i64 - exp;
            let idx = split as usize;
            if idx > 0 && idx < s.len() {
                s.insert(idx, '.');
            }
        }
        if negative && !coeff.is_zero() {
            format!("-{s}")
        } else {
            s
        }
    } else if sci {
        let mut out = if negative { "-" } else { "" }.to_string();
        out.push_str(&s[..1.min(s.len())]);
        if s.len() > 1 {
            out.push('.');
            out.push_str(&s[1..]);
        }
        out.push('E');
        out.push_str(&(exp + (s.len() as i64 - 1)).to_string());
        out
    } else {
        let places = (-exp) as usize;
        let mut out = if negative { "-" } else { "" }.to_string();
        if places >= s.len() {
            out.push_str("0.");
            out.push_str(&"0".repeat(places - s.len()));
            out.push_str(&s);
        } else {
            let idx = s.len() - places;
            out.push_str(&s[..idx]);
            out.push('.');
            out.push_str(&s[idx..]);
        }
        out
    }
}

fn binary_op<F>(a: &Decimal, b: &Decimal, ctx: &Context, f: F) -> DecimalResult<Decimal>
where
    F: FnOnce(&Decimal, &Decimal, &Context) -> DecimalResult<Decimal>,
{
    if a.is_nan() || b.is_nan() {
        return Ok(Decimal::nan());
    }
    if a.is_infinite() || b.is_infinite() {
        return Err(DecimalError::InvalidOperation(
            "operation on infinity".into(),
        ));
    }
    f(a, b, ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Context;
    use crate::RoundingMode;

    #[test]
    fn add_twice() {
        let ctx = Context::default();
        let a = parse_decimal("21.64").unwrap();
        let mut acc = Decimal::zero();
        acc = acc.add(&a, &ctx).unwrap();
        acc = acc.add(&a, &ctx).unwrap();
        assert_eq!(acc.to_string(), "43.28");
    }

    #[test]
    fn add_from_zero() {
        let ctx = Context::default();
        let z = Decimal::zero();
        let a = parse_decimal("21.64").unwrap();
        let s = z.add(&a, &ctx).unwrap();
        assert_eq!(s.to_string(), "21.64");
    }

    #[test]
    fn parse_and_add() {
        let a = parse_decimal("1.10").unwrap();
        let b = parse_decimal("2.30").unwrap();
        let ctx = Context::default();
        let c = a.add(&b, &ctx).unwrap();
        assert_eq!(c.to_string(), "3.40");
    }

    #[test]
    fn quantize_money() {
        let d = parse_decimal("2.675").unwrap();
        let ctx = Context::money();
        let q = d.quantize(-2, &ctx).unwrap();
        assert_eq!(q.to_string(), "2.68");
    }

    #[test]
    fn fraction_conversion() {
        let f = Fraction::from_raw(BigInt::from(1), BigInt::from(3));
        let ctx = Context::new(10, RoundingMode::HalfEven);
        let d = Decimal::from_fraction(&f, &ctx).unwrap();
        assert!(d.to_string().starts_with("0.333"));
    }
}
