//! Cell values and coordinate helpers.

use crate::error::{XlsxError, XlsxResult};
use calamine::Data;

#[derive(Debug, Clone, PartialEq)]
pub enum CellValue {
    Empty,
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Formula(String),
    /// Excel serial date (days since 1899-12-30).
    Date(f64),
    Error(String),
}

impl CellValue {
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    pub fn as_display_string(&self) -> String {
        match self {
            Self::Empty => String::new(),
            Self::Int(n) => n.to_string(),
            Self::Float(f) => f.to_string(),
            Self::Bool(b) => b.to_string(),
            Self::String(s) => s.clone(),
            Self::Formula(f) => f.clone(),
            Self::Date(d) => d.to_string(),
            Self::Error(e) => e.clone(),
        }
    }
}

impl From<Data> for CellValue {
    fn from(dt: Data) -> Self {
        match dt {
            Data::Empty => Self::Empty,
            Data::String(s) => Self::String(s),
            Data::Float(f) => {
                if (f.fract() - 0.0).abs() < f64::EPSILON
                    && f >= i64::MIN as f64
                    && f <= i64::MAX as f64
                {
                    Self::Int(f as i64)
                } else {
                    Self::Float(f)
                }
            }
            Data::Int(i) => Self::Int(i),
            Data::Bool(b) => Self::Bool(b),
            Data::DateTime(dt) => Self::Date(dt.as_f64()),
            Data::DateTimeIso(s) => Self::String(s),
            Data::DurationIso(s) => Self::String(s),
            Data::Error(e) => Self::Error(format!("{e:?}")),
        }
    }
}

/// 1-based column index to Excel letters (`1` -> `A`, `27` -> `AA`).
pub fn column_letter(mut col: u32) -> XlsxResult<String> {
    if col == 0 {
        return Err(XlsxError::Cell("column index must be >= 1".into()));
    }
    let mut s = String::new();
    while col > 0 {
        let rem = ((col - 1) % 26) as u8;
        s.insert(0, (b'A' + rem) as char);
        col = (col - 1) / 26;
    }
    Ok(s)
}

/// Excel letters to 1-based column index (`A` -> `1`, `AA` -> `27`).
pub fn column_index(letters: &str) -> XlsxResult<u32> {
    let up = letters.trim().to_ascii_uppercase();
    if up.is_empty() {
        return Err(XlsxError::Cell("empty column letters".into()));
    }
    let mut col: u32 = 0;
    for ch in up.chars() {
        if !ch.is_ascii_alphabetic() {
            return Err(XlsxError::Cell(format!("invalid column letter: {letters}")));
        }
        col = col
            .checked_mul(26)
            .and_then(|v| v.checked_add((ch as u8 - b'A' + 1) as u32))
            .ok_or_else(|| XlsxError::Cell(format!("column overflow: {letters}")))?;
    }
    Ok(col)
}

/// Parse `A1`, `B2:C10`, or `Sheet1!A1` style ranges (1-based).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellRange {
    pub sheet: Option<String>,
    pub start_row: u32,
    pub start_col: u32,
    pub end_row: u32,
    pub end_col: u32,
}

pub fn parse_range(spec: &str) -> XlsxResult<CellRange> {
    let spec = spec.trim();
    let (sheet, rest) = if let Some((s, r)) = spec.split_once('!') {
        (Some(s.to_string()), r)
    } else {
        (None, spec)
    };
    let parts: Vec<&str> = rest.split(':').collect();
    if parts.is_empty() || parts.len() > 2 {
        return Err(XlsxError::Cell(format!("invalid range: {spec}")));
    }
    let (sr, sc) = parse_cell_ref(parts[0])?;
    let (er, ec) = if parts.len() == 2 {
        parse_cell_ref(parts[1])?
    } else {
        (sr, sc)
    };
    Ok(CellRange {
        sheet,
        start_row: sr.min(er),
        start_col: sc.min(ec),
        end_row: sr.max(er),
        end_col: sc.max(ec),
    })
}

fn parse_cell_ref(s: &str) -> XlsxResult<(u32, u32)> {
    let s = s.trim().to_ascii_uppercase();
    let split = s
        .char_indices()
        .find(|(_, c)| c.is_ascii_digit())
        .ok_or_else(|| XlsxError::Cell(format!("invalid cell ref: {s}")))?;
    let (letters, digits) = s.split_at(split.0);
    if letters.is_empty() || digits.is_empty() {
        return Err(XlsxError::Cell(format!("invalid cell ref: {s}")));
    }
    let col = column_index(letters)?;
    let row: u32 = digits
        .parse()
        .map_err(|_| XlsxError::Cell(format!("invalid row in cell ref: {s}")))?;
    if row == 0 {
        return Err(XlsxError::Cell("row index must be >= 1".into()));
    }
    Ok((row, col))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_roundtrip() {
        assert_eq!(column_letter(1).unwrap(), "A");
        assert_eq!(column_letter(26).unwrap(), "Z");
        assert_eq!(column_letter(27).unwrap(), "AA");
        assert_eq!(column_index("AA").unwrap(), 27);
    }

    #[test]
    fn parse_a1_c3() {
        let r = parse_range("B2:D4").unwrap();
        assert_eq!(r.start_row, 2);
        assert_eq!(r.end_col, column_index("D").unwrap());
    }
}
