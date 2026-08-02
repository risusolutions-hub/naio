//! Multi-column aligned DataFrame.

use crate::error::{FrameError, FrameResult};
use crate::series::{ColumnData, CompositeKey, Series, StringColumn};
use crate::validity::Validity;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct DataFrame {
    pub columns: Vec<Series>,
    index: HashMap<String, usize>,
}

impl DataFrame {
    pub fn new(columns: Vec<Series>) -> FrameResult<Self> {
        if columns.is_empty() {
            return Ok(Self {
                columns,
                index: HashMap::new(),
            });
        }
        let len = columns[0].len();
        let mut index = HashMap::new();
        for (i, c) in columns.iter().enumerate() {
            if c.len() != len {
                return Err(FrameError::LengthMismatch(format!(
                    "column '{}' length {} != expected {}",
                    c.name,
                    c.len(),
                    len
                )));
            }
            if index.insert(c.name.clone(), i).is_some() {
                return Err(FrameError::Error(format!(
                    "duplicate column name '{}'",
                    c.name
                )));
            }
        }
        Ok(Self { columns, index })
    }

    pub fn empty() -> Self {
        Self {
            columns: Vec::new(),
            index: HashMap::new(),
        }
    }

    #[inline]
    pub fn nrows(&self) -> usize {
        self.columns.first().map(|c| c.len()).unwrap_or(0)
    }

    #[inline]
    pub fn ncols(&self) -> usize {
        self.columns.len()
    }

    pub fn shape(&self) -> (usize, usize) {
        (self.nrows(), self.ncols())
    }

    pub fn column_names(&self) -> Vec<String> {
        self.columns.iter().map(|c| c.name.clone()).collect()
    }

    pub fn get(&self, name: &str) -> FrameResult<&Series> {
        self.index
            .get(name)
            .map(|&i| &self.columns[i])
            .ok_or_else(|| FrameError::BadColumn(format!("unknown column '{name}'")))
    }

    pub fn get_mut(&mut self, name: &str) -> FrameResult<&mut Series> {
        let i = *self
            .index
            .get(name)
            .ok_or_else(|| FrameError::BadColumn(format!("unknown column '{name}'")))?;
        Ok(&mut self.columns[i])
    }

    pub fn select(&self, names: &[&str]) -> FrameResult<Self> {
        let mut cols = Vec::with_capacity(names.len());
        for &n in names {
            cols.push(self.get(n)?.clone());
        }
        Self::new(cols)
    }

    pub fn drop(&self, names: &[&str]) -> FrameResult<Self> {
        let drop: std::collections::HashSet<&str> = names.iter().copied().collect();
        for n in &drop {
            if !self.index.contains_key(*n) {
                return Err(FrameError::BadColumn(format!("unknown column '{n}'")));
            }
        }
        let cols: Vec<Series> = self
            .columns
            .iter()
            .filter(|c| !drop.contains(c.name.as_str()))
            .cloned()
            .collect();
        Self::new(cols)
    }

    pub fn rename(&self, mapping: &[(&str, &str)]) -> FrameResult<Self> {
        let mut cols = self.columns.clone();
        for &(from, to) in mapping {
            let i = *self
                .index
                .get(from)
                .ok_or_else(|| FrameError::BadColumn(format!("unknown column '{from}'")))?;
            cols[i].name = to.to_string();
        }
        Self::new(cols)
    }

    pub fn with_column(&self, series: Series) -> FrameResult<Self> {
        if !self.columns.is_empty() && series.len() != self.nrows() {
            return Err(FrameError::LengthMismatch(format!(
                "column '{}' length {} does not match frame length {}",
                series.name,
                series.len(),
                self.nrows()
            )));
        }
        let mut cols = self.columns.clone();
        if let Some(&i) = self.index.get(&series.name) {
            cols[i] = series;
        } else {
            cols.push(series);
        }
        Self::new(cols)
    }

    pub fn take_rows(&self, indices: &[usize]) -> FrameResult<Self> {
        let cols: Vec<Series> = self.columns.iter().map(|c| c.take(indices)).collect();
        Self::new(cols)
    }

    pub fn slice(&self, start: usize, end: usize) -> FrameResult<Self> {
        let cols: Vec<Series> = self.columns.iter().map(|c| c.slice(start, end)).collect();
        Self::new(cols)
    }

    pub fn head(&self, n: usize) -> FrameResult<Self> {
        self.slice(0, n.min(self.nrows()))
    }

    pub fn tail(&self, n: usize) -> FrameResult<Self> {
        let start = self.nrows().saturating_sub(n);
        self.slice(start, self.nrows())
    }

    /// Filter by boolean mask series (true = keep).
    pub fn filter_mask(&self, mask: &Series) -> FrameResult<Self> {
        if mask.len() != self.nrows() {
            return Err(FrameError::LengthMismatch(format!(
                "mask length {} != frame length {}",
                mask.len(),
                self.nrows()
            )));
        }
        let bits = match &mask.data {
            ColumnData::Bool(v) => v.as_slice(),
            _ => return Err(FrameError::Dtype("filter mask must be bool series".into())),
        };
        let indices: Vec<usize> = bits
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| if b { Some(i) } else { None })
            .collect();
        self.take_rows(&indices)
    }

    /// Predicate: keep rows where column `col` equals `value` (typed loosely).
    pub fn filter_eq(&self, col: &str, value: &FilterValue) -> FrameResult<Self> {
        let s = self.get(col)?;
        let mut indices = Vec::new();
        for i in 0..s.len() {
            if s.validity.is_null(i) {
                continue;
            }
            let keep = match (value, &s.data) {
                (FilterValue::I64(x), ColumnData::I64(v) | ColumnData::Date(v)) => v[i] == *x,
                (FilterValue::F64(x), ColumnData::F64(v)) => (v[i] - *x).abs() < 1e-15,
                (FilterValue::Bool(x), ColumnData::Bool(v)) => v[i] == *x,
                (FilterValue::Str(x), ColumnData::Str(v)) => v.get(i) == x.as_str(),
                (FilterValue::I64(x), ColumnData::F64(v)) => (v[i] - *x as f64).abs() < 1e-15,
                _ => {
                    return Err(FrameError::Dtype(format!(
                        "filter type mismatch on column '{}'",
                        col
                    )))
                }
            };
            if keep {
                indices.push(i);
            }
        }
        self.take_rows(&indices)
    }

    /// Stable multi-key sort. `keys` are (column, descending).
    pub fn sort(&self, keys: &[(&str, bool)]) -> FrameResult<Self> {
        if keys.is_empty() {
            return Ok(self.clone());
        }
        for (k, _) in keys {
            let _ = self.get(k)?;
        }
        let n = self.nrows();
        let mut indices: Vec<usize> = (0..n).collect();
        indices.sort_by(|&a, &b| {
            for &(name, desc) in keys {
                let s = self.get(name).unwrap();
                let cmp = compare_rows(s, a, b);
                if cmp != std::cmp::Ordering::Equal {
                    return if desc { cmp.reverse() } else { cmp };
                }
            }
            std::cmp::Ordering::Equal
        });
        self.take_rows(&indices)
    }

    /// Sample `n` rows without replacement (LCG seeded).
    pub fn sample(&self, n: usize, seed: u64) -> FrameResult<Self> {
        let nrows = self.nrows();
        if n > nrows {
            return Err(FrameError::Error(format!(
                "sample size {n} > frame length {nrows}"
            )));
        }
        let mut idx: Vec<usize> = (0..nrows).collect();
        let mut state = seed;
        for i in (1..nrows).rev() {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let j = (state as usize) % (i + 1);
            idx.swap(i, j);
        }
        self.take_rows(&idx[..n])
    }

    pub fn composite_key_at(&self, key_cols: &[&str], row: usize) -> FrameResult<CompositeKey> {
        let mut parts = Vec::with_capacity(key_cols.len());
        for &c in key_cols {
            parts.push(self.get(c)?.row_key(row));
        }
        Ok(CompositeKey(parts))
    }
}

#[derive(Clone, Debug)]
pub enum FilterValue {
    I64(i64),
    F64(f64),
    Bool(bool),
    Str(String),
}

fn compare_rows(s: &Series, a: usize, b: usize) -> std::cmp::Ordering {
    let na = s.validity.is_null(a);
    let nb = s.validity.is_null(b);
    if na && nb {
        return std::cmp::Ordering::Equal;
    }
    if na {
        return std::cmp::Ordering::Greater; // nulls last
    }
    if nb {
        return std::cmp::Ordering::Less;
    }
    match &s.data {
        ColumnData::I64(v) | ColumnData::Date(v) => v[a].cmp(&v[b]),
        ColumnData::F64(v) => v[a].partial_cmp(&v[b]).unwrap_or(std::cmp::Ordering::Equal),
        ColumnData::Bool(v) => v[a].cmp(&v[b]),
        ColumnData::Str(v) => v.get(a).cmp(v.get(b)),
    }
}

/// Append null rows / pick with optional nulls for joins.
pub(crate) fn series_take_opt(s: &Series, indices: &[Option<usize>]) -> Series {
    let n = indices.len();
    let mut validity = Validity::all_valid(n);
    let data = match &s.data {
        ColumnData::I64(v) => {
            let mut out = vec![0i64; n];
            for (j, idx) in indices.iter().enumerate() {
                match idx {
                    Some(i) if s.validity.is_valid(*i) => out[j] = v[*i],
                    _ => validity.set_null(j),
                }
            }
            ColumnData::I64(out)
        }
        ColumnData::F64(v) => {
            let mut out = vec![0.0f64; n];
            for (j, idx) in indices.iter().enumerate() {
                match idx {
                    Some(i) if s.validity.is_valid(*i) => out[j] = v[*i],
                    _ => {
                        out[j] = f64::NAN;
                        validity.set_null(j);
                    }
                }
            }
            ColumnData::F64(out)
        }
        ColumnData::Bool(v) => {
            let mut out = vec![false; n];
            for (j, idx) in indices.iter().enumerate() {
                match idx {
                    Some(i) if s.validity.is_valid(*i) => out[j] = v[*i],
                    _ => validity.set_null(j),
                }
            }
            ColumnData::Bool(out)
        }
        ColumnData::Str(v) => {
            let mut out = StringColumn::new();
            for (j, idx) in indices.iter().enumerate() {
                match idx {
                    Some(i) if s.validity.is_valid(*i) => out.push(v.get(*i)),
                    _ => {
                        out.push("");
                        validity.set_null(j);
                    }
                }
            }
            ColumnData::Str(out)
        }
        ColumnData::Date(v) => {
            let mut out = vec![0i64; n];
            for (j, idx) in indices.iter().enumerate() {
                match idx {
                    Some(i) if s.validity.is_valid(*i) => out[j] = v[*i],
                    _ => validity.set_null(j),
                }
            }
            ColumnData::Date(out)
        }
    };
    Series {
        name: s.name.clone(),
        data,
        validity,
    }
}
