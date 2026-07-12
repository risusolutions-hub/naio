//! Autocorrelation diagnostics, stationarity tests, differencing.

use crate::error::{TsError, TsResult};
use crate::util::{mean, next_pow2, var};
use niao_num::{fft, from_slice, ifft, Complex};
use niao_stats::dist::{ChiSquare, Normal};

#[derive(Debug, Clone)]
pub struct TestResult {
    pub statistic: f64,
    pub pvalue: f64,
    pub lags: usize,
}

/// Autocorrelation function via FFT-based autocovariance.
pub fn acf(x: &[f64], nlags: Option<usize>) -> TsResult<Vec<f64>> {
    let n = x.len();
    if n < 2 {
        return Err(TsError::Domain("acf: need at least 2 observations".into()));
    }
    let maxlag = nlags.unwrap_or(n / 4).min(n - 1);
    let mu = mean(x)?;
    let xc: Vec<f64> = x.iter().map(|v| v - mu).collect();

    let m = next_pow2(2 * n);
    let mut padded = vec![Complex::from_real(0.0); m];
    for (i, &v) in xc.iter().enumerate() {
        padded[i] = Complex::from_real(v);
    }
    let arr = from_slice(&[m], &padded.iter().map(|c| c.re).collect::<Vec<_>>())
        .map_err(|e| TsError::Error(e.to_string()))?;
    let spec = fft(&arr).map_err(|e| TsError::Error(e.to_string()))?;
    let power: Vec<Complex> = spec.iter().map(|c| Complex::new(c.re * c.re + c.im * c.im, 0.0)).collect();
    let cov = ifft(&power).map_err(|e| TsError::Error(e.to_string()))?;

    let mut acf_vals = vec![1.0; maxlag + 1];
    let r0 = cov[0].re / n as f64;
    if r0.abs() < 1e-15 {
        return Ok(acf_vals);
    }
    for k in 1..=maxlag {
        acf_vals[k] = cov[k].re / (n as f64 * r0);
    }
    Ok(acf_vals)
}

/// Partial autocorrelation via Durbin–Levinson on ACF.
pub fn pacf(x: &[f64], nlags: Option<usize>) -> TsResult<Vec<f64>> {
    let n = x.len();
    if n < 2 {
        return Err(TsError::Domain("pacf: need at least 2 observations".into()));
    }
    let maxlag = nlags.unwrap_or(n / 4).min(n - 1);
    let acf_full = acf(x, Some(maxlag))?;
    let mut out = vec![1.0; maxlag + 1];
    let mut phi_prev = Vec::new();
    let mut sigma = acf_full[0];

    for k in 1..=maxlag {
        let mut num = acf_full[k];
        for (j, &p) in phi_prev.iter().enumerate() {
            num -= p * acf_full[k - 1 - j];
        }
        let kk = if sigma.abs() < 1e-15 {
            0.0
        } else {
            num / sigma
        };
        out[k] = kk;
        let mut phi_new = vec![0.0; k];
        phi_new[k - 1] = kk;
        for j in 0..(k - 1) {
            phi_new[j] = phi_prev[j] - kk * phi_prev[k - 2 - j];
        }
        phi_prev = phi_new;
        sigma *= 1.0 - kk * kk;
    }
    Ok(out)
}

pub fn diff(x: &[f64], periods: usize) -> TsResult<Vec<f64>> {
    if periods == 0 {
        return Ok(x.to_vec());
    }
    if x.len() <= periods {
        return Err(TsError::Domain("diff: insufficient length".into()));
    }
    Ok(x[periods..]
        .iter()
        .zip(&x[..x.len() - periods])
        .map(|(&a, &b)| a - b)
        .collect())
}

pub fn seasonal_diff(x: &[f64], seasonal: usize, periods: usize) -> TsResult<Vec<f64>> {
    if seasonal == 0 {
        return diff(x, periods);
    }
    diff(x, seasonal * periods.max(1))
}

/// Lag matrix: columns are lagged versions of x.
pub fn lagmat(x: &[f64], maxlag: usize, trim: bool) -> TsResult<Vec<Vec<f64>>> {
    if maxlag == 0 || x.is_empty() {
        return Err(TsError::Domain("lagmat: invalid parameters".into()));
    }
    let n = x.len();
    let start = if trim { maxlag } else { 0 };
    let rows = n - start;
    let mut out = vec![vec![0.0; maxlag]; rows];
    for i in 0..rows {
        let t = start + i;
        for l in 0..maxlag {
            out[i][l] = x[t - l - 1];
        }
    }
    Ok(out)
}

fn ols_simple(x: &[Vec<f64>], y: &[f64]) -> Result<(f64, f64), String> {
    let n = y.len();
    if n == 0 || x.len() != n || x[0].len() < 2 {
        return Err("ols_simple: bad shape".into());
    }
    let p = x[0].len();
    // Normal equations for small p (ADF uses p<= few lags)
    let mut xtx = vec![0.0; p * p];
    let mut xty = vec![0.0; p];
    for i in 0..n {
        for a in 0..p {
            xty[a] += x[i][a] * y[i];
            for b in 0..p {
                xtx[a * p + b] += x[i][a] * x[i][b];
            }
        }
    }
    let beta = solve_sym(xtx.clone(), xty, p)?;
    let rho = beta[1];
    let mut rss = 0.0;
    for i in 0..n {
        let mut pred = 0.0;
        for a in 0..p {
            pred += beta[a] * x[i][a];
        }
        let e = y[i] - pred;
        rss += e * e;
    }
    let df = (n - p) as f64;
    let mse = rss / df;
    // SE of rho: mse * (X'X)^{-1}[1,1]
    let mut xtx_inv = invert_small(xtx, p)?;
    let se = (mse * xtx_inv[p + 1]).sqrt();
    Ok((rho, se))
}

fn solve_sym(a: Vec<f64>, b: Vec<f64>, n: usize) -> Result<Vec<f64>, String> {
    let mut aug = vec![0.0; n * (n + 1)];
    for i in 0..n {
        for j in 0..n {
            aug[i * (n + 1) + j] = a[i * n + j];
        }
        aug[i * (n + 1) + n] = b[i];
    }
    for col in 0..n {
        let mut piv = col;
        for row in (col + 1)..n {
            if aug[row * (n + 1) + col].abs() > aug[piv * (n + 1) + col].abs() {
                piv = row;
            }
        }
        if aug[piv * (n + 1) + col].abs() < 1e-15 {
            return Err("singular matrix".into());
        }
        if piv != col {
            for j in 0..=n {
                aug.swap(col * (n + 1) + j, piv * (n + 1) + j);
            }
        }
        let div = aug[col * (n + 1) + col];
        for j in col..=n {
            aug[col * (n + 1) + j] /= div;
        }
        for row in 0..n {
            if row == col {
                continue;
            }
            let f = aug[row * (n + 1) + col];
            for j in col..=n {
                aug[row * (n + 1) + j] -= f * aug[col * (n + 1) + j];
            }
        }
    }
    Ok((0..n).map(|i| aug[i * (n + 1) + n]).collect())
}

fn invert_small(a: Vec<f64>, n: usize) -> Result<Vec<f64>, String> {
    let mut inv = vec![0.0; n * n];
    for col in 0..n {
        let mut e = vec![0.0; n];
        e[col] = 1.0;
        let sol = solve_sym(a.clone(), e, n)?;
        for row in 0..n {
            inv[row * n + col] = sol[row];
        }
    }
    Ok(inv)
}

/// Augmented Dickey–Fuller test (constant, no trend).
pub fn adfuller(x: &[f64], maxlag: Option<usize>) -> TsResult<TestResult> {
    let n = x.len();
    if n < 4 {
        return Err(TsError::Domain("adfuller: need at least 4 observations".into()));
    }
    let p = maxlag.unwrap_or(((n - 1) as f64).powf(1.0 / 3.0).floor() as usize).min(n / 2 - 2);
    let dy = diff(x, 1)?;
    let y_lag = x[..n - 1].to_vec();
    let m = dy.len();
    if m <= p + 2 {
        return Err(TsError::Domain("adfuller: insufficient length after lags".into()));
    }

    let mut design = Vec::with_capacity(m - p);
    let mut y = Vec::with_capacity(m - p);
    for t in (p + 1)..m {
        let mut row = vec![1.0, y_lag[t]];
        for j in 1..=p {
            row.push(dy[t - j]);
        }
        design.push(row);
        y.push(dy[t]);
    }
    let (rho, se) = ols_simple(&design, &y).map_err(|e| TsError::Error(e))?;
    let t_stat = if se > 0.0 { rho / se } else { 0.0 };

    // MacKinnon approximate p-value (constant, no trend) — simplified logistic
    let pvalue = adf_pvalue(t_stat, n);

    Ok(TestResult {
        statistic: t_stat,
        pvalue,
        lags: p,
    })
}

fn adf_pvalue(t_stat: f64, n: usize) -> f64 {
    // Interpolated critical values at 1%, 5%, 10% for constant model (approx)
    let cv_1 = -3.43 - 6.0 / n as f64;
    let cv_5 = -2.86 - 3.0 / n as f64;
    let cv_10 = -2.57 - 1.5 / n as f64;
    if t_stat <= cv_1 {
        0.01
    } else if t_stat <= cv_5 {
        0.05
    } else if t_stat <= cv_10 {
        0.10
    } else {
        (1.0 - Normal::standard().cdf(-t_stat / 2.0)).min(0.99_f64)
    }
}

/// KPSS level-stationarity test.
pub fn kpss(x: &[f64], lags: Option<usize>) -> TsResult<TestResult> {
    let n = x.len();
    if n < 4 {
        return Err(TsError::Domain("kpss: need at least 4 observations".into()));
    }
    let mu = mean(x)?;
    let e: Vec<f64> = x.iter().map(|v| v - mu).collect();
    let mut s = 0.0;
    let mut partial: Vec<f64> = Vec::with_capacity(n);
    for &v in &e {
        s += v;
        partial.push(s);
    }
    let s_sum: f64 = partial.iter().map(|v| v * v).sum();

    let maxlag = lags.unwrap_or((4.0 * n as f64 / 100.0).ceil() as usize + 1);
    let acf_vals = acf(x, Some(maxlag))?;
    let mut s0 = var(x, 0)?;
    let mut long_run = s0;
    for k in 1..=maxlag {
        let w = 1.0 - k as f64 / (maxlag + 1) as f64;
        long_run += 2.0 * w * acf_vals[k] * s0;
    }
    if long_run <= 0.0 {
        long_run = s0;
    }

    let stat = s_sum / (n as f64 * n as f64 * long_run);
    // Asymptotic distribution ~ mixture; approximate with chi-square(2) scaled
    let chi = ChiSquare::new(2.0).map_err(|e| TsError::Error(e.to_string()))?;
    let pvalue = 1.0 - chi.cdf(stat).map_err(|e| TsError::Error(e.to_string()))?;

    Ok(TestResult {
        statistic: stat,
        pvalue,
        lags: maxlag,
    })
}

/// Ljung–Box portmanteau test for white noise.
pub fn ljungbox(x: &[f64], lags: usize) -> TsResult<TestResult> {
    let n = x.len();
    if n < lags + 2 {
        return Err(TsError::Domain("ljungbox: insufficient length".into()));
    }
    let acf_vals = acf(x, Some(lags))?;
    let mut q = 0.0;
    for k in 1..=lags {
        let rk = acf_vals[k];
        q += rk * rk / (n - k) as f64;
    }
    q *= n as f64 * (n as f64 + 1.0);

    let chi = ChiSquare::new(lags as f64).map_err(|e| TsError::Error(e.to_string()))?;
    let pvalue = 1.0 - chi.cdf(q).map_err(|e| TsError::Error(e.to_string()))?;

    Ok(TestResult {
        statistic: q,
        pvalue,
        lags,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::close;

    fn ar2_series(n: usize) -> Vec<f64> {
        let mut y = vec![0.0; n];
        y[0] = 1.0;
        y[1] = 0.5;
        let phi1 = 0.6;
        let phi2 = -0.3;
        for t in 2..n {
            y[t] = phi1 * y[t - 1] + phi2 * y[t - 2];
        }
        y
    }

    #[test]
    fn acf_pacf_ar2() {
        let y = ar2_series(200);
        let acf_v = acf(&y, Some(5)).unwrap();
        assert!((acf_v[0] - 1.0).abs() < 1e-10);
        assert!(acf_v[1].abs() > 0.3);

        let pacf_v = pacf(&y, Some(5)).unwrap();
        assert!((pacf_v[0] - 1.0).abs() < 1e-10);
        assert!(pacf_v[2].abs() > pacf_v[5].abs() * 0.5);
    }

    #[test]
    fn diff_lagmat() {
        let x = vec![1.0, 4.0, 9.0, 16.0];
        let d = diff(&x, 1).unwrap();
        assert_eq!(d, vec![3.0, 5.0, 7.0]);
        let lm = lagmat(&x, 2, true).unwrap();
        assert_eq!(lm.len(), 2);
        assert!((lm[0][0] - 4.0).abs() < 1e-12);
    }

    #[test]
    fn ljungbox_white_noise() {
        let mut rng = 0.12345_f64;
        let noise: Vec<f64> = (0..500)
            .map(|_| {
                rng = (rng * 16807.0) % 2147483647.0;
                (rng / 2147483647.0 - 0.5) * 2.0
            })
            .collect();
        let lb = ljungbox(&noise, 10).unwrap();
        assert!(lb.pvalue > 0.05, "p={}", lb.pvalue);
    }

    #[test]
    fn adfuller_stationary() {
        let mut rng = 0.42;
        let noise: Vec<f64> = (0..120)
            .map(|_| {
                rng = (rng * 16807.0) % 2147483647.0;
                rng / 2147483647.0 - 0.5
            })
            .collect();
        let adf = adfuller(&noise, Some(0)).unwrap();
        assert!(adf.statistic.is_finite());
        assert!(adf.pvalue.is_finite());
    }
}
