//! Classical seasonal decomposition (additive / multiplicative).

use crate::error::{TsError, TsResult};
use crate::util::mean;

#[derive(Debug, Clone)]
pub struct DecomposeResult {
    pub observed: Vec<f64>,
    pub trend: Vec<f64>,
    pub seasonal: Vec<f64>,
    pub resid: Vec<f64>,
    pub period: usize,
}

fn moving_average(x: &[f64], window: usize) -> TsResult<Vec<f64>> {
    let n = x.len();
    if window == 0 || window > n {
        return Err(TsError::Domain("moving_average: bad window".into()));
    }
    let half = window / 2;
    let mut out = vec![f64::NAN; n];
    for t in 0..n {
        let lo = t.saturating_sub(half);
        let hi = (t + half + 1).min(n);
        let slice = &x[lo..hi];
        if slice.len() == window || (t >= half && t + half < n) {
            out[t] = slice.iter().sum::<f64>() / slice.len() as f64;
        } else {
            // edge: use available
            out[t] = slice.iter().sum::<f64>() / slice.len() as f64;
        }
    }
    // center 2*half+1 MA
    if window % 2 == 0 {
        for t in 0..n.saturating_sub(1) {
            if !out[t].is_nan() && !out[t + 1].is_nan() {
                out[t] = (out[t] + out[t + 1]) / 2.0;
            }
        }
    }
    Ok(out)
}

/// Classical seasonal decomposition.
pub fn seasonal_decompose(
    x: &[f64],
    period: usize,
    multiplicative: bool,
) -> TsResult<DecomposeResult> {
    let n = x.len();
    if period < 2 || n < 2 * period {
        return Err(TsError::Domain(
            "seasonal_decompose: need length >= 2*period".into(),
        ));
    }

    let trend_raw = moving_average(x, period)?;
    let mut trend = trend_raw;
    // fill NaN at edges by linear extrapolation
    if trend[0].is_nan() {
        let first = trend.iter().position(|v| !v.is_nan()).unwrap_or(0);
        let last = trend.iter().rposition(|v| !v.is_nan()).unwrap_or(n - 1);
        for t in 0..first {
            trend[t] = trend[first];
        }
        for t in (last + 1)..n {
            trend[t] = trend[last];
        }
    }

    let mut detrended = vec![0.0; n];
    for t in 0..n {
        detrended[t] = if multiplicative {
            if trend[t].abs() < 1e-15 {
                1.0
            } else {
                x[t] / trend[t]
            }
        } else {
            x[t] - trend[t]
        };
    }

    let mut seasonal = vec![0.0; n];
    let mut pattern = vec![0.0; period];
    let mut counts = vec![0usize; period];
    for t in 0..n {
        let idx = t % period;
        pattern[idx] += detrended[t];
        counts[idx] += 1;
    }
    for i in 0..period {
        if counts[i] > 0 {
            pattern[i] /= counts[i] as f64;
        }
    }
    if multiplicative {
        let m = mean(&pattern)?;
        if m.abs() > 1e-15 {
            for v in &mut pattern {
                *v /= m;
            }
        }
    } else {
        let m = mean(&pattern)?;
        for v in &mut pattern {
            *v -= m;
        }
    }
    for t in 0..n {
        seasonal[t] = pattern[t % period];
    }

    let mut resid = vec![0.0; n];
    for t in 0..n {
        resid[t] = if multiplicative {
            x[t] / (trend[t] * seasonal[t])
        } else {
            x[t] - trend[t] - seasonal[t]
        };
    }

    Ok(DecomposeResult {
        observed: x.to_vec(),
        trend,
        seasonal,
        resid,
        period,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn additive_decompose() {
        let n = 24;
        let x: Vec<f64> = (0..n)
            .map(|t| {
                let trend = t as f64 * 0.5;
                let season = if t % 4 < 2 { 1.0 } else { -1.0 };
                trend + season
            })
            .collect();
        let d = seasonal_decompose(&x, 4, false).unwrap();
        assert_eq!(d.observed.len(), n);
        for t in 0..n {
            let recon = d.trend[t] + d.seasonal[t] + d.resid[t];
            assert!((recon - x[t]).abs() < 0.5);
        }
    }
}
