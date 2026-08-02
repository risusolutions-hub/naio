use crate::rounding::RoundingMode;

/// Arithmetic context controlling precision and rounding (Python `decimal.Context` subset).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    pub prec: u32,
    pub rounding: RoundingMode,
    pub emax: i32,
    pub emin: i32,
    pub clamp: bool,
}

impl Default for Context {
    fn default() -> Self {
        Self {
            prec: 28,
            rounding: RoundingMode::HalfEven,
            emax: 999_999,
            emin: -999_999,
            clamp: false,
        }
    }
}

impl Context {
    pub fn new(prec: u32, rounding: RoundingMode) -> Self {
        Self {
            prec,
            rounding,
            ..Self::default()
        }
    }

    pub fn money() -> Self {
        Self {
            prec: 28,
            rounding: RoundingMode::HalfEven,
            ..Self::default()
        }
    }
}
