use crate::dimension::Dimension;
use crate::error::{UnitError, UnitResult};
use crate::unit::Unit;

#[derive(Clone, Debug, PartialEq)]
pub struct Quantity {
    /// Magnitude expressed in SI base units for `unit.dimension`.
    pub base: f64,
    pub unit: Unit,
}

impl Quantity {
    pub fn new(magnitude: f64, unit: Unit) -> Self {
        let base = unit.to_base_magnitude(magnitude);
        Self { base, unit }
    }

    pub fn dimensionless(value: f64) -> Self {
        Self {
            base: value,
            unit: Unit::dimensionless(),
        }
    }

    pub fn magnitude(&self) -> f64 {
        self.unit.from_base_magnitude(self.base)
    }

    pub fn dimension(&self) -> Dimension {
        self.unit.dimension
    }

    pub fn is_dimensionless(&self) -> bool {
        self.unit.is_dimensionless()
    }

    pub fn compatible(&self, other: &Self) -> bool {
        self.unit.compatible(&other.unit)
    }

    pub fn to_unit(&self, target: &Unit) -> UnitResult<Self> {
        if !self.unit.compatible(target) {
            return Err(UnitError::DimensionMismatch {
                left: self.unit.dimension.format(),
                right: target.dimension.format(),
            });
        }
        Ok(Self {
            base: self.base,
            unit: target.clone(),
        })
    }

    pub fn add(&self, other: &Self) -> UnitResult<Self> {
        if !self.compatible(other) {
            return Err(UnitError::DimensionMismatch {
                left: self.unit.dimension.format(),
                right: other.unit.dimension.format(),
            });
        }
        Ok(Self {
            base: self.base + other.base,
            unit: self.unit.clone(),
        })
    }

    pub fn sub(&self, other: &Self) -> UnitResult<Self> {
        if !self.compatible(other) {
            return Err(UnitError::DimensionMismatch {
                left: self.unit.dimension.format(),
                right: other.unit.dimension.format(),
            });
        }
        Ok(Self {
            base: self.base - other.base,
            unit: self.unit.clone(),
        })
    }

    pub fn mul(&self, other: &Self) -> UnitResult<Self> {
        let unit = self.unit.mul(&other.unit)?;
        let base = self.base * other.base;
        Ok(Self { base, unit })
    }

    pub fn div(&self, other: &Self) -> UnitResult<Self> {
        if other.base == 0.0 {
            return Err(UnitError::DivisionByZero);
        }
        let unit = self.unit.div(&other.unit)?;
        let base = self.base / other.base;
        Ok(Self { base, unit })
    }

    pub fn scale(&self, factor: f64) -> Self {
        Self {
            base: self.base * factor,
            unit: self.unit.clone(),
        }
    }

    pub fn neg(&self) -> Self {
        self.scale(-1.0)
    }

    pub fn abs(&self) -> Self {
        Self {
            base: self.base.abs(),
            unit: self.unit.clone(),
        }
    }

    pub fn pow(&self, exp: i32) -> UnitResult<Self> {
        let unit = self.unit.pow(exp)?;
        let base = self.base.powi(exp);
        Ok(Self { base, unit })
    }

    pub fn sqrt(&self) -> UnitResult<Self> {
        if self.base < 0.0 {
            return Err(UnitError::Parse("cannot sqrt negative quantity".into()));
        }
        let unit = self.unit.sqrt()?;
        Ok(Self {
            base: self.base.sqrt(),
            unit,
        })
    }

    pub fn compare(&self, other: &Self) -> UnitResult<std::cmp::Ordering> {
        if !self.compatible(other) {
            return Err(UnitError::DimensionMismatch {
                left: self.unit.dimension.format(),
                right: other.unit.dimension.format(),
            });
        }
        Ok(self
            .base
            .partial_cmp(&other.base)
            .unwrap_or(std::cmp::Ordering::Equal))
    }

    pub fn format(&self, precision: Option<usize>) -> String {
        let mag = self.magnitude();
        let mag_s = match precision {
            Some(p) => format!("{mag:.prec$}", prec = p),
            None => trim_float(mag),
        };
        if self.unit.is_dimensionless() {
            mag_s
        } else if self.unit.symbol.is_empty() {
            format!("{mag_s} {}", self.unit.dimension.format())
        } else {
            format!("{mag_s} {}", self.unit.symbol)
        }
    }
}

fn trim_float(v: f64) -> String {
    let s = format!("{v:.12}");
    let s = s.trim_end_matches('0');
    let s = s.trim_end_matches('.');
    if s == "-0" {
        "0".into()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unit::Affine;

    #[test]
    fn add_lengths() {
        let m = Unit {
            dimension: Dimension {
                l: 1,
                ..Default::default()
            },
            scale: 1.0,
            affine: Affine::MULTIPLICATIVE,
            symbol: "m".into(),
        };
        let a = Quantity::new(5.0, m.clone());
        let b = Quantity::new(3.0, m);
        let c = a.add(&b).unwrap();
        assert!((c.magnitude() - 8.0).abs() < 1e-12);
    }
}
