//! Typed columnar Series with Arrow-style string storage.

use crate::error::{FrameError, FrameResult};
use crate::validity::Validity;
use niao_num::NdArray;
use std::hash::{Hash, Hasher};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dtype {
    I64,
    F64,
    Bool,
    Str,
    /// Civil date as days since Unix epoch (1970-01-01).
    Date,
}

impl Dtype {
    pub fn name(self) -> &'static str {
        match self {
            Dtype::I64 => "i64",
            Dtype::F64 => "f64",
            Dtype::Bool => "bool",
            Dtype::Str => "str",
            Dtype::Date => "date",
        }
    }
}

/// Arrow-style UTF-8 column: `offsets` length = n+1, `data` holds concatenated bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StringColumn {
    pub offsets: Vec<u32>,
    pub data: Vec<u8>,
}

impl StringColumn {
    pub fn new() -> Self {
        Self {
            offsets: vec![0],
            data: Vec::new(),
        }
    }

    pub fn from_iter<I, S>(iter: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut col = Self::new();
        for s in iter {
            col.push(s.as_ref());
        }
        col
    }

    pub fn from_strings(v: &[String]) -> Self {
        Self::from_iter(v.iter().map(|s| s.as_str()))
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.offsets.len().saturating_sub(1)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn push(&mut self, s: &str) {
        self.data.extend_from_slice(s.as_bytes());
        self.offsets.push(self.data.len() as u32);
    }

    #[inline]
    pub fn get(&self, i: usize) -> &str {
        if i + 1 >= self.offsets.len() {
            return "";
        }
        let start = self.offsets[i] as usize;
        let end = self.offsets[i + 1] as usize;
        // SAFETY: only valid UTF-8 pushed via `&str`
        std::str::from_utf8(&self.data[start..end]).unwrap_or("")
    }

    pub fn take(&self, indices: &[usize]) -> Self {
        let mut out = Self::new();
        out.offsets.reserve(indices.len() + 1);
        let est: usize = indices
            .iter()
            .map(|&i| {
                if i + 1 < self.offsets.len() {
                    (self.offsets[i + 1] - self.offsets[i]) as usize
                } else {
                    0
                }
            })
            .sum();
        out.data.reserve(est);
        for &i in indices {
            out.push(self.get(i));
        }
        out
    }

    pub fn slice(&self, start: usize, end: usize) -> Self {
        let end = end.min(self.len());
        let start = start.min(end);
        let mut out = Self::new();
        for i in start..end {
            out.push(self.get(i));
        }
        out
    }

    pub fn concat(&self, other: &Self) -> Self {
        let mut out = self.clone();
        for i in 0..other.len() {
            out.push(other.get(i));
        }
        out
    }

    pub fn to_vec(&self) -> Vec<String> {
        (0..self.len()).map(|i| self.get(i).to_string()).collect()
    }
}

impl Default for StringColumn {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub enum ColumnData {
    I64(Vec<i64>),
    F64(Vec<f64>),
    Bool(Vec<bool>),
    Str(StringColumn),
    Date(Vec<i64>),
}

impl ColumnData {
    pub fn len(&self) -> usize {
        match self {
            ColumnData::I64(v) => v.len(),
            ColumnData::F64(v) => v.len(),
            ColumnData::Bool(v) => v.len(),
            ColumnData::Str(v) => v.len(),
            ColumnData::Date(v) => v.len(),
        }
    }

    pub fn dtype(&self) -> Dtype {
        match self {
            ColumnData::I64(_) => Dtype::I64,
            ColumnData::F64(_) => Dtype::F64,
            ColumnData::Bool(_) => Dtype::Bool,
            ColumnData::Str(_) => Dtype::Str,
            ColumnData::Date(_) => Dtype::Date,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Series {
    pub name: String,
    pub data: ColumnData,
    pub validity: Validity,
}

impl Series {
    pub fn new(name: impl Into<String>, data: ColumnData) -> Self {
        let len = data.len();
        Self {
            name: name.into(),
            data,
            validity: Validity::all_valid(len),
        }
    }

    pub fn with_validity(mut self, validity: Validity) -> FrameResult<Self> {
        if validity.len() != self.len() {
            return Err(FrameError::LengthMismatch(format!(
                "validity length {} != series length {}",
                validity.len(),
                self.len()
            )));
        }
        self.validity = validity;
        Ok(self)
    }

    pub fn from_i64(name: impl Into<String>, v: Vec<i64>) -> Self {
        Self::new(name, ColumnData::I64(v))
    }

    pub fn from_f64(name: impl Into<String>, v: Vec<f64>) -> Self {
        Self::new(name, ColumnData::F64(v))
    }

    pub fn from_bool(name: impl Into<String>, v: Vec<bool>) -> Self {
        Self::new(name, ColumnData::Bool(v))
    }

    pub fn from_str(name: impl Into<String>, v: &[impl AsRef<str>]) -> Self {
        Self::new(
            name,
            ColumnData::Str(StringColumn::from_iter(v.iter().map(|s| s.as_ref()))),
        )
    }

    pub fn from_date(name: impl Into<String>, days: Vec<i64>) -> Self {
        Self::new(name, ColumnData::Date(days))
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn dtype(&self) -> Dtype {
        self.data.dtype()
    }

    pub fn null_count(&self) -> usize {
        self.validity.null_count()
    }

    pub fn is_null_mask(&self) -> Series {
        let bits: Vec<bool> = (0..self.len()).map(|i| self.validity.is_null(i)).collect();
        Series::from_bool(format!("{}_is_null", self.name), bits)
    }

    pub fn take(&self, indices: &[usize]) -> Self {
        let data = match &self.data {
            ColumnData::I64(v) => ColumnData::I64(indices.iter().map(|&i| v[i]).collect()),
            ColumnData::F64(v) => ColumnData::F64(indices.iter().map(|&i| v[i]).collect()),
            ColumnData::Bool(v) => ColumnData::Bool(indices.iter().map(|&i| v[i]).collect()),
            ColumnData::Str(v) => ColumnData::Str(v.take(indices)),
            ColumnData::Date(v) => ColumnData::Date(indices.iter().map(|&i| v[i]).collect()),
        };
        Series {
            name: self.name.clone(),
            data,
            validity: self.validity.take(indices),
        }
    }

    pub fn slice(&self, start: usize, end: usize) -> Self {
        let end = end.min(self.len());
        let start = start.min(end);
        let data = match &self.data {
            ColumnData::I64(v) => ColumnData::I64(v[start..end].to_vec()),
            ColumnData::F64(v) => ColumnData::F64(v[start..end].to_vec()),
            ColumnData::Bool(v) => ColumnData::Bool(v[start..end].to_vec()),
            ColumnData::Str(v) => ColumnData::Str(v.slice(start, end)),
            ColumnData::Date(v) => ColumnData::Date(v[start..end].to_vec()),
        };
        Series {
            name: self.name.clone(),
            data,
            validity: self.validity.slice(start, end),
        }
    }

    /// Contiguous f64 view for numeric series (copy for i64/date).
    pub fn to_f64_vec(&self) -> FrameResult<Vec<f64>> {
        match &self.data {
            ColumnData::F64(v) => Ok(v.clone()),
            ColumnData::I64(v) | ColumnData::Date(v) => Ok(v.iter().map(|&x| x as f64).collect()),
            ColumnData::Bool(v) => Ok(v.iter().map(|&b| if b { 1.0 } else { 0.0 }).collect()),
            ColumnData::Str(_) => Err(FrameError::Dtype(
                "cannot convert string series to f64".into(),
            )),
        }
    }

    /// Zero-copy contiguous `NdArray` when series is f64 and fully valid.
    pub fn to_nnum(&self) -> FrameResult<NdArray> {
        let v = self.to_f64_vec()?;
        NdArray::from_vec(vec![v.len()], v).map_err(|e| FrameError::Error(e.to_string()))
    }

    pub fn rename(&self, name: impl Into<String>) -> Self {
        let mut s = self.clone();
        s.name = name.into();
        s
    }

    /// Hashable key for row `i` (nulls get a dedicated sentinel).
    pub fn row_key(&self, i: usize) -> RowKey {
        if self.validity.is_null(i) {
            return RowKey::Null;
        }
        match &self.data {
            ColumnData::I64(v) | ColumnData::Date(v) => RowKey::I64(v[i]),
            ColumnData::F64(v) => RowKey::F64(v[i].to_bits()),
            ColumnData::Bool(v) => RowKey::Bool(v[i]),
            ColumnData::Str(v) => RowKey::Str(v.get(i).to_string()),
        }
    }

    pub fn as_f64_slice(&self) -> Option<&[f64]> {
        match &self.data {
            ColumnData::F64(v) => Some(v.as_slice()),
            _ => None,
        }
    }

    pub fn as_i64_slice(&self) -> Option<&[i64]> {
        match &self.data {
            ColumnData::I64(v) | ColumnData::Date(v) => Some(v.as_slice()),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RowKey {
    Null,
    I64(i64),
    F64(u64),
    Bool(bool),
    Str(String),
}

impl RowKey {
    pub fn hash_u64(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.hash(&mut h);
        h.finish()
    }
}

/// Multi-column composite key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CompositeKey(pub Vec<RowKey>);
