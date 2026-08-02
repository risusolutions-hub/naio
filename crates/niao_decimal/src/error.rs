use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecimalError {
    Parse(String),
    InvalidOperation(String),
    DivisionByZero,
    DivisionImpossible,
    InvalidContext,
    Overflow,
    Underflow,
}

impl fmt::Display for DecimalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(msg) => write!(f, "{msg}"),
            Self::InvalidOperation(msg) => write!(f, "{msg}"),
            Self::DivisionByZero => f.write_str("division by zero"),
            Self::DivisionImpossible => f.write_str("division impossible"),
            Self::InvalidContext => f.write_str("invalid context"),
            Self::Overflow => f.write_str("overflow"),
            Self::Underflow => f.write_str("underflow"),
        }
    }
}

impl std::error::Error for DecimalError {}

pub type DecimalResult<T> = Result<T, DecimalError>;
