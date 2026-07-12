use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Number {
    I64(i64),
    U64(u64),
    F64(f64),
}

impl Number {
    #[inline]
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::I64(n) => Some(*n),
            Self::U64(n) if *n <= i64::MAX as u64 => Some(*n as i64),
            Self::F64(f) => {
                if f.fract() == 0.0 && *f >= i64::MIN as f64 && *f <= i64::MAX as f64 {
                    Some(*f as i64)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    #[inline]
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::U64(n) => Some(*n),
            Self::I64(n) if *n >= 0 => Some(*n as u64),
            Self::F64(f) if f.fract() == 0.0 && *f >= 0.0 && *f <= u64::MAX as f64 => {
                Some(*f as u64)
            }
            _ => None,
        }
    }

    #[inline]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::F64(f) => Some(*f),
            Self::I64(n) => Some(*n as f64),
            Self::U64(n) => Some(*n as f64),
        }
    }

    #[inline]
    pub fn is_finite(&self) -> bool {
        match self {
            Self::F64(f) => f.is_finite(),
            _ => true,
        }
    }
}

impl fmt::Display for Number {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::I64(n) => write!(f, "{n}"),
            Self::U64(n) => write!(f, "{n}"),
            Self::F64(v) => {
                if *v == 0.0 && v.is_sign_negative() {
                    write!(f, "-0")
                } else if v.is_finite() {
                    write!(f, "{v}")
                } else {
                    write!(f, "null")
                }
            }
        }
    }
}
