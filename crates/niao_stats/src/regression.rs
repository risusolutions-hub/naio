//! Regression summaries: OLS and logistic (IRLS).

use crate::descriptive::{mean, std};
use crate::dist::{StudentT, F};
use crate::error::{StatsError, StatsResult};
use crate::special::norm_ppf;
use niao_num::{from_slice, matmul, NdArray};

#[derive(Debug, Clone)]
pub struct OlsResult {
    pub coefficients: Vec<f64>,
    pub std_errors: Vec<f64>,
    pub t_stats: Vec<f64>,
    pub p_values: Vec<f64>,
    pub r_squared: f64,
    pub adj_r_squared: f64,
    pub f_statistic: f64,
    pub f_pvalue: f64,
    pub ci_lower: Vec<f64>,
    pub ci_upper: Vec<f64>,
    pub residuals: Vec<f64>,
}

pub fn ols(x: &[Vec<f64>], y: &[f64]) -> StatsResult<OlsResult> {
    let n = y.len();
    if n == 0 || x.len() != n {
        return Err(StatsError::Error("ols: length mismatch".into()));
    }
    let p = x[0].len();
    for row in x {
        if row.len() != p {
            return Err(StatsError::Error("ols: ragged design matrix".into()));
        }
    }
    if n <= p {
        return Err(StatsError::Error("ols: n <= p".into()));
    }

    // Design matrix with intercept
    let cols = p + 1;
    let mut design = vec![1.0; n * cols];
    for i in 0..n {
        for j in 0..p {
            design[i * cols + j + 1] = x[i][j];
        }
    }
    let x_mat = from_slice(&[n, cols], &design).map_err(|e| StatsError::Error(e.to_string()))?;

    let beta = lstsq_qr(&design, n, cols, y)?;

    // fitted and residuals
    let xb = matmul_vec(&design, n, cols, &beta);
    let fitted: Vec<f64> = xb;
    let residuals: Vec<f64> = y.iter().zip(&fitted).map(|(&yi, fi)| yi - fi).collect();

    let y_mean = mean(y)?;
    let ss_tot: f64 = y.iter().map(|yi| (yi - y_mean).powi(2)).sum();
    let ss_res: f64 = residuals.iter().map(|r| r * r).sum();
    let r_squared = 1.0 - ss_res / ss_tot;
    let adj_r_squared = 1.0 - (1.0 - r_squared) * (n - 1) as f64 / (n - cols) as f64;

    let df_resid = (n - cols) as f64;
    let mse = ss_res / df_resid;

    // (X'X)^{-1} via normal equations for SE
    let xt = x_mat
        .transpose()
        .map_err(|e| StatsError::Error(e.to_string()))?;
    let xtx = matmul(&xt, &x_mat).map_err(|e| StatsError::Error(e.to_string()))?;
    let xtx_inv = invert(&xtx)?;

    let mut std_errors = Vec::with_capacity(cols);
    let mut t_stats = Vec::with_capacity(cols);
    let mut p_values = Vec::with_capacity(cols);
    let mut ci_lower = Vec::with_capacity(cols);
    let mut ci_upper = Vec::with_capacity(cols);
    let t_crit = norm_ppf(0.975)?; // large-sample; use t for small n
    let t_dist = StudentT::new(df_resid)?;
    let t_q = t_dist.ppf(0.975)?;

    for j in 0..cols {
        let se = (mse * xtx_inv[j][j]).sqrt();
        std_errors.push(se);
        let t = if se > 0.0 { beta[j] / se } else { 0.0 };
        t_stats.push(t);
        let p = 2.0 * t_dist.sf(t.abs())?;
        p_values.push(p);
        ci_lower.push(beta[j] - t_q * se);
        ci_upper.push(beta[j] + t_q * se);
    }

    let ms_reg = (ss_tot - ss_res) / (p as f64);
    let f_stat = if mse > 0.0 { ms_reg / mse } else { 0.0 };
    let f_p = F::new(p as f64, df_resid)?.sf(f_stat)?;

    Ok(OlsResult {
        coefficients: beta,
        std_errors,
        t_stats,
        p_values,
        r_squared,
        adj_r_squared,
        f_statistic: f_stat,
        f_pvalue: f_p,
        ci_lower,
        ci_upper,
        residuals,
    })
}

fn lstsq_qr(design: &[f64], n: usize, cols: usize, y: &[f64]) -> StatsResult<Vec<f64>> {
    let m = n;
    let mut q_cols: Vec<Vec<f64>> = Vec::with_capacity(cols);
    let mut r = vec![0.0; cols * cols];

    for j in 0..cols {
        let mut col: Vec<f64> = (0..m).map(|i| design[i * cols + j]).collect();
        for (i, qi) in q_cols.iter().enumerate() {
            let dot: f64 = col.iter().zip(qi.iter()).map(|(&x, &yv)| x * yv).sum();
            r[i * cols + j] = dot;
            for t in 0..m {
                col[t] -= dot * qi[t];
            }
        }
        let norm: f64 = col.iter().map(|x| x * x).sum::<f64>().sqrt();
        if norm < 1e-15 {
            return Err(StatsError::Error("rank deficient design matrix".into()));
        }
        r[j * cols + j] = norm;
        for t in 0..m {
            col[t] /= norm;
        }
        q_cols.push(col);
    }

    let mut qty = vec![0.0; cols];
    for (i, qi) in q_cols.iter().enumerate() {
        qty[i] = qi.iter().zip(y.iter()).map(|(&q, &yi)| q * yi).sum();
    }

    let mut beta = vec![0.0; cols];
    for i in (0..cols).rev() {
        let mut s = qty[i];
        for j in (i + 1)..cols {
            s -= r[i * cols + j] * beta[j];
        }
        beta[i] = s / r[i * cols + i];
    }
    Ok(beta)
}

fn matmul_vec(design: &[f64], n: usize, cols: usize, beta: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; n];
    for i in 0..n {
        let mut s = 0.0;
        for j in 0..cols {
            s += design[i * cols + j] * beta[j];
        }
        out[i] = s;
    }
    out
}

fn invert(a: &NdArray) -> StatsResult<Vec<Vec<f64>>> {
    let n = a.shape[0];
    let v = a.to_vec();
    let mut aug = vec![0.0; n * 2 * n];
    for i in 0..n {
        for j in 0..n {
            aug[i * 2 * n + j] = v[i * n + j];
        }
        aug[i * 2 * n + n + i] = 1.0;
    }
    for col in 0..n {
        let mut pivot = col;
        let mut max_val = aug[pivot * 2 * n + col].abs();
        for row in (col + 1)..n {
            let val = aug[row * 2 * n + col].abs();
            if val > max_val {
                max_val = val;
                pivot = row;
            }
        }
        if max_val < 1e-15 {
            return Err(StatsError::Error("singular matrix".into()));
        }
        if pivot != col {
            for j in 0..2 * n {
                aug.swap(pivot * 2 * n + j, col * 2 * n + j);
            }
        }
        let div = aug[col * 2 * n + col];
        for j in 0..2 * n {
            aug[col * 2 * n + j] /= div;
        }
        for row in 0..n {
            if row == col {
                continue;
            }
            let factor = aug[row * 2 * n + col];
            for j in 0..2 * n {
                aug[row * 2 * n + j] -= factor * aug[col * 2 * n + j];
            }
        }
    }
    let mut out = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in 0..n {
            out[i][j] = aug[i * 2 * n + n + j];
        }
    }
    Ok(out)
}

#[derive(Debug, Clone)]
pub struct LogisticResult {
    pub coefficients: Vec<f64>,
    pub std_errors: Vec<f64>,
    pub z_stats: Vec<f64>,
    pub p_values: Vec<f64>,
    pub log_likelihood: f64,
    pub converged: bool,
    pub iterations: usize,
}

pub fn logistic(x: &[Vec<f64>], y: &[f64]) -> StatsResult<LogisticResult> {
    let n = y.len();
    if n == 0 || x.len() != n {
        return Err(StatsError::Error("logistic: length mismatch".into()));
    }
    let p = x[0].len();
    let cols = p + 1;
    let mut beta = vec![0.0; cols];
    let max_iter = 100;
    let tol = 1e-8;
    let mut converged = false;
    let mut iterations = 0;

    for iter in 0..max_iter {
        iterations = iter + 1;
        let mut gradient = vec![0.0; cols];
        let mut hessian = vec![vec![0.0; cols]; cols];
        let mut log_lik = 0.0;

        for i in 0..n {
            let mut eta = beta[0];
            for j in 0..p {
                eta += beta[j + 1] * x[i][j];
            }
            let mu = sigmoid(eta);
            let w = mu * (1.0 - mu);
            if w < 1e-15 {
                continue;
            }
            let resid = y[i] - mu;
            gradient[0] += resid;
            hessian[0][0] += w;
            for j in 0..p {
                let xj = x[i][j];
                gradient[j + 1] += resid * xj;
                hessian[0][j + 1] += w * xj;
                hessian[j + 1][0] = hessian[0][j + 1];
                for k in 0..p {
                    hessian[j + 1][k + 1] += w * xj * x[i][k];
                }
            }
            let yi = y[i].clamp(1e-15, 1.0 - 1e-15);
            log_lik += yi * mu.ln() + (1.0 - yi) * (1.0 - mu).ln();
        }

        let delta = solve_sym(&hessian, &gradient)?;
        let max_delta = delta.iter().map(|d| d.abs()).fold(0.0, f64::max);
        for j in 0..cols {
            beta[j] += delta[j];
        }
        if max_delta < tol {
            converged = true;
            break;
        }
    }

    if !converged {
        return Err(StatsError::NonConvergence(
            "logistic IRLS did not converge".into(),
        ));
    }

    // final Hessian for SE
    let mut hessian = vec![vec![0.0; cols]; cols];
    let mut log_lik = 0.0;
    for i in 0..n {
        let mut eta = beta[0];
        for j in 0..p {
            eta += beta[j + 1] * x[i][j];
        }
        let mu = sigmoid(eta);
        let w = mu * (1.0 - mu);
        hessian[0][0] += w;
        for j in 0..p {
            let xj = x[i][j];
            hessian[0][j + 1] += w * xj;
            hessian[j + 1][0] = hessian[0][j + 1];
            for k in 0..p {
                hessian[j + 1][k + 1] += w * xj * x[i][k];
            }
        }
        let yi = y[i].clamp(1e-15, 1.0 - 1e-15);
        log_lik += yi * mu.ln() + (1.0 - yi) * (1.0 - mu).ln();
    }
    let cov = invert_hessian(&hessian)?;
    let mut std_errors = Vec::with_capacity(cols);
    let mut z_stats = Vec::with_capacity(cols);
    let mut p_values = Vec::with_capacity(cols);
    for j in 0..cols {
        let se = cov[j][j].sqrt();
        std_errors.push(se);
        let z = if se > 0.0 { beta[j] / se } else { 0.0 };
        z_stats.push(z);
        let p = 2.0 * (1.0 - crate::special::norm_cdf(z.abs()));
        p_values.push(p);
    }

    Ok(LogisticResult {
        coefficients: beta,
        std_errors,
        z_stats,
        p_values,
        log_likelihood: log_lik,
        converged,
        iterations,
    })
}

fn sigmoid(x: f64) -> f64 {
    if x >= 0.0 {
        1.0 / (1.0 + (-x).exp())
    } else {
        let ex = x.exp();
        ex / (1.0 + ex)
    }
}

fn solve_sym(a: &[Vec<f64>], b: &[f64]) -> StatsResult<Vec<f64>> {
    let n = b.len();
    let flat: Vec<f64> = a.iter().flat_map(|row| row.iter().copied()).collect();
    let mat = from_slice(&[n, n], &flat).map_err(|e| StatsError::Error(e.to_string()))?;
    let rhs = from_slice(&[n, 1], b).map_err(|e| StatsError::Error(e.to_string()))?;
    let sol = niao_num::solve(&mat, &rhs).map_err(|e| StatsError::Error(e.to_string()))?;
    Ok(sol.to_vec())
}

fn invert_hessian(h: &[Vec<f64>]) -> StatsResult<Vec<Vec<f64>>> {
    let n = h.len();
    let flat: Vec<f64> = h.iter().flat_map(|row| row.iter().copied()).collect();
    let mat = from_slice(&[n, n], &flat).map_err(|e| StatsError::Error(e.to_string()))?;
    invert(&mat)
}

/// Confidence interval for a mean.
pub fn ci_mean(data: &[f64], confidence: f64) -> StatsResult<(f64, f64)> {
    if data.is_empty() {
        return Err(StatsError::Error("ci_mean: empty".into()));
    }
    let m = mean(data)?;
    let s = std(data, 1)?;
    let n = data.len() as f64;
    let alpha = 1.0 - confidence;
    let t = StudentT::new(n - 1.0)?.ppf(1.0 - alpha / 2.0)?;
    let margin = t * s / n.sqrt();
    Ok((m - margin, m + margin))
}

/// Wilson score interval for a proportion.
pub fn ci_proportion(successes: u64, trials: u64, confidence: f64) -> StatsResult<(f64, f64)> {
    if trials == 0 {
        return Err(StatsError::Error("ci_proportion: trials=0".into()));
    }
    let p = successes as f64 / trials as f64;
    let z = norm_ppf(1.0 - (1.0 - confidence) / 2.0)?;
    let z2 = z * z;
    let nf = trials as f64;
    let denom = 1.0 + z2 / nf;
    let center = (p + z2 / (2.0 * nf)) / denom;
    let margin = z * (p * (1.0 - p) / nf + z2 / (4.0 * nf * nf)).sqrt() / denom;
    Ok((center - margin, center + margin))
}

/// CI for difference of two means (Welch).
pub fn ci_diff_means(a: &[f64], b: &[f64], confidence: f64) -> StatsResult<(f64, f64)> {
    let ma = mean(a)?;
    let mb = mean(b)?;
    let va = crate::descriptive::var(a, 1)?;
    let vb = crate::descriptive::var(b, 1)?;
    let na = a.len() as f64;
    let nb = b.len() as f64;
    let se = (va / na + vb / nb).sqrt();
    let num = va / na + vb / nb;
    let den = (va / na).powi(2) / (na - 1.0) + (vb / nb).powi(2) / (nb - 1.0);
    let df = num / den;
    let alpha = 1.0 - confidence;
    let t = StudentT::new(df)?.ppf(1.0 - alpha / 2.0)?;
    let diff = ma - mb;
    Ok((diff - t * se, diff + t * se))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, rtol: f64) -> bool {
        (a - b).abs() <= rtol * b.abs().max(1e-12)
    }

    #[test]
    fn ols_simple() {
        // y = 1 + 2*x
        let x: Vec<Vec<f64>> = (0..20).map(|i| vec![i as f64]).collect();
        let y: Vec<f64> = (0..20).map(|i| 1.0 + 2.0 * i as f64).collect();
        let r = ols(&x, &y).unwrap();
        assert!(close(r.coefficients[0], 1.0, 1e-8));
        assert!(close(r.coefficients[1], 2.0, 1e-8));
        assert!(close(r.r_squared, 1.0, 1e-8));
    }
}
