use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalError {
    InvalidDate { year: i32, month: u32, day: u32 },
    InvalidMonth(u32),
    InvalidWeekday(u32),
    ParseError(String),
    RangeError(String),
    EmptyRange,
}

impl CalError {
    pub fn message(&self) -> String {
        match self {
            Self::InvalidDate { year, month, day } => {
                format!("invalid date {year:04}-{month:02}-{day:02}")
            }
            Self::InvalidMonth(m) => format!("month must be 1..=12, got {m}"),
            Self::InvalidWeekday(w) => format!("weekday must be 0..=6 (Mon=0), got {w}"),
            Self::ParseError(s) => s.clone(),
            Self::RangeError(s) => s.clone(),
            Self::EmptyRange => "empty date range".into(),
        }
    }
}

impl fmt::Display for CalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for CalError {}
