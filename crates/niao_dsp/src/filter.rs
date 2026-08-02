//! Filter application: lfilter, filtfilt, sosfilt.

use crate::error::{DspError, DspResult};
use crate::iir::Sos;

/// Direct Form II Transposed IIR/FIR filter.
pub fn lfilter(b: &[f64], a: &[f64], x: &[f64]) -> DspResult<Vec<f64>> {
    if b.is_empty() || a.is_empty() {
        return Err(DspError::Filter("b and a must be non-empty".into()));
    }
    if a[0].abs() < 1e-30 {
        return Err(DspError::Filter("a[0] must be non-zero".into()));
    }
    let mut bb: Vec<f64> = b.iter().map(|v| v / a[0]).collect();
    let mut aa: Vec<f64> = a.iter().map(|v| v / a[0]).collect();
    aa[0] = 1.0;

    let n = x.len();
    let mut y = vec![0.0; n];
    let order = bb.len().max(aa.len()).saturating_sub(1);
    let mut z = vec![0.0; order.max(1)];

    // Pad shorter coefficient vectors
    while bb.len() < order + 1 {
        bb.push(0.0);
    }
    while aa.len() < order + 1 {
        aa.push(0.0);
    }

    for i in 0..n {
        let xi = x[i];
        y[i] = bb[0] * xi + z[0];
        for j in 0..order.saturating_sub(1) {
            z[j] = bb[j + 1] * xi + z[j + 1] - aa[j + 1] * y[i];
        }
        if order > 0 {
            z[order - 1] = bb[order] * xi - aa[order] * y[i];
        }
    }
    Ok(y)
}

fn odd_ext(x: &[f64], n: usize) -> DspResult<Vec<f64>> {
    if x.len() < 2 {
        return Err(DspError::Length(
            "filtfilt requires signal length >= 2".into(),
        ));
    }
    if n == 0 {
        return Ok(x.to_vec());
    }
    if n >= x.len() {
        return Err(DspError::Length("pad length too large for signal".into()));
    }
    let mut left = Vec::with_capacity(n);
    for i in 0..n {
        left.push(2.0 * x[0] - x[n - i]);
    }
    let mut right = Vec::with_capacity(n);
    let last = x.len() - 1;
    for i in 0..n {
        right.push(2.0 * x[last] - x[last - 1 - i]);
    }
    let mut out = Vec::with_capacity(x.len() + 2 * n);
    out.extend(left);
    out.extend_from_slice(x);
    out.extend(right);
    Ok(out)
}

/// Zero-phase forward-backward filtering.
pub fn filtfilt(b: &[f64], a: &[f64], x: &[f64]) -> DspResult<Vec<f64>> {
    if x.is_empty() {
        return Ok(vec![]);
    }
    let max_pad = x.len().saturating_sub(1);
    let want = (3 * b.len().max(a.len()).saturating_sub(1)).max(1);
    let pad = want.min(max_pad);
    let ext = odd_ext(x, pad)?;
    let y = lfilter(b, a, &ext)?;
    let yr: Vec<f64> = y.into_iter().rev().collect();
    let y2 = lfilter(b, a, &yr)?;
    let y2r: Vec<f64> = y2.into_iter().rev().collect();
    Ok(y2r[pad..pad + x.len()].to_vec())
}

pub fn sosfilt(sos: &Sos, x: &[f64]) -> DspResult<Vec<f64>> {
    if sos.is_empty() {
        return Err(DspError::Filter("sos must be non-empty".into()));
    }
    let mut y = x.to_vec();
    for sec in sos {
        let b = &sec[0..3];
        let a = &sec[3..6];
        y = lfilter(b, a, &y)?;
    }
    Ok(y)
}

pub fn sosfiltfilt(sos: &Sos, x: &[f64]) -> DspResult<Vec<f64>> {
    if x.is_empty() {
        return Ok(vec![]);
    }
    let y = sosfilt(sos, x)?;
    let yr: Vec<f64> = y.into_iter().rev().collect();
    let y2 = sosfilt(sos, &yr)?;
    Ok(y2.into_iter().rev().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fir_lfilter_moving_average() {
        let b = [1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0];
        let a = [1.0];
        let x = [1.0, 1.0, 1.0, 1.0];
        let y = lfilter(&b, &a, &x).unwrap();
        assert!((y[2] - 1.0).abs() < 1e-12);
    }
}
