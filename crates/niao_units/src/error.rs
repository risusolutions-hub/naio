use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum UnitError {
    Parse(String),
    UnknownUnit(String),
    DimensionMismatch { left: String, right: String },
    DimensionOverflow,
    NonIntegerRoot { dimension: String, exponent: i8 },
    NotDimensionless,
    EmptyInput,
    InvalidExponent,
    DivisionByZero,
    Overflow,
}

impl fmt::Display for UnitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(m) => write!(f, "parse error: {m}"),
            Self::UnknownUnit(u) => write!(f, "unknown unit: {u}"),
            Self::DimensionMismatch { left, right } => {
                write!(f, "incompatible dimensions: {left} vs {right}")
            }
            Self::DimensionOverflow => write!(f, "dimension exponent overflow"),
            Self::NonIntegerRoot {
                dimension,
                exponent,
            } => write!(
                f,
                "cannot take root: {dimension} exponent {exponent} is odd"
            ),
            Self::NotDimensionless => write!(f, "quantity is not dimensionless"),
            Self::EmptyInput => write!(f, "empty input"),
            Self::InvalidExponent => write!(f, "invalid exponent"),
            Self::DivisionByZero => write!(f, "division by zero"),
            Self::Overflow => write!(f, "numeric overflow"),
        }
    }
}

impl std::error::Error for UnitError {}

pub type UnitResult<T> = Result<T, UnitError>;
