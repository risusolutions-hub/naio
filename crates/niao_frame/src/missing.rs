//! Missing-data helpers: is_null, drop_nulls, fill_null.

use crate::dataframe::DataFrame;
use crate::error::{FrameError, FrameResult};
use crate::series::{ColumnData, Series};
use crate::validity::Validity;

#[derive(Clone, Debug)]
pub enum FillStrategy {
    ValueF64(f64),
    ValueI64(i64),
    ValueStr(String),
    ValueBool(bool),
    Forward,
    Backward,
    Mean,
}

pub fn is_null(df: &DataFrame, col: &str) -> FrameResult<Series> {
    Ok(df.get(col)?.is_null_mask())
}

pub fn drop_nulls(df: &DataFrame, subset: Option<&[&str]>) -> FrameResult<DataFrame> {
    let cols: Vec<&Series> = if let Some(names) = subset {
        let mut v = Vec::new();
        for n in names {
            v.push(df.get(n)?);
        }
        v
    } else {
        df.columns.iter().collect()
    };
    let mut keep = Vec::new();
    for i in 0..df.nrows() {
        let any_null = cols.iter().any(|c| c.validity.is_null(i));
        if !any_null {
            keep.push(i);
        }
    }
    df.take_rows(&keep)
}

pub fn fill_null(df: &DataFrame, col: &str, strategy: FillStrategy) -> FrameResult<DataFrame> {
    let s = df.get(col)?;
    let filled = fill_series(s, strategy)?;
    df.with_column(filled)
}

pub fn fill_series(s: &Series, strategy: FillStrategy) -> FrameResult<Series> {
    match strategy {
        FillStrategy::Forward => ffill(s),
        FillStrategy::Backward => bfill(s),
        FillStrategy::Mean => fill_mean(s),
        FillStrategy::ValueF64(v) => fill_value_f64(s, v),
        FillStrategy::ValueI64(v) => fill_value_i64(s, v),
        FillStrategy::ValueStr(v) => fill_value_str(s, &v),
        FillStrategy::ValueBool(v) => fill_value_bool(s, v),
    }
}

fn fill_value_f64(s: &Series, fill: f64) -> FrameResult<Series> {
    match &s.data {
        ColumnData::F64(v) => {
            let mut out = v.clone();
            for i in 0..out.len() {
                if s.validity.is_null(i) || out[i].is_nan() {
                    out[i] = fill;
                }
            }
            Ok(Series::from_f64(s.name.clone(), out))
        }
        ColumnData::I64(v) => {
            let mut out: Vec<f64> = v.iter().map(|&x| x as f64).collect();
            for i in 0..out.len() {
                if s.validity.is_null(i) {
                    out[i] = fill;
                }
            }
            Ok(Series::from_f64(s.name.clone(), out))
        }
        _ => Err(FrameError::Dtype(
            "fill_null f64 value requires numeric series".into(),
        )),
    }
}

fn fill_value_i64(s: &Series, fill: i64) -> FrameResult<Series> {
    match &s.data {
        ColumnData::I64(v) | ColumnData::Date(v) => {
            let mut out = v.clone();
            for i in 0..out.len() {
                if s.validity.is_null(i) {
                    out[i] = fill;
                }
            }
            let data = if matches!(s.data, ColumnData::Date(_)) {
                ColumnData::Date(out)
            } else {
                ColumnData::I64(out)
            };
            Ok(Series::new(s.name.clone(), data))
        }
        _ => Err(FrameError::Dtype(
            "fill_null i64 value requires i64/date series".into(),
        )),
    }
}

fn fill_value_str(s: &Series, fill: &str) -> FrameResult<Series> {
    match &s.data {
        ColumnData::Str(v) => {
            let mut parts = Vec::with_capacity(v.len());
            for i in 0..v.len() {
                if s.validity.is_null(i) {
                    parts.push(fill.to_string());
                } else {
                    parts.push(v.get(i).to_string());
                }
            }
            Ok(Series::from_str(s.name.clone(), &parts))
        }
        _ => Err(FrameError::Dtype(
            "fill_null str value requires string series".into(),
        )),
    }
}

fn fill_value_bool(s: &Series, fill: bool) -> FrameResult<Series> {
    match &s.data {
        ColumnData::Bool(v) => {
            let mut out = v.clone();
            for i in 0..out.len() {
                if s.validity.is_null(i) {
                    out[i] = fill;
                }
            }
            Ok(Series::from_bool(s.name.clone(), out))
        }
        _ => Err(FrameError::Dtype(
            "fill_null bool value requires bool series".into(),
        )),
    }
}

fn fill_mean(s: &Series) -> FrameResult<Series> {
    let vals = s.to_f64_vec()?;
    let mut sum = 0.0;
    let mut cnt = 0usize;
    for i in 0..vals.len() {
        if s.validity.is_valid(i) && !vals[i].is_nan() {
            sum += vals[i];
            cnt += 1;
        }
    }
    if cnt == 0 {
        return Err(FrameError::Error("fill_null mean: no valid values".into()));
    }
    let mean = sum / cnt as f64;
    fill_value_f64(s, mean)
}

fn ffill(s: &Series) -> FrameResult<Series> {
    let mut validity = Validity::all_valid(s.len());
    match &s.data {
        ColumnData::F64(v) => {
            let mut out = v.clone();
            let mut last = None;
            for i in 0..out.len() {
                if s.validity.is_valid(i) && !out[i].is_nan() {
                    last = Some(out[i]);
                } else if let Some(x) = last {
                    out[i] = x;
                } else {
                    validity.set_null(i);
                }
            }
            Series::from_f64(s.name.clone(), out).with_validity(validity)
        }
        ColumnData::I64(v) | ColumnData::Date(v) => {
            let mut out = v.clone();
            let mut last = None;
            for i in 0..out.len() {
                if s.validity.is_valid(i) {
                    last = Some(out[i]);
                } else if let Some(x) = last {
                    out[i] = x;
                } else {
                    validity.set_null(i);
                }
            }
            let data = if matches!(s.data, ColumnData::Date(_)) {
                ColumnData::Date(out)
            } else {
                ColumnData::I64(out)
            };
            Series {
                name: s.name.clone(),
                data,
                validity,
            }
            .pipe_ok()
        }
        ColumnData::Str(v) => {
            let mut parts = Vec::with_capacity(v.len());
            let mut last: Option<String> = None;
            for i in 0..v.len() {
                if s.validity.is_valid(i) {
                    let t = v.get(i).to_string();
                    last = Some(t.clone());
                    parts.push(t);
                } else if let Some(ref x) = last {
                    parts.push(x.clone());
                } else {
                    parts.push(String::new());
                    validity.set_null(i);
                }
            }
            Series::from_str(s.name.clone(), &parts).with_validity(validity)
        }
        ColumnData::Bool(v) => {
            let mut out = v.clone();
            let mut last = None;
            for i in 0..out.len() {
                if s.validity.is_valid(i) {
                    last = Some(out[i]);
                } else if let Some(x) = last {
                    out[i] = x;
                } else {
                    validity.set_null(i);
                }
            }
            Series::from_bool(s.name.clone(), out).with_validity(validity)
        }
    }
}

fn bfill(s: &Series) -> FrameResult<Series> {
    // reverse, ffill, reverse
    let n = s.len();
    let indices: Vec<usize> = (0..n).rev().collect();
    let rev = s.take(&indices);
    let filled = ffill(&rev)?;
    Ok(filled.take(&indices))
}

trait PipeOk {
    fn pipe_ok(self) -> FrameResult<Series>;
}
impl PipeOk for Series {
    fn pipe_ok(self) -> FrameResult<Series> {
        Ok(self)
    }
}
