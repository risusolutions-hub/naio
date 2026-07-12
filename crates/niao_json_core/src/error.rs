use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    UnexpectedEof,
    Expected(&'static str),
    InvalidEscape,
    InvalidUnicode,
    InvalidNumber,
    TrailingData,
    DepthLimit,
    RecursionLimit,
    Message(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof => write!(f, "unexpected end of input"),
            Self::Expected(s) => write!(f, "expected {s}"),
            Self::InvalidEscape => write!(f, "invalid escape sequence"),
            Self::InvalidUnicode => write!(f, "invalid unicode escape"),
            Self::InvalidNumber => write!(f, "invalid number"),
            Self::TrailingData => write!(f, "trailing characters after JSON value"),
            Self::DepthLimit => write!(f, "maximum nesting depth exceeded"),
            Self::RecursionLimit => write!(f, "maximum recursion depth exceeded"),
            Self::Message(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for ParseError {}
