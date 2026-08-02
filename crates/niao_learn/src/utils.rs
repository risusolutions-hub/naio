//! Shared matrix helpers (row-major f64 views over NdArray).

use crate::error::{LearnError, LearnResult};
use niao_num::NdArray;

#[inline]
pub fn check_2d(x: &NdArray, name: &str) -> LearnResult<(usize, usize)> {
    if x.ndim() != 2 {
        return Err(LearnError::Shape(format!("{name} must be 2-D")));
    }
    Ok((x.shape[0], x.shape[1]))
}

#[inline]
pub fn check_xy(x: &NdArray, y: &NdArray) -> LearnResult<(usize, usize)> {
    let (n, d) = check_2d(x, "X")?;
    let ny = if y.ndim() == 1 {
        y.shape[0]
    } else if y.ndim() == 2 && y.shape[1] == 1 {
        y.shape[0]
    } else if y.ndim() == 2 {
        y.shape[0]
    } else {
        return Err(LearnError::Shape("y must be 1-D or 2-D".into()));
    };
    if n != ny {
        return Err(LearnError::Shape(format!(
            "X/y row mismatch: X has {n} rows, y has {ny}"
        )));
    }
    Ok((n, d))
}

pub fn row_slice<'a>(data: &'a [f64], i: usize, d: usize) -> &'a [f64] {
    &data[i * d..(i + 1) * d]
}

pub fn matrix_from(shape: (usize, usize), data: Vec<f64>) -> LearnResult<NdArray> {
    NdArray::from_vec(vec![shape.0, shape.1], data).map_err(|e| LearnError::Error(e.to_string()))
}

pub fn vector_from(data: Vec<f64>) -> LearnResult<NdArray> {
    let n = data.len();
    NdArray::from_vec(vec![n], data).map_err(|e| LearnError::Error(e.to_string()))
}

pub fn y_as_vec(y: &NdArray) -> LearnResult<Vec<f64>> {
    if y.ndim() == 1 {
        Ok(y.to_vec())
    } else if y.ndim() == 2 && y.shape[1] == 1 {
        Ok(y.to_vec())
    } else if y.ndim() == 2 {
        Ok(y.to_vec())
    } else {
        Err(LearnError::Shape("y must be 1-D or column vector".into()))
    }
}

/// Unique sorted class labels from y.
pub fn unique_labels(y: &[f64]) -> Vec<f64> {
    let mut v = y.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    v.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
    v
}

pub fn label_index(labels: &[f64], y: f64) -> Option<usize> {
    labels.iter().position(|&c| (c - y).abs() < 1e-12)
}

#[inline]
pub fn squared_dist(a: &[f64], b: &[f64]) -> f64 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| {
            let d = x - y;
            d * d
        })
        .sum()
}

#[inline]
pub fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

#[inline]
pub fn sigmoid(z: f64) -> f64 {
    if z >= 0.0 {
        let e = (-z).exp();
        1.0 / (1.0 + e)
    } else {
        let e = z.exp();
        e / (1.0 + e)
    }
}

/// Softmax in-place on a row.
pub fn softmax_inplace(row: &mut [f64]) {
    let m = row.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let mut s = 0.0;
    for v in row.iter_mut() {
        *v = (*v - m).exp();
        s += *v;
    }
    if s > 0.0 {
        for v in row.iter_mut() {
            *v /= s;
        }
    }
}

/// Fix PCA / SVD component signs: make the entry with largest |value| positive.
pub fn fix_component_sign(comp: &mut [f64]) {
    let mut imax = 0usize;
    let mut amax = 0.0f64;
    for (i, &v) in comp.iter().enumerate() {
        let a = v.abs();
        if a > amax {
            amax = a;
            imax = i;
        }
    }
    if comp[imax] < 0.0 {
        for v in comp.iter_mut() {
            *v = -*v;
        }
    }
}

pub fn mean_axis0(data: &[f64], n: usize, d: usize) -> Vec<f64> {
    let mut m = vec![0.0; d];
    for i in 0..n {
        for j in 0..d {
            m[j] += data[i * d + j];
        }
    }
    for j in 0..d {
        m[j] /= n as f64;
    }
    m
}

pub fn std_axis0(data: &[f64], mean: &[f64], n: usize, d: usize, ddof: usize) -> Vec<f64> {
    let mut v = vec![0.0; d];
    for i in 0..n {
        for j in 0..d {
            let diff = data[i * d + j] - mean[j];
            v[j] += diff * diff;
        }
    }
    let denom = (n.saturating_sub(ddof)).max(1) as f64;
    for j in 0..d {
        v[j] = (v[j] / denom).sqrt();
        if v[j] < 1e-12 {
            v[j] = 1.0;
        }
    }
    v
}

pub fn design_with_intercept(x: &[f64], n: usize, d: usize) -> Vec<f64> {
    let mut out = vec![0.0; n * (d + 1)];
    for i in 0..n {
        out[i * (d + 1)] = 1.0;
        for j in 0..d {
            out[i * (d + 1) + 1 + j] = x[i * d + j];
        }
    }
    out
}
