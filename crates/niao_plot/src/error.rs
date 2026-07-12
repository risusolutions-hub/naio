//! Typed errors for nplot (codes 4040–4049).

use std::fmt;

pub const E4040_NPLOT_ARITY: u32 = 4040;
pub const E4041_NPLOT_EMPTY: u32 = 4041;
pub const E4042_NPLOT_LENGTH: u32 = 4042;
pub const E4043_NPLOT_HANDLE: u32 = 4043;
pub const E4044_NPLOT_RENDER: u32 = 4044;

#[derive(Debug, Clone, PartialEq)]
pub enum PlotError {
    Arity { expected: usize, got: usize },
    Empty(String),
    LengthMismatch(String),
    InvalidHandle(String),
    Render(String),
}

impl PlotError {
    pub fn code(&self) -> u32 {
        match self {
            Self::Arity { .. } => E4040_NPLOT_ARITY,
            Self::Empty(_) => E4041_NPLOT_EMPTY,
            Self::LengthMismatch(_) => E4042_NPLOT_LENGTH,
            Self::InvalidHandle(_) => E4043_NPLOT_HANDLE,
            Self::Render(_) => E4044_NPLOT_RENDER,
        }
    }
}

impl fmt::Display for PlotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arity { expected, got } => write!(f, "expected {expected} argument(s), got {got}"),
            Self::Empty(msg)
            | Self::LengthMismatch(msg)
            | Self::InvalidHandle(msg)
            | Self::Render(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for PlotError {}

impl From<niao_num::NumError> for PlotError {
    fn from(e: niao_num::NumError) -> Self {
        PlotError::Render(e.to_string())
    }
}

pub type PlotResult<T> = Result<T, PlotError>;

pub fn require_non_empty(data: &[f64], name: &str) -> PlotResult<()> {
    if data.is_empty() {
        return Err(PlotError::Empty(format!("{name}: empty data")));
    }
    Ok(())
}

pub fn require_same_len(a: &[f64], b: &[f64], name: &str) -> PlotResult<()> {
    if a.len() != b.len() {
        return Err(PlotError::LengthMismatch(format!(
            "{name}: length mismatch ({} vs {})",
            a.len(),
            b.len()
        )));
    }
    Ok(())
}
