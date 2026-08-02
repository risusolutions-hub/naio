//! Decimal rounding modes (PEP 327 / Python `decimal` parity).

/// Rounding mode for decimal quantization and arithmetic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RoundingMode {
    /// Round toward +∞.
    Ceiling,
    /// Round toward -∞.
    Floor,
    /// Round toward zero.
    #[default]
    Down,
    /// Round away from zero.
    Up,
    /// Round to nearest, ties away from zero.
    HalfUp,
    /// Round to nearest, ties to even (banker's — money-safe default).
    HalfEven,
    /// Round to nearest, ties toward zero.
    HalfDown,
    /// Round away from zero if last digit is 0 or 5.
    ZeroFiveUp,
}

impl RoundingMode {
    pub fn from_name(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "ceiling" | "round_ceiling" => Some(Self::Ceiling),
            "floor" | "round_floor" => Some(Self::Floor),
            "down" | "round_down" | "truncate" => Some(Self::Down),
            "up" | "round_up" => Some(Self::Up),
            "half_up" | "round_half_up" => Some(Self::HalfUp),
            "half_even" | "round_half_even" | "bankers" => Some(Self::HalfEven),
            "half_down" | "round_half_down" => Some(Self::HalfDown),
            "05up" | "round_05up" | "zero_five_up" => Some(Self::ZeroFiveUp),
            _ => None,
        }
    }

    pub fn as_name(self) -> &'static str {
        match self {
            Self::Ceiling => "ceiling",
            Self::Floor => "floor",
            Self::Down => "down",
            Self::Up => "up",
            Self::HalfUp => "half_up",
            Self::HalfEven => "half_even",
            Self::HalfDown => "half_down",
            Self::ZeroFiveUp => "05up",
        }
    }
}

/// Increment `coeff` (non-negative magnitude) by one when rounding says so.
pub fn increment_coeff(
    coeff: &mut niao_bignum::BigInt,
    mode: RoundingMode,
    sign: niao_bignum::Sign,
    digit: u8,
) -> bool {
    use niao_bignum::Sign;
    let last_odd = {
        let s = coeff.to_string();
        s.chars()
            .last()
            .map(|c| c.to_digit(10).unwrap_or(0) % 2 == 1)
            .unwrap_or(false)
    };
    let increment = match mode {
        RoundingMode::Down => false,
        RoundingMode::Up => digit != 0,
        RoundingMode::Ceiling => sign != Sign::Minus && digit != 0,
        RoundingMode::Floor => sign == Sign::Minus && digit != 0,
        RoundingMode::HalfUp => digit >= 5,
        RoundingMode::HalfEven => digit > 5 || (digit == 5 && last_odd),
        RoundingMode::HalfDown => digit > 5 || (digit == 5 && false),
        RoundingMode::ZeroFiveUp => digit != 0 && (digit != 5 || !coeff.is_zero()),
    };
    if increment {
        *coeff = (&*coeff) + &niao_bignum::BigInt::from(1u32);
    }
    increment
}
