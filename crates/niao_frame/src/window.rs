//! Rolling / window ops: rolling mean/sum/std, cumsum, shift, diff, rank.

use crate::error::{FrameError, FrameResult};
use crate::series::{ColumnData, Series};
use crate::validity::Validity;

pub struct Rolling<'a> {
    series: &'a Series,
    window: usize,
}

impl<'a> Rolling<'a> {
    pub fn new(series: &'a Series, window: usize) -> FrameResult<Self> {
        if window == 0 {
            return Err(FrameError::Error("rolling window must be > 0".into()));
        }
        Ok(Self { series, window })
    }

    pub fn mean(&self) -> FrameResult<Series> {
        self.rolling_reduce(|slice| slice.iter().sum::<f64>() / slice.len() as f64, "mean")
    }

    pub fn sum(&self) -> FrameResult<Series> {
        self.rolling_reduce(|slice| slice.iter().sum(), "sum")
    }

    pub fn std(&self) -> FrameResult<Series> {
        self.rolling_reduce(
            |slice| {
                let n = slice.len() as f64;
                if n < 2.0 {
                    return 0.0;
                }
                let mean = slice.iter().sum::<f64>() / n;
                let ss: f64 = slice.iter().map(|x| (x - mean).powi(2)).sum();
                (ss / (n - 1.0)).sqrt()
            },
            "std",
        )
    }

    fn rolling_reduce<F>(&self, f: F, name: &str) -> FrameResult<Series>
    where
        F: Fn(&[f64]) -> f64,
    {
        let vals = self.series.to_f64_vec()?;
        let n = vals.len();
        let w = self.window;
        let mut out = vec![f64::NAN; n];
        let mut validity = Validity::all_valid(n);
        for i in 0..n {
            if i + 1 < w {
                validity.set_null(i);
                continue;
            }
            let start = i + 1 - w;
            let mut window_vals = Vec::with_capacity(w);
            let mut ok = true;
            for j in start..=i {
                if self.series.validity.is_null(j) || vals[j].is_nan() {
                    ok = false;
                    break;
                }
                window_vals.push(vals[j]);
            }
            if ok {
                out[i] = f(&window_vals);
            } else {
                validity.set_null(i);
            }
        }
        Series::from_f64(format!("{}_{name}", self.series.name), out).with_validity(validity)
    }
}

pub fn cumsum(s: &Series) -> FrameResult<Series> {
    let vals = s.to_f64_vec()?;
    let mut out = Vec::with_capacity(vals.len());
    let mut acc = 0.0;
    let mut validity = Validity::all_valid(vals.len());
    for i in 0..vals.len() {
        if s.validity.is_null(i) || vals[i].is_nan() {
            out.push(f64::NAN);
            validity.set_null(i);
        } else {
            acc += vals[i];
            out.push(acc);
        }
    }
    Series::from_f64(format!("{}_cumsum", s.name), out).with_validity(validity)
}

pub fn cumcount(s: &Series) -> Series {
    let mut out = Vec::with_capacity(s.len());
    let mut c = 0i64;
    for i in 0..s.len() {
        if s.validity.is_valid(i) {
            c += 1;
        }
        out.push(c);
    }
    Series::from_i64(format!("{}_cumcount", s.name), out)
}

pub fn shift(s: &Series, periods: i64) -> Series {
    let n = s.len();
    let mut validity = Validity::all_valid(n);
    let indices: Vec<Option<usize>> = (0..n)
        .map(|i| {
            let src = i as i64 - periods;
            if src < 0 || src >= n as i64 {
                validity.set_null(i);
                None
            } else {
                let src = src as usize;
                if s.validity.is_null(src) {
                    validity.set_null(i);
                }
                Some(src)
            }
        })
        .collect();

    let data = match &s.data {
        ColumnData::I64(v) => {
            let mut out = vec![0i64; n];
            for (i, src) in indices.iter().enumerate() {
                if let Some(j) = src {
                    out[i] = v[*j];
                }
            }
            ColumnData::I64(out)
        }
        ColumnData::F64(v) => {
            let mut out = vec![f64::NAN; n];
            for (i, src) in indices.iter().enumerate() {
                if let Some(j) = src {
                    out[i] = v[*j];
                }
            }
            ColumnData::F64(out)
        }
        ColumnData::Bool(v) => {
            let mut out = vec![false; n];
            for (i, src) in indices.iter().enumerate() {
                if let Some(j) = src {
                    out[i] = v[*j];
                }
            }
            ColumnData::Bool(out)
        }
        ColumnData::Date(v) => {
            let mut out = vec![0i64; n];
            for (i, src) in indices.iter().enumerate() {
                if let Some(j) = src {
                    out[i] = v[*j];
                }
            }
            ColumnData::Date(out)
        }
        ColumnData::Str(v) => {
            let mut out = crate::series::StringColumn::new();
            for src in &indices {
                if let Some(j) = src {
                    out.push(v.get(*j));
                } else {
                    out.push("");
                }
            }
            ColumnData::Str(out)
        }
    };
    Series {
        name: format!("{}_shift", s.name),
        data,
        validity,
    }
}

pub fn diff(s: &Series, periods: usize) -> FrameResult<Series> {
    let vals = s.to_f64_vec()?;
    let n = vals.len();
    let mut out = vec![f64::NAN; n];
    let mut validity = Validity::all_valid(n);
    for i in 0..n {
        if i < periods {
            validity.set_null(i);
            continue;
        }
        let j = i - periods;
        if s.validity.is_null(i) || s.validity.is_null(j) {
            validity.set_null(i);
        } else {
            out[i] = vals[i] - vals[j];
        }
    }
    Series::from_f64(format!("{}_diff", s.name), out).with_validity(validity)
}

/// Average rank (1-based), nulls stay null. Ties get average rank.
pub fn rank(s: &Series) -> FrameResult<Series> {
    let vals = s.to_f64_vec()?;
    let n = vals.len();
    let mut order: Vec<usize> = (0..n)
        .filter(|&i| s.validity.is_valid(i) && !vals[i].is_nan())
        .collect();
    order.sort_by(|&a, &b| vals[a].partial_cmp(&vals[b]).unwrap());

    let mut out = vec![f64::NAN; n];
    let mut validity = Validity::all_valid(n);
    for i in 0..n {
        if s.validity.is_null(i) || vals[i].is_nan() {
            validity.set_null(i);
        }
    }

    let mut i = 0;
    while i < order.len() {
        let mut j = i + 1;
        while j < order.len() && (vals[order[j]] - vals[order[i]]).abs() < 1e-15 {
            j += 1;
        }
        // ranks i+1 ..= j (1-based)
        let avg = ((i + 1) + j) as f64 / 2.0;
        for k in i..j {
            out[order[k]] = avg;
        }
        i = j;
    }
    Series::from_f64(format!("{}_rank", s.name), out).with_validity(validity)
}
