use std::fmt;

/// Errors from iCalendar / vCard / RRULE operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IcalError {
    TooLarge(usize),
    Empty,
    UnbalancedComponent { name: String, line: u32 },
    UnexpectedEnd { expected: String, line: u32 },
    InvalidProperty { line: u32, detail: String },
    InvalidDateTime(String),
    InvalidRrule(String),
    Io(String),
}

impl IcalError {
    pub fn message(&self) -> String {
        match self {
            IcalError::TooLarge(n) => format!("input exceeds {n} byte limit"),
            IcalError::Empty => "empty input".into(),
            IcalError::UnbalancedComponent { name, line } => {
                format!("unbalanced component {name} at line {line}")
            }
            IcalError::UnexpectedEnd { expected, line } => {
                format!("unexpected end, expected {expected} at line {line}")
            }
            IcalError::InvalidProperty { line, detail } => {
                format!("invalid property at line {line}: {detail}")
            }
            IcalError::InvalidDateTime(s) => format!("invalid date/time: {s}"),
            IcalError::InvalidRrule(s) => format!("invalid RRULE: {s}"),
            IcalError::Io(s) => s.clone(),
        }
    }
}

impl fmt::Display for IcalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for IcalError {}

/// Maximum input/output size (64 MiB guard).
pub const MAX_BYTES: usize = 64 * 1024 * 1024;
