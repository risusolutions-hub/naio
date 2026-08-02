//! Columnar dataset backed by nframe DataFrame.

use crate::error::{DatasetError, DatasetResult};
use niao_frame::{concat, DataFrame, FilterValue};
use rand::seq::SliceRandom;
use rand::SeedableRng;
use std::collections::HashSet;

/// In-memory tabular dataset (columnar storage via `nframe`).
#[derive(Clone, Debug)]
pub struct Dataset {
    pub frame: DataFrame,
}

impl Dataset {
    pub fn new(frame: DataFrame) -> Self {
        Self { frame }
    }

    /// Row count.
    ///
    /// // >>> use niao_dataset::Dataset;
    /// // (len checked via frame.nrows in tests)
    #[inline]
    pub fn len(&self) -> usize {
        self.frame.nrows()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Column names in frame order.
    pub fn columns(&self) -> Vec<String> {
        self.frame.column_names()
    }

    /// Subset columns by name.
    pub fn select(&self, cols: &[String]) -> DatasetResult<Self> {
        if cols.is_empty() {
            return Err(DatasetError::Param(
                "select requires at least one column".into(),
            ));
        }
        let mut out_cols = Vec::with_capacity(cols.len());
        for name in cols {
            out_cols.push(self.frame.get(name)?.clone());
        }
        Ok(Self::new(DataFrame::new(out_cols)?))
    }

    /// Keep rows where `column` equals `value`.
    pub fn filter_eq(&self, column: &str, value: &FilterValue) -> DatasetResult<Self> {
        Ok(Self::new(self.frame.filter_eq(column, value)?))
    }

    /// Return a shuffled copy (Fisher–Yates on row indices).
    pub fn shuffle(&self, seed: u64) -> DatasetResult<Self> {
        let n = self.len();
        if n <= 1 {
            return Ok(self.clone());
        }
        let mut indices: Vec<usize> = (0..n).collect();
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        indices.shuffle(&mut rng);
        Ok(Self::new(self.frame.take_rows(&indices)?))
    }

    /// First `n` rows.
    pub fn take(&self, n: usize) -> DatasetResult<Self> {
        Ok(Self::new(self.frame.head(n)?))
    }

    /// Skip first `n` rows.
    pub fn skip(&self, n: usize) -> DatasetResult<Self> {
        if n >= self.len() {
            return Ok(Self::new(DataFrame::empty()));
        }
        Ok(Self::new(self.frame.slice(n, self.len())?))
    }

    /// Vertical concat of datasets with matching columns.
    pub fn concat(items: &[&Dataset]) -> DatasetResult<Self> {
        if items.is_empty() {
            return Ok(Self::new(DataFrame::empty()));
        }
        let refs: Vec<&DataFrame> = items.iter().map(|d| &d.frame).collect();
        Ok(Self::new(concat(&refs, 0)?))
    }

    /// Row index bounds check.
    pub fn check_index(&self, index: isize) -> DatasetResult<usize> {
        let n = self.len() as isize;
        let idx = if index < 0 { n + index } else { index };
        if idx < 0 || idx >= n {
            return Err(DatasetError::Index(format!(
                "row index {index} out of range for dataset len {}",
                self.len()
            )));
        }
        Ok(idx as usize)
    }
}

/// Shuffle-split into train / optional val / optional test portions.
pub fn split_ratios(
    ds: &Dataset,
    train: f64,
    val: Option<f64>,
    test: Option<f64>,
    seed: u64,
) -> DatasetResult<SplitOutput> {
    if train <= 0.0 || train >= 1.0 {
        return Err(DatasetError::Param("train ratio must be in (0, 1)".into()));
    }
    let v = val.unwrap_or(0.0);
    let t = test.unwrap_or(0.0);
    if v < 0.0 || t < 0.0 {
        return Err(DatasetError::Param(
            "val and test ratios must be >= 0".into(),
        ));
    }
    let sum = train + v + t;
    if sum > 1.0 + 1e-9 {
        return Err(DatasetError::Param(format!(
            "ratios sum to {sum}, must be <= 1"
        )));
    }
    let n = ds.len();
    if n == 0 {
        return Err(DatasetError::Error("cannot split empty dataset".into()));
    }
    if n == 1 && (v > 0.0 || t > 0.0) {
        return Err(DatasetError::Error(
            "need at least 2 rows for multi-way split".into(),
        ));
    }

    let mut indices: Vec<usize> = (0..n).collect();
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    indices.shuffle(&mut rng);

    let mut train_n = ((n as f64) * train).round() as usize;
    train_n = train_n.clamp(1, n);
    let val_n = if v > 0.0 {
        ((n as f64) * v).round() as usize
    } else {
        0
    };
    let explicit_test = if t > 0.0 {
        Some(((n as f64) * t).round() as usize)
    } else {
        None
    };
    let mut end_train = train_n.min(n);
    let mut end_val = (end_train + val_n).min(n);
    let end_all = if let Some(test_n) = explicit_test {
        (end_val + test_n).min(n)
    } else {
        n
    };

    if end_train >= n && n > 1 {
        end_train = n - 1;
    }
    if end_val <= end_train && val_n > 0 && end_train < n {
        end_val = (end_train + 1).min(n);
    }

    let train_idx = &indices[..end_train];
    let val_idx = if val_n > 0 && end_train < end_val {
        Some(&indices[end_train..end_val])
    } else {
        None
    };
    let test_idx = if end_val < end_all {
        Some(&indices[end_val..end_all])
    } else {
        None
    };

    Ok(SplitOutput {
        train: Dataset::new(ds.frame.take_rows(train_idx)?),
        val: val_idx.map(|ix| Dataset::new(ds.frame.take_rows(ix).unwrap())),
        test: test_idx.map(|ix| Dataset::new(ds.frame.take_rows(ix).unwrap())),
    })
}

#[derive(Clone, Debug)]
pub struct SplitOutput {
    pub train: Dataset,
    pub val: Option<Dataset>,
    pub test: Option<Dataset>,
}

/// Build dataset from row maps (column -> string representation for inference).
pub fn from_row_maps(
    rows: Vec<std::collections::HashMap<String, String>>,
) -> DatasetResult<Dataset> {
    if rows.is_empty() {
        return Ok(Dataset::new(DataFrame::empty()));
    }
    let mut key_set: HashSet<String> = HashSet::new();
    let mut col_order: Vec<String> = Vec::new();
    for row in &rows {
        for k in row.keys() {
            if key_set.insert(k.clone()) {
                col_order.push(k.clone());
            }
        }
    }
    let mut cols: Vec<Vec<String>> = vec![Vec::with_capacity(rows.len()); col_order.len()];
    for row in &rows {
        for (ci, name) in col_order.iter().enumerate() {
            cols[ci].push(row.get(name).cloned().unwrap_or_default());
        }
    }
    let series: Vec<niao_frame::Series> = col_order
        .into_iter()
        .zip(cols)
        .map(|(name, raw)| infer_series(name, &raw))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Dataset::new(DataFrame::new(series)?))
}

fn infer_series(name: String, raw: &[String]) -> DatasetResult<niao_frame::Series> {
    use niao_frame::{Series, Validity};
    if raw.is_empty() {
        return Ok(Series::from_str(name, &[] as &[String]));
    }
    let non_empty: Vec<&str> = raw
        .iter()
        .map(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .collect();
    if non_empty.is_empty() {
        return Ok(Series::from_str(name, raw));
    }
    let mut all_int = true;
    let mut all_float = true;
    let mut all_bool = true;
    for s in &non_empty {
        if parse_bool(s).is_none() {
            all_bool = false;
        }
        if s.parse::<i64>().is_err() {
            all_int = false;
        }
        if s.parse::<f64>().is_err() {
            all_float = false;
        }
    }
    if all_bool && non_empty.iter().all(|s| parse_bool(s).is_some()) {
        let vals: Vec<bool> = raw.iter().map(|s| parse_bool(s).unwrap_or(false)).collect();
        let validity = Validity::from_bools(&raw.iter().map(|s| !s.is_empty()).collect::<Vec<_>>());
        return Ok(Series::from_bool(name, vals).with_validity(validity)?);
    }
    if all_int {
        let mut vals = Vec::with_capacity(raw.len());
        let mut mask = Vec::with_capacity(raw.len());
        for s in raw {
            if s.is_empty() {
                vals.push(0);
                mask.push(false);
            } else {
                vals.push(s.parse().unwrap_or(0));
                mask.push(true);
            }
        }
        let validity = Validity::from_bools(&mask);
        return Ok(Series::from_i64(name, vals).with_validity(validity)?);
    }
    if all_float {
        let mut vals = Vec::with_capacity(raw.len());
        let mut mask = Vec::with_capacity(raw.len());
        for s in raw {
            if s.is_empty() {
                vals.push(0.0);
                mask.push(false);
            } else {
                vals.push(s.parse().unwrap_or(0.0));
                mask.push(true);
            }
        }
        let validity = Validity::from_bools(&mask);
        return Ok(Series::from_f64(name, vals).with_validity(validity)?);
    }
    Ok(Series::from_str(name, raw))
}

fn parse_bool(s: &str) -> Option<bool> {
    match s {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_frame::Series;

    fn sample() -> Dataset {
        Dataset::new(
            DataFrame::new(vec![
                Series::from_i64("id", vec![1, 2, 3, 4, 5]),
                Series::from_str("label", &["a", "b", "a", "c", "b"]),
            ])
            .unwrap(),
        )
    }

    #[test]
    fn shuffle_preserves_len() {
        let ds = sample();
        let sh = ds.shuffle(42).unwrap();
        assert_eq!(sh.len(), 5);
    }

    #[test]
    fn filter_eq_works() {
        let ds = sample();
        let f = ds
            .filter_eq("label", &FilterValue::Str("a".into()))
            .unwrap();
        assert_eq!(f.len(), 2);
    }

    #[test]
    fn split_covers_all_rows() {
        let ds = sample();
        let parts = split_ratios(&ds, 0.6, Some(0.2), Some(0.2), 7).unwrap();
        let total = parts.train.len()
            + parts.val.as_ref().map(|d| d.len()).unwrap_or(0)
            + parts.test.as_ref().map(|d| d.len()).unwrap_or(0);
        assert_eq!(total, 5);
    }
}
