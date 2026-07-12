//! Metrics (local stand-ins until `neval` is wired).

use crate::error::{LearnError, LearnResult};
use crate::utils::y_as_vec;
use niao_num::NdArray;

pub fn accuracy(y_true: &NdArray, y_pred: &NdArray) -> LearnResult<f64> {
    let yt = y_as_vec(y_true)?;
    let yp = y_as_vec(y_pred)?;
    if yt.len() != yp.len() {
        return Err(LearnError::Shape("accuracy length mismatch".into()));
    }
    if yt.is_empty() {
        return Ok(0.0);
    }
    let correct = yt
        .iter()
        .zip(yp.iter())
        .filter(|(a, b)| (*a - *b).abs() < 1e-9)
        .count();
    Ok(correct as f64 / yt.len() as f64)
}

pub fn r2_score(y_true: &NdArray, y_pred: &NdArray) -> LearnResult<f64> {
    let yt = y_as_vec(y_true)?;
    let yp = y_as_vec(y_pred)?;
    if yt.len() != yp.len() || yt.is_empty() {
        return Err(LearnError::Shape("r2 length mismatch".into()));
    }
    let mean: f64 = yt.iter().sum::<f64>() / yt.len() as f64;
    let mut ss_res = 0.0;
    let mut ss_tot = 0.0;
    for i in 0..yt.len() {
        let e = yt[i] - yp[i];
        ss_res += e * e;
        let d = yt[i] - mean;
        ss_tot += d * d;
    }
    if ss_tot < 1e-15 {
        return Ok(1.0);
    }
    Ok(1.0 - ss_res / ss_tot)
}

pub fn mse(y_true: &NdArray, y_pred: &NdArray) -> LearnResult<f64> {
    let yt = y_as_vec(y_true)?;
    let yp = y_as_vec(y_pred)?;
    if yt.len() != yp.len() || yt.is_empty() {
        return Err(LearnError::Shape("mse length mismatch".into()));
    }
    let s: f64 = yt
        .iter()
        .zip(yp.iter())
        .map(|(a, b)| {
            let e = a - b;
            e * e
        })
        .sum();
    Ok(s / yt.len() as f64)
}
