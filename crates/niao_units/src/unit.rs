use crate::dimension::Dimension;
use crate::error::{UnitError, UnitResult};

/// Affine transform for non-multiplicative units (temperature).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    /// `base = scale * (magnitude + offset)`
    pub scale: f64,
    pub offset: f64,
}

impl Affine {
    pub const MULTIPLICATIVE: Self = Self {
        scale: 1.0,
        offset: 0.0,
    };

    pub fn to_base(&self, magnitude: f64) -> f64 {
        self.scale * (magnitude + self.offset)
    }

    pub fn from_base(&self, base: f64) -> f64 {
        base / self.scale - self.offset
    }

    pub fn is_multiplicative(&self) -> bool {
        self.offset == 0.0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Unit {
    pub dimension: Dimension,
    /// Multiplicative scale to SI base for this dimension product.
    pub scale: f64,
    pub affine: Affine,
    /// Canonical display symbol when known.
    pub symbol: String,
}

impl Unit {
    pub fn dimensionless() -> Self {
        Self {
            dimension: Dimension::dimensionless(),
            scale: 1.0,
            affine: Affine::MULTIPLICATIVE,
            symbol: "".into(),
        }
    }

    pub fn is_dimensionless(&self) -> bool {
        self.dimension.is_dimensionless()
    }

    pub fn compatible(&self, other: &Self) -> bool {
        self.dimension.compatible(&other.dimension)
    }

    pub fn mul(&self, other: &Self) -> UnitResult<Self> {
        if !self.affine.is_multiplicative() || !other.affine.is_multiplicative() {
            return Err(UnitError::Parse(
                "affine units cannot be multiplied or divided".into(),
            ));
        }
        Ok(Self {
            dimension: self.dimension.mul(other.dimension),
            scale: self.scale * other.scale,
            affine: Affine::MULTIPLICATIVE,
            symbol: format!("{}*{}", self.symbol, other.symbol),
        })
    }

    pub fn div(&self, other: &Self) -> UnitResult<Self> {
        if !self.affine.is_multiplicative() || !other.affine.is_multiplicative() {
            return Err(UnitError::Parse(
                "affine units cannot be multiplied or divided".into(),
            ));
        }
        if other.scale == 0.0 {
            return Err(UnitError::DivisionByZero);
        }
        Ok(Self {
            dimension: self.dimension.div(other.dimension),
            scale: self.scale / other.scale,
            affine: Affine::MULTIPLICATIVE,
            symbol: format!("{}/{}", self.symbol, other.symbol),
        })
    }

    pub fn pow(&self, exp: i32) -> UnitResult<Self> {
        if !self.affine.is_multiplicative() {
            return Err(UnitError::Parse(
                "affine units cannot be raised to a power".into(),
            ));
        }
        Ok(Self {
            dimension: self.dimension.pow(exp)?,
            scale: self.scale.powi(exp),
            affine: Affine::MULTIPLICATIVE,
            symbol: if exp == 1 {
                self.symbol.clone()
            } else {
                format!("{}^{exp}", self.symbol)
            },
        })
    }

    pub fn sqrt(&self) -> UnitResult<Self> {
        if !self.affine.is_multiplicative() {
            return Err(UnitError::Parse(
                "affine units cannot take square root".into(),
            ));
        }
        Ok(Self {
            dimension: self.dimension.sqrt()?,
            scale: self.scale.sqrt(),
            affine: Affine::MULTIPLICATIVE,
            symbol: format!("{}^0.5", self.symbol),
        })
    }

    pub fn to_base_magnitude(&self, magnitude: f64) -> f64 {
        if self.affine.is_multiplicative() {
            magnitude * self.scale
        } else {
            self.affine.to_base(magnitude)
        }
    }

    pub fn from_base_magnitude(&self, base: f64) -> f64 {
        if self.affine.is_multiplicative() {
            if self.scale == 0.0 {
                return f64::NAN;
            }
            base / self.scale
        } else {
            self.affine.from_base(base)
        }
    }

    pub fn conversion_ratio(&self, target: &Self) -> UnitResult<f64> {
        if !self.compatible(target) {
            return Err(UnitError::DimensionMismatch {
                left: self.dimension.format(),
                right: target.dimension.format(),
            });
        }
        if !self.affine.is_multiplicative() || !target.affine.is_multiplicative() {
            return Err(UnitError::Parse(
                "affine temperature conversion requires quantity.to()".into(),
            ));
        }
        if target.scale == 0.0 {
            return Err(UnitError::DivisionByZero);
        }
        Ok(self.scale / target.scale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn km_to_m_ratio() {
        let km = Unit {
            dimension: Dimension {
                l: 1,
                ..Default::default()
            },
            scale: 1000.0,
            affine: Affine::MULTIPLICATIVE,
            symbol: "km".into(),
        };
        let m = Unit {
            dimension: Dimension {
                l: 1,
                ..Default::default()
            },
            scale: 1.0,
            affine: Affine::MULTIPLICATIVE,
            symbol: "m".into(),
        };
        assert!((km.conversion_ratio(&m).unwrap() - 1000.0).abs() < 1e-12);
    }
}
