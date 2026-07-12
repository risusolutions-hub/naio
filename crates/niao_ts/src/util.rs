//! Shared kernels: Levinson recursion, information criteria, parameter transforms.

use crate::error::{TsError, TsResult};

#[inline]
pub fn mean(x: &[f64]) -> TsResult<f64> {
    if x.is_empty() {
        return Err(TsError::Domain("empty series".into()));
    }
    Ok(x.iter().sum::<f64>() / x.len() as f64)
}

#[inline]
pub fn var(x: &[f64], ddof: usize) -> TsResult<f64> {
    if x.len() <= ddof {
        return Err(TsError::Domain("var: insufficient length".into()));
    }
    let m = mean(x)?;
    let s: f64 = x.iter().map(|v| (v - m).powi(2)).sum();
    Ok(s / (x.len() - ddof) as f64)
}

/// Levinson–Durbin: solve Toeplitz system for Yule–Walker / PACF.
#[inline]
pub fn levinson(r: &[f64], p: usize) -> TsResult<(Vec<f64>, f64)> {
    if p == 0 {
        return Ok((Vec::new(), r.first().copied().unwrap_or(0.0)));
    }
    if r.len() <= p {
        return Err(TsError::Domain("levinson: insufficient lags".into()));
    }
    let mut phi = vec![0.0; p];
    let mut pacf = vec![0.0; p];
    let mut sigma = r[0];
    if sigma <= 0.0 {
        return Err(TsError::NonStationary("zero or negative variance".into()));
    }

    for k in 0..p {
        let mut num = r[k + 1];
        for j in 0..k {
            num -= phi[j] * r[k - j];
        }
        let den = sigma;
        if den.abs() < 1e-15 {
            return Err(TsError::NonStationary("singular Toeplitz system".into()));
        }
        let kk = num / den;
        pacf[k] = kk;
        let mut phi_new = vec![0.0; k + 1];
        phi_new[k] = kk;
        for j in 0..k {
            phi_new[j] = phi[j] - kk * phi[k - 1 - j];
        }
        phi = phi_new;
        sigma *= 1.0 - kk * kk;
        if sigma <= 0.0 {
            return Err(TsError::NonStationary("non-positive innovation variance".into()));
        }
    }
    Ok((phi, sigma))
}

#[inline]
pub fn aic(log_likelihood: f64, k: usize, n: usize) -> f64 {
    -2.0 * log_likelihood + 2.0 * k as f64
}

#[inline]
pub fn bic(log_likelihood: f64, k: usize, n: usize) -> f64 {
    -2.0 * log_likelihood + (k as f64) * (n as f64).ln()
}

#[inline]
pub fn aicc(log_likelihood: f64, k: usize, n: usize) -> f64 {
    let a = aic(log_likelihood, k, n);
    if n - k - 1 <= 0 {
        a
    } else {
        a + (2.0 * k as f64 * (k + 1) as f64) / (n - k - 1) as f64
    }
}

/// Map unconstrained → (-1, 1) for stationarity.
#[inline]
pub fn sigmoid_bounded(u: f64) -> f64 {
    2.0 / (1.0 + (-u).exp()) - 1.0
}

#[inline]
pub fn next_pow2(n: usize) -> usize {
    1usize << (usize::BITS - (n.saturating_sub(1)).leading_zeros())
}

pub fn close(a: f64, b: f64, rtol: f64) -> bool {
    (a - b).abs() <= rtol * b.abs().max(a.abs()).max(1.0) + 1e-12
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levinson_ar1() {
        let r = vec![1.0, 0.8, 0.64, 0.512];
        let (phi, sigma) = levinson(&r, 1).unwrap();
        assert!((phi[0] - 0.8).abs() < 1e-10);
        assert!((sigma - 0.36).abs() < 1e-10);
    }
}
