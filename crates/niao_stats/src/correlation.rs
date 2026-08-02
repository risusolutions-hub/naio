//! Correlation and covariance.

use crate::descriptive::mean;
use crate::dist::StudentT;
use crate::error::{StatsError, StatsResult};

#[derive(Debug, Clone, Copy)]
pub struct CorrResult {
    pub statistic: f64,
    pub pvalue: f64,
}

pub fn pearsonr(x: &[f64], y: &[f64]) -> StatsResult<CorrResult> {
    if x.len() != y.len() || x.is_empty() {
        return Err(StatsError::Error(
            "pearsonr: length mismatch or empty".into(),
        ));
    }
    let mx = mean(x)?;
    let my = mean(y)?;
    let mut num = 0.0;
    let mut dx2 = 0.0;
    let mut dy2 = 0.0;
    for (&xi, &yi) in x.iter().zip(y.iter()) {
        let dx = xi - mx;
        let dy = yi - my;
        num += dx * dy;
        dx2 += dx * dx;
        dy2 += dy * dy;
    }
    if dx2 == 0.0 || dy2 == 0.0 {
        return Ok(CorrResult {
            statistic: if dx2 == 0.0 && dy2 == 0.0 { 1.0 } else { 0.0 },
            pvalue: 1.0,
        });
    }
    let r = num / (dx2 * dy2).sqrt();
    let n = x.len() as f64;
    let df = n - 2.0;
    let t = r * (df / (1.0 - r * r).max(1e-300)).sqrt();
    let p = 2.0 * StudentT::new(df)?.sf(t.abs())?;
    Ok(CorrResult {
        statistic: r,
        pvalue: p,
    })
}

pub fn spearmanr(x: &[f64], y: &[f64]) -> StatsResult<CorrResult> {
    if x.len() != y.len() || x.is_empty() {
        return Err(StatsError::Error("spearmanr: length mismatch".into()));
    }
    let rx = rank(x)?;
    let ry = rank(y)?;
    pearsonr(&rx, &ry)
}

pub fn kendalltau(x: &[f64], y: &[f64]) -> StatsResult<CorrResult> {
    if x.len() != y.len() || x.is_empty() {
        return Err(StatsError::Error("kendalltau: length mismatch".into()));
    }
    let n = x.len();
    let mut concordant = 0i64;
    let mut discordant = 0i64;
    for i in 0..n {
        for j in (i + 1)..n {
            let prod = (x[i] - x[j]) * (y[i] - y[j]);
            if prod > 0.0 {
                concordant += 1;
            } else if prod < 0.0 {
                discordant += 1;
            }
        }
    }
    let pairs = (n * (n - 1) / 2) as f64;
    let tau = (concordant - discordant) as f64 / pairs;
    // asymptotic p-value (two-sided, no ties correction)
    let var = 2.0 * (2.0 * n as f64 + 5.0) / (9.0 * n as f64 * (n as f64 - 1.0));
    let z = tau / var.sqrt();
    let p = 2.0 * (1.0 - crate::special::norm_cdf(z.abs()));
    Ok(CorrResult {
        statistic: tau,
        pvalue: p,
    })
}

pub fn cov(x: &[f64], y: &[f64], ddof: usize) -> StatsResult<f64> {
    if x.len() != y.len() || x.len() <= ddof {
        return Err(StatsError::Error("cov: length mismatch".into()));
    }
    let mx = mean(x)?;
    let my = mean(y)?;
    let s: f64 = x
        .iter()
        .zip(y.iter())
        .map(|(&xi, &yi)| (xi - mx) * (yi - my))
        .sum();
    Ok(s / (x.len() - ddof) as f64)
}

pub fn cov_matrix(data: &[Vec<f64>]) -> StatsResult<Vec<Vec<f64>>> {
    if data.is_empty() {
        return Err(StatsError::Error("cov_matrix: empty".into()));
    }
    let p = data.len();
    let n = data[0].len();
    for row in data {
        if row.len() != n {
            return Err(StatsError::Error("cov_matrix: ragged rows".into()));
        }
    }
    let mut out = vec![vec![0.0; p]; p];
    for i in 0..p {
        for j in i..p {
            let c = cov(&data[i], &data[j], 1)?;
            out[i][j] = c;
            out[j][i] = c;
        }
    }
    Ok(out)
}

fn rank(data: &[f64]) -> StatsResult<Vec<f64>> {
    let n = data.len();
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&a, &b| data[a].partial_cmp(&data[b]).unwrap());
    let mut ranks = vec![0.0; n];
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j + 1 < n && data[idx[j + 1]] == data[idx[i]] {
            j += 1;
        }
        let avg = (i + j + 2) as f64 / 2.0; // 1-based average rank
        for k in i..=j {
            ranks[idx[k]] = avg;
        }
        i = j + 1;
    }
    Ok(ranks)
}
