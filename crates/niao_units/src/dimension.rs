use crate::error::{UnitError, UnitResult};
use std::fmt;

/// Seven SI base dimensions: length, mass, time, current, temperature,
/// amount of substance, luminous intensity.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Dimension {
    pub l: i8,
    pub m: i8,
    pub t: i8,
    pub i: i8,
    pub th: i8,
    pub n: i8,
    pub j: i8,
}

impl Dimension {
    pub const fn dimensionless() -> Self {
        Self {
            l: 0,
            m: 0,
            t: 0,
            i: 0,
            th: 0,
            n: 0,
            j: 0,
        }
    }

    pub fn is_dimensionless(&self) -> bool {
        self.l == 0
            && self.m == 0
            && self.t == 0
            && self.i == 0
            && self.th == 0
            && self.n == 0
            && self.j == 0
    }

    pub fn compatible(&self, other: &Self) -> bool {
        self == other
    }

    pub fn mul(self, other: Self) -> Self {
        Self {
            l: self.l.saturating_add(other.l),
            m: self.m.saturating_add(other.m),
            t: self.t.saturating_add(other.t),
            i: self.i.saturating_add(other.i),
            th: self.th.saturating_add(other.th),
            n: self.n.saturating_add(other.n),
            j: self.j.saturating_add(other.j),
        }
    }

    pub fn div(self, other: Self) -> Self {
        Self {
            l: self.l.saturating_sub(other.l),
            m: self.m.saturating_sub(other.m),
            t: self.t.saturating_sub(other.t),
            i: self.i.saturating_sub(other.i),
            th: self.th.saturating_sub(other.th),
            n: self.n.saturating_sub(other.n),
            j: self.j.saturating_sub(other.j),
        }
    }

    pub fn pow(self, exp: i32) -> UnitResult<Self> {
        if exp > i8::MAX as i32 || exp < i8::MIN as i32 {
            return Err(UnitError::DimensionOverflow);
        }
        let e = exp as i8;
        Ok(Self {
            l: self.l.saturating_mul(e),
            m: self.m.saturating_mul(e),
            t: self.t.saturating_mul(e),
            i: self.i.saturating_mul(e),
            th: self.th.saturating_mul(e),
            n: self.n.saturating_mul(e),
            j: self.j.saturating_mul(e),
        })
    }

    pub fn sqrt(self) -> UnitResult<Self> {
        for (name, exp) in [
            ("length", self.l),
            ("mass", self.m),
            ("time", self.t),
            ("current", self.i),
            ("temperature", self.th),
            ("amount", self.n),
            ("luminous_intensity", self.j),
        ] {
            if exp % 2 != 0 {
                return Err(UnitError::NonIntegerRoot {
                    dimension: name.into(),
                    exponent: exp,
                });
            }
        }
        Ok(Self {
            l: self.l / 2,
            m: self.m / 2,
            t: self.t / 2,
            i: self.i / 2,
            th: self.th / 2,
            n: self.n / 2,
            j: self.j / 2,
        })
    }

    pub fn format(&self) -> String {
        if self.is_dimensionless() {
            return "dimensionless".into();
        }
        let parts = [
            ("m", self.l),
            ("kg", self.m),
            ("s", self.t),
            ("A", self.i),
            ("K", self.th),
            ("mol", self.n),
            ("cd", self.j),
        ];
        let mut num = Vec::new();
        let mut den = Vec::new();
        for (sym, exp) in parts {
            if exp > 0 {
                num.push(fmt_exp(sym, exp));
            } else if exp < 0 {
                den.push(fmt_exp(sym, -exp));
            }
        }
        if den.is_empty() {
            num.join("*")
        } else if num.is_empty() {
            format!("1/{}", den.join("*"))
        } else {
            format!("{}/{}", num.join("*"), den.join("*"))
        }
    }
}

fn fmt_exp(sym: &str, exp: i8) -> String {
    if exp == 1 {
        sym.to_string()
    } else {
        format!("{sym}^{exp}")
    }
}

impl fmt::Debug for Dimension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mul_div_roundtrip() {
        let vel = Dimension {
            l: 1,
            t: -1,
            ..Default::default()
        };
        let time = Dimension {
            t: 1,
            ..Default::default()
        };
        assert!(vel.mul(time).compatible(&Dimension {
            l: 1,
            ..Default::default()
        }));
    }

    #[test]
    fn sqrt_area_dimension() {
        let area = Dimension {
            l: 2,
            ..Default::default()
        };
        let root = area.sqrt().unwrap();
        assert_eq!(root.l, 1);
        assert_eq!(root.format(), "m");
    }
}
