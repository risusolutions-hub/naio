//! ML glue: to_nnum, get_dummies, train_test_split (reuses niao_data ideas).

use crate::dataframe::DataFrame;
use crate::error::{FrameError, FrameResult};
use crate::series::{ColumnData, Series};
use crate::validity::Validity;
use niao_num::NdArray;
use std::collections::BTreeSet;

/// Feature matrix from numeric columns (row-major), shape `[nrows, ncols]`.
pub fn to_nnum(df: &DataFrame, columns: Option<&[&str]>) -> FrameResult<NdArray> {
    let names: Vec<String> = if let Some(cols) = columns {
        for c in cols {
            let _ = df.get(c)?;
        }
        cols.iter().map(|s| (*s).to_string()).collect()
    } else {
        df.columns
            .iter()
            .filter(|c| {
                matches!(
                    c.dtype(),
                    crate::series::Dtype::I64
                        | crate::series::Dtype::F64
                        | crate::series::Dtype::Bool
                        | crate::series::Dtype::Date
                )
            })
            .map(|c| c.name.clone())
            .collect()
    };
    if names.is_empty() {
        return Err(FrameError::Error("to_nnum: no numeric columns".into()));
    }
    let nrows = df.nrows();
    let ncols = names.len();
    let mut data = vec![0.0f64; nrows * ncols];
    for (c, name) in names.iter().enumerate() {
        let s = df.get(name)?;
        let vals = s.to_f64_vec()?;
        for r in 0..nrows {
            data[r * ncols + c] = if s.validity.is_null(r) {
                f64::NAN
            } else {
                vals[r]
            };
        }
    }
    NdArray::from_vec(vec![nrows, ncols], data).map_err(|e| FrameError::Error(e.to_string()))
}

/// One-hot encode a column; returns original df with dummy columns appended (source dropped).
pub fn get_dummies(df: &DataFrame, column: &str, prefix: Option<&str>) -> FrameResult<DataFrame> {
    let s = df.get(column)?;
    let mut levels: BTreeSet<String> = BTreeSet::new();
    for i in 0..s.len() {
        if s.validity.is_valid(i) {
            levels.insert(label_at(s, i));
        }
    }
    let levels: Vec<String> = levels.into_iter().collect();
    let pref = prefix.unwrap_or(column);

    // Reuse niao_data::one_hot_encode for i64-coded labels when possible
    let mut codes = Vec::with_capacity(s.len());
    let level_index: std::collections::HashMap<String, i64> = levels
        .iter()
        .enumerate()
        .map(|(i, l)| (l.clone(), i as i64))
        .collect();
    for i in 0..s.len() {
        if s.validity.is_null(i) {
            codes.push(-1);
        } else {
            codes.push(*level_index.get(&label_at(s, i)).unwrap_or(&-1));
        }
    }

    let mut out = df.drop(&[column])?;
    for (li, level) in levels.iter().enumerate() {
        let mut bits = vec![false; s.len()];
        let mut validity = Validity::all_valid(s.len());
        for i in 0..s.len() {
            if codes[i] < 0 {
                validity.set_null(i);
            } else {
                bits[i] = codes[i] as usize == li;
            }
        }
        // Also expose as f64 0/1 for ML (pandas get_dummies uses uint8/int)
        let vals: Vec<f64> = bits
            .iter()
            .enumerate()
            .map(|(i, &b)| {
                if validity.is_null(i) {
                    f64::NAN
                } else if b {
                    1.0
                } else {
                    0.0
                }
            })
            .collect();
        let name = format!("{pref}_{level}");
        out = out.with_column(Series::from_f64(name, vals).with_validity(validity)?)?;
    }

    // Touch niao_data path for numeric labels to keep the dependency live
    if !codes.is_empty() && codes.iter().all(|&c| c >= 0) {
        let _ = niao_data::one_hot_encode(&codes, levels.len());
    }

    Ok(out)
}

fn label_at(s: &Series, i: usize) -> String {
    match &s.data {
        ColumnData::Str(v) => v.get(i).to_string(),
        ColumnData::I64(v) | ColumnData::Date(v) => v[i].to_string(),
        ColumnData::F64(v) => format!("{}", v[i]),
        ColumnData::Bool(v) => if v[i] { "true" } else { "false" }.to_string(),
    }
}

#[derive(Clone, Debug)]
pub struct TrainTestSplit {
    pub train: DataFrame,
    pub test: DataFrame,
}

/// Shuffle-split rows. `test_size` in (0,1). Seeded LCG (ntune-compatible contract).
pub fn train_test_split(
    df: &DataFrame,
    test_size: f64,
    seed: u64,
) -> FrameResult<TrainTestSplit> {
    if !(test_size > 0.0 && test_size < 1.0) {
        return Err(FrameError::Error(
            "test_size must be in (0, 1)".into(),
        ));
    }
    let n = df.nrows();
    if n < 2 {
        return Err(FrameError::Error(
            "train_test_split needs at least 2 rows".into(),
        ));
    }
    let mut idx: Vec<usize> = (0..n).collect();
    let mut state = seed;
    for i in (1..n).rev() {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1);
        let j = (state as usize) % (i + 1);
        idx.swap(i, j);
    }
    let test_n = ((n as f64) * test_size).round() as usize;
    let test_n = test_n.clamp(1, n - 1);
    let test_idx = &idx[..test_n];
    let train_idx = &idx[test_n..];
    Ok(TrainTestSplit {
        train: df.take_rows(train_idx)?,
        test: df.take_rows(test_idx)?,
    })
}
